// SPDX-License-Identifier: Apache-2.0

#![forbid(unsafe_code)]

#[cfg(any(target_os = "ios", target_os = "android"))]
use std::time::Duration;
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};

#[cfg(target_os = "android")]
use oxid_adapter_mobile_native::take_identity_link_json;
#[cfg(any(target_os = "ios", target_os = "android"))]
use oxid_adapter_mobile_native::{
    NativeBridgeError, start_scan_json, take_scan_result_json, timeout_scan_json,
};
use oxid_platform_ports::{
    IdentityLinkIngressError, IdentityLinkIngressPort, InboundIdentityLink, QrScanError,
    QrScanFuture, QrScannerPort, ScannedQrPayload,
};
use oxid_protocol_application::{
    IdentityRequestKind, IdentityRequestRouterPort, IdentityRequestRoutingError,
};
#[cfg(any(target_os = "ios", target_os = "android", test))]
use serde::Deserialize;
use url::Url;
#[cfg(feature = "loopback-test-offer-trigger")]
use zeroize::Zeroizing;

#[cfg(any(target_os = "ios", target_os = "android"))]
const SCAN_POLL_INTERVAL: Duration = Duration::from_millis(100);
#[cfg(any(target_os = "ios", target_os = "android"))]
const SCAN_POLL_LIMIT: usize = 600;
// A second request must not replace a consent screen that the holder is
// already reviewing.
const IDENTITY_LINK_QUEUE_LIMIT: usize = 1;

/// Accepts only one canonical, explicit-port Tailscale HTTPS origin outside
/// the standalone routes reserved by Oxid.
pub fn validate_tailnet_public_origin(value: &str) -> Result<(), &'static str> {
    if value.len() > 512 {
        return Err("Portal tailnet public origin is invalid");
    }
    let url = Url::parse(value).map_err(|_| "Portal tailnet public origin is invalid")?;
    let host = url
        .host_str()
        .ok_or("Portal tailnet public origin is invalid")?;
    let labels_are_canonical = host.split('.').all(|label| {
        !label.is_empty()
            && label.len() <= 63
            && label
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
            && !label.starts_with('-')
            && !label.ends_with('-')
    });
    url.port()
        .filter(|port| *port >= 1024 && !matches!(*port, 8443 | 10_000))
        .ok_or("Portal tailnet public origin is invalid")?;
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.path() != "/"
        || url.query().is_some()
        || url.fragment().is_some()
        || !host.ends_with(".ts.net")
        || host == "ts.net"
        || !labels_are_canonical
        || url.origin().ascii_serialization() != value
    {
        return Err("Portal tailnet public origin is invalid");
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct RegisteredOpenId4VpRequest {
    client_id: String,
    request_uri: String,
}

/// Strict protocol-link classifier. `openid4vp` links are accepted only when
/// their client and request endpoints match an explicitly registered flow.
pub struct StrictIdentityRequestRouter {
    self_issued: Option<RegisteredOpenId4VpRequest>,
    presentation: Option<RegisteredOpenId4VpRequest>,
}

impl StrictIdentityRequestRouter {
    /// Routes credential offers but fails closed for ambiguous `openid4vp`
    /// links until production endpoint discovery supplies a registry.
    #[must_use]
    pub const fn credential_offers_only() -> Self {
        Self {
            self_issued: None,
            presentation: None,
        }
    }

    pub fn with_registered_openid4vp_requests(
        self_issued: &str,
        presentation: &str,
    ) -> Result<Self, IdentityRequestRoutingError> {
        let self_issued = registered_openid4vp_request(self_issued)?;
        let presentation = registered_openid4vp_request(presentation)?;
        if self_issued == presentation {
            return Err(IdentityRequestRoutingError::AmbiguousRequest);
        }
        Ok(Self {
            self_issued: Some(self_issued),
            presentation: Some(presentation),
        })
    }
}

impl IdentityRequestRouterPort for StrictIdentityRequestRouter {
    fn route(&self, request_uri: &str) -> Result<IdentityRequestKind, IdentityRequestRoutingError> {
        let parsed =
            Url::parse(request_uri).map_err(|_| IdentityRequestRoutingError::InvalidRequest)?;
        match parsed.scheme() {
            "openid-credential-offer" => {
                validate_credential_offer_route(&parsed)?;
                Ok(IdentityRequestKind::CredentialIssuance)
            }
            "openid4vp" => {
                let request = registered_openid4vp_request(request_uri)?;
                match (
                    self.self_issued.as_ref() == Some(&request),
                    self.presentation.as_ref() == Some(&request),
                ) {
                    (true, false) => Ok(IdentityRequestKind::SelfIssuedAuthentication),
                    (false, true) => Ok(IdentityRequestKind::CredentialPresentation),
                    (false, false) | (true, true) => {
                        Err(IdentityRequestRoutingError::AmbiguousRequest)
                    }
                }
            }
            _ => Err(IdentityRequestRoutingError::UnsupportedRequest),
        }
    }
}

fn validate_credential_offer_route(url: &Url) -> Result<(), IdentityRequestRoutingError> {
    if url.has_host()
        || !matches!(url.path(), "" | "/")
        || url.fragment().is_some()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some()
    {
        return Err(IdentityRequestRoutingError::InvalidRequest);
    }
    let mut embedded = 0_u8;
    let mut referenced = 0_u8;
    for (name, value) in url.query_pairs() {
        if value.is_empty() {
            return Err(IdentityRequestRoutingError::InvalidRequest);
        }
        match name.as_ref() {
            "credential_offer" => embedded = embedded.saturating_add(1),
            "credential_offer_uri" => referenced = referenced.saturating_add(1),
            _ => return Err(IdentityRequestRoutingError::InvalidRequest),
        }
    }
    if (embedded, referenced) == (1, 0) || (embedded, referenced) == (0, 1) {
        Ok(())
    } else {
        Err(IdentityRequestRoutingError::InvalidRequest)
    }
}

fn registered_openid4vp_request(
    request_uri: &str,
) -> Result<RegisteredOpenId4VpRequest, IdentityRequestRoutingError> {
    let url = Url::parse(request_uri).map_err(|_| IdentityRequestRoutingError::InvalidRequest)?;
    if url.scheme() != "openid4vp"
        || url.host_str() != Some("authorize")
        || !matches!(url.path(), "" | "/")
        || url.fragment().is_some()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some()
    {
        return Err(IdentityRequestRoutingError::InvalidRequest);
    }

    let mut client_id = None;
    let mut nested_request_uri = None;
    for (name, value) in url.query_pairs() {
        if value.is_empty() {
            return Err(IdentityRequestRoutingError::InvalidRequest);
        }
        let slot = match name.as_ref() {
            "client_id" => &mut client_id,
            "request_uri" => &mut nested_request_uri,
            _ => return Err(IdentityRequestRoutingError::InvalidRequest),
        };
        if slot.replace(value.into_owned()).is_some() {
            return Err(IdentityRequestRoutingError::InvalidRequest);
        }
    }

    Ok(RegisteredOpenId4VpRequest {
        client_id: client_id.ok_or(IdentityRequestRoutingError::InvalidRequest)?,
        request_uri: nested_request_uri.ok_or(IdentityRequestRoutingError::InvalidRequest)?,
    })
}

/// The standalone Portal handoff keeps the single-use offer out of OS link
/// state and accepts it only through an app-private capability.
/// `simctl openurl`/`am start -d` deliver only the fixed, non-secret
/// [`loopback_test_offer_trigger::TRIGGER`] string. The harness places a fresh
/// capability in app-private storage without argv; a named worker unlinks it,
/// authenticates one bounded loopback response, zeroizes it, and enqueues the
/// validated offer into the normal one-item ingress. Tao/Wry's OS-event
/// callback never performs network I/O. Failure instead enqueues the literal
/// trigger, which the strict credential-offer router rejects as malformed.
#[cfg(feature = "loopback-test-offer-trigger")]
mod loopback_test_offer_trigger {
    use std::fs;
    use std::io::{Read, Write};
    use std::net::{SocketAddr, TcpStream};
    use std::path::{Path, PathBuf};
    use std::time::{Duration, Instant};

    #[cfg(feature = "tailnet-test-offer-trigger")]
    use reqwest::{
        Certificate, Client,
        header::{AUTHORIZATION, CONTENT_LENGTH, HeaderValue},
        redirect::Policy,
    };
    #[cfg(unix)]
    use std::os::unix::fs::PermissionsExt as _;
    use zeroize::Zeroizing;

    pub const TRIGGER: &str = "openid-credential-offer://standalone-portal-test-fetch";

    const CONTROL_TIMEOUT: Duration = Duration::from_secs(10);
    const MAX_RESPONSE_BYTES: usize = 32 * 1_024;
    const CAPABILITY_BYTES: usize = 64;
    #[cfg(any(target_os = "ios", target_os = "android", test))]
    const CAPABILITY_FILE: &str = "portal-offer.capability";

    pub fn is_trigger(value: &str) -> bool {
        value == TRIGGER
    }

    pub fn resolve_trigger() -> Zeroizing<String> {
        let control_address = SocketAddr::from(([127, 0, 0, 1], 18091));
        read_capability()
            .and_then(|capability| fetch_offer(control_address, CONTROL_TIMEOUT, &capability))
            .unwrap_or_else(|()| Zeroizing::new(TRIGGER.to_owned()))
    }

    #[cfg(feature = "tailnet-test-offer-trigger")]
    pub fn resolve_tailnet_trigger(public_origin: &str) -> Zeroizing<String> {
        read_capability()
            .and_then(|capability| {
                let runtime = tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .map_err(|_| ())?;
                runtime.block_on(fetch_tailnet_offer(public_origin, &capability))
            })
            .unwrap_or_else(|()| Zeroizing::new(TRIGGER.to_owned()))
    }

    #[cfg(feature = "tailnet-test-offer-trigger")]
    async fn fetch_tailnet_offer(
        public_origin: &str,
        capability: &[u8],
    ) -> Result<Zeroizing<String>, ()> {
        super::validate_tailnet_public_origin(public_origin).map_err(|_| ())?;
        if capability.len() != CAPABILITY_BYTES {
            return Err(());
        }
        let _ = rustls::crypto::ring::default_provider().install_default();
        let roots = webpki_root_certs::TLS_SERVER_ROOT_CERTS
            .iter()
            .map(|certificate| Certificate::from_der(certificate.as_ref()))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|_| ())?;
        let client = Client::builder()
            .no_proxy()
            .redirect(Policy::none())
            .retry(reqwest::retry::never())
            .connect_timeout(Duration::from_secs(5))
            .timeout(CONTROL_TIMEOUT)
            .user_agent("oxid-portal-offer-handoff/0.1")
            .tls_certs_only(roots)
            .build()
            .map_err(|_| ())?;
        let mut header_bytes = Zeroizing::new(Vec::with_capacity(7 + capability.len()));
        header_bytes.extend_from_slice(b"Bearer ");
        header_bytes.extend_from_slice(capability);
        let mut authorization = HeaderValue::from_bytes(&header_bytes).map_err(|_| ())?;
        authorization.set_sensitive(true);
        let mut response = client
            .get(format!("{public_origin}/offer"))
            .header(AUTHORIZATION, authorization)
            .send()
            .await
            .map_err(|_| ())?;
        if response.status() != reqwest::StatusCode::OK {
            return Err(());
        }
        let expected_length = response
            .headers()
            .get(CONTENT_LENGTH)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse::<usize>().ok())
            .filter(|length| *length > 0 && *length <= MAX_RESPONSE_BYTES)
            .ok_or(())?;
        let mut body = Zeroizing::new(Vec::with_capacity(expected_length));
        while let Some(chunk) = response.chunk().await.map_err(|_| ())? {
            if body.len().saturating_add(chunk.len()) > MAX_RESPONSE_BYTES {
                return Err(());
            }
            body.extend_from_slice(&chunk);
        }
        if body.len() != expected_length {
            return Err(());
        }
        String::from_utf8(std::mem::take(&mut *body))
            .map(Zeroizing::new)
            .map_err(|_| ())
    }

    fn capability_path() -> Result<PathBuf, ()> {
        #[cfg(target_os = "ios")]
        {
            let home = std::env::var_os("HOME").ok_or(())?;
            return Ok(PathBuf::from(home)
                .join("Library/Application Support/io.medianox.oxid")
                .join(CAPABILITY_FILE));
        }
        #[cfg(target_os = "android")]
        {
            return Ok(PathBuf::from("/data/data/io.medianox.oxid/files").join(CAPABILITY_FILE));
        }
        #[cfg(not(any(target_os = "ios", target_os = "android")))]
        Err(())
    }

    fn read_capability() -> Result<Zeroizing<Vec<u8>>, ()> {
        read_capability_file(&capability_path()?)
    }

    fn read_capability_file(path: &Path) -> Result<Zeroizing<Vec<u8>>, ()> {
        let metadata = fs::symlink_metadata(path).map_err(|_| ())?;
        if !metadata.file_type().is_file() || metadata.file_type().is_symlink() {
            return Err(());
        }
        #[cfg(unix)]
        let owner_private = metadata.permissions().mode() & 0o077 == 0;
        #[cfg(not(unix))]
        let owner_private = true;
        if metadata.len() != CAPABILITY_BYTES as u64 || !owner_private {
            fs::remove_file(path).map_err(|_| ())?;
            return Err(());
        }
        let capability = match fs::read(path) {
            Ok(bytes) => Zeroizing::new(bytes),
            Err(_) => {
                let _ = fs::remove_file(path);
                return Err(());
            }
        };
        // A capability is burned before the network request. Failure to
        // unlink fails closed rather than leaving replayable app-private state.
        fs::remove_file(path).map_err(|_| ())?;
        if capability.len() != CAPABILITY_BYTES
            || !capability
                .iter()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(byte))
        {
            return Err(());
        }
        Ok(capability)
    }

    fn fetch_offer(
        address: SocketAddr,
        timeout: Duration,
        capability: &[u8],
    ) -> Result<Zeroizing<String>, ()> {
        if capability.len() != CAPABILITY_BYTES {
            return Err(());
        }
        let started = Instant::now();
        let mut stream = TcpStream::connect_timeout(&address, timeout).map_err(|_| ())?;
        set_remaining_timeouts(&stream, started, timeout)?;
        stream
            .write_all(b"GET /offer HTTP/1.1\r\nHost: 127.0.0.1:18091\r\nAuthorization: Bearer ")
            .map_err(|_| ())?;
        stream.write_all(capability).map_err(|_| ())?;
        stream
            .write_all(b"\r\nConnection: close\r\n\r\n")
            .map_err(|_| ())?;

        let mut buffer = Zeroizing::new(Vec::new());
        let mut chunk = Zeroizing::new([0_u8; 4_096]);
        loop {
            set_remaining_timeouts(&stream, started, timeout)?;
            let read = stream.read(&mut chunk[..]).map_err(|_| ())?;
            if read == 0 {
                break;
            }
            buffer.extend_from_slice(&chunk[..read]);
            if buffer.len() > MAX_RESPONSE_BYTES {
                return Err(());
            }
        }
        let response =
            Zeroizing::new(String::from_utf8(std::mem::take(&mut *buffer)).map_err(|_| ())?);
        parse_offer_response(&response).ok_or(())
    }

    fn set_remaining_timeouts(
        stream: &TcpStream,
        started: Instant,
        timeout: Duration,
    ) -> Result<(), ()> {
        let remaining = timeout.checked_sub(started.elapsed()).ok_or(())?;
        if remaining.is_zero() {
            return Err(());
        }
        stream.set_read_timeout(Some(remaining)).map_err(|_| ())?;
        stream.set_write_timeout(Some(remaining)).map_err(|_| ())
    }

    fn parse_offer_response(response: &str) -> Option<Zeroizing<String>> {
        let (head, body) = response.split_once("\r\n\r\n")?;
        let mut lines = head.split("\r\n");
        let mut status = lines.next()?.splitn(3, ' ');
        if status.next()? != "HTTP/1.1"
            || status.next()? != "200"
            || status.next().is_none_or(str::is_empty)
        {
            return None;
        }

        let mut content_length = None;
        for line in lines {
            let (name, value) = line.split_once(':')?;
            if name.eq_ignore_ascii_case("transfer-encoding") {
                return None;
            }
            if name.eq_ignore_ascii_case("content-length") {
                if content_length.is_some() {
                    return None;
                }
                content_length = Some(value.trim().parse::<usize>().ok()?);
            }
        }
        let content_length = content_length?;
        if content_length == 0
            || content_length > MAX_RESPONSE_BYTES
            || body.len() != content_length
        {
            return None;
        }
        Some(Zeroizing::new(body.to_owned()))
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn parses_only_an_exact_successful_bounded_body() {
            let offer = "openid-credential-offer://?credential_offer=abc";
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/plain\r\nContent-Length: {}\r\n\r\n{offer}",
                offer.len()
            );
            assert_eq!(
                parse_offer_response(&response)
                    .as_deref()
                    .map(String::as_str),
                Some(offer)
            );

            for rejected in [
                "HTTP/1.1 503 Service Unavailable\r\nContent-Length: 1\r\n\r\nx",
                "HTTP/1.1 2000 Not A Status\r\nContent-Length: 1\r\n\r\nx",
                "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n1\r\nx\r\n0\r\n\r\n",
                "HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nx",
                "HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n",
                "not an http response",
            ] {
                assert!(parse_offer_response(rejected).is_none());
            }
            assert!(
                parse_offer_response(&format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\nx",
                    MAX_RESPONSE_BYTES + 1
                ))
                .is_none()
            );
        }

        #[test]
        fn loopback_fetch_sends_the_capability_only_in_the_authorization_header() {
            use std::net::TcpListener;

            let capability = b"0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
            let listener = TcpListener::bind(("127.0.0.1", 0)).expect("loopback listener");
            let address = listener.local_addr().expect("listener address");
            let server = std::thread::spawn(move || {
                let (mut stream, _) = listener.accept().expect("accept fetch");
                let mut request = Vec::new();
                let mut chunk = [0_u8; 128];
                while !request.ends_with(b"\r\n\r\n") {
                    let read = stream.read(&mut chunk).expect("read request");
                    assert!(read > 0, "request ended before its headers");
                    request.extend_from_slice(&chunk[..read]);
                    assert!(request.len() <= 1_024, "request headers exceeded bound");
                }
                let expected = format!(
                    "GET /offer HTTP/1.1\r\nHost: 127.0.0.1:18091\r\nAuthorization: Bearer {}\r\nConnection: close\r\n\r\n",
                    String::from_utf8_lossy(capability)
                );
                assert_eq!(request, expected.as_bytes());
                let offer = "openid-credential-offer://?credential_offer=abc";
                write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{offer}",
                    offer.len()
                )
                .expect("write response");
            });

            let fetched =
                fetch_offer(address, Duration::from_secs(1), capability).expect("fetched offer");
            assert_eq!(
                fetched.as_str(),
                "openid-credential-offer://?credential_offer=abc"
            );
            server.join().expect("server thread");
        }

        #[test]
        fn app_private_capability_is_exact_owner_only_and_unlinked_before_use() {
            #[cfg(unix)]
            {
                let root = std::env::temp_dir().join(format!(
                    "oxid-portal-capability-{}-{}",
                    std::process::id(),
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .expect("clock")
                        .as_nanos()
                ));
                fs::create_dir(&root).expect("private root");
                fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).expect("root mode");
                let path = root.join(CAPABILITY_FILE);
                fs::write(
                    &path,
                    b"0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
                )
                .expect("capability");
                fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).expect("mode");
                let capability = read_capability_file(&path).expect("private capability");
                assert_eq!(capability.len(), CAPABILITY_BYTES);
                assert!(!path.exists());

                fs::write(&path, b"short").expect("short capability");
                fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).expect("mode");
                assert!(read_capability_file(&path).is_err());
                assert!(!path.exists(), "rejected capability must also be deleted");
                fs::remove_dir(&root).expect("remove root");
            }
        }

        #[test]
        fn loopback_fetch_has_one_closed_wall_clock_deadline() {
            use std::net::TcpListener;

            let capability = b"0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
            let listener = TcpListener::bind(("127.0.0.1", 0)).expect("loopback listener");
            let address = listener.local_addr().expect("listener address");
            let server = std::thread::spawn(move || {
                let (_stream, _) = listener.accept().expect("accept fetch");
                std::thread::sleep(Duration::from_millis(100));
            });
            let started = Instant::now();
            assert_eq!(
                fetch_offer(address, Duration::from_millis(20), capability),
                Err(())
            );
            assert!(started.elapsed() < Duration::from_secs(1));
            server.join().expect("server thread");
        }
    }
}
/// Native scanner backed by AVFoundation on iOS and Google Code Scanner on
/// Android. Other targets return `Unavailable` without attempting a bridge.
#[derive(Clone, Copy, Debug, Default)]
pub struct NativeQrScanner;

impl QrScannerPort for NativeQrScanner {
    fn scan<'a>(&'a self) -> QrScanFuture<'a> {
        Box::pin(async { scan_native().await })
    }
}

/// Bounded OS URL ingress. iOS/Tao events enter through `capture`; Android's
/// repository-owned activity queues VIEW intents in the static Kotlin bridge.
/// The explicit Portal test trigger reserves the sole queue slot while its
/// background worker performs bounded loopback retrieval.
#[derive(Default)]
pub struct NativeIdentityLinkIngress {
    captured: Arc<Mutex<CapturedIdentityLinks>>,
    #[cfg(feature = "loopback-test-offer-trigger")]
    resolve_loopback_test_offer_trigger: bool,
    #[cfg(feature = "tailnet-test-offer-trigger")]
    tailnet_public_origin: Option<String>,
}

#[derive(Default)]
struct CapturedIdentityLinks {
    links: VecDeque<InboundIdentityLink>,
    #[cfg(feature = "loopback-test-offer-trigger")]
    trigger_fetch_in_flight: bool,
}

impl NativeIdentityLinkIngress {
    #[cfg(feature = "loopback-test-offer-trigger")]
    #[must_use]
    pub fn standalone_portal_test() -> Self {
        Self {
            captured: Arc::new(Mutex::new(CapturedIdentityLinks::default())),
            resolve_loopback_test_offer_trigger: true,
            #[cfg(feature = "tailnet-test-offer-trigger")]
            tailnet_public_origin: None,
        }
    }

    #[cfg(feature = "tailnet-test-offer-trigger")]
    pub fn standalone_portal_tailnet(public_origin: &str) -> Result<Self, &'static str> {
        validate_tailnet_public_origin(public_origin)
            .map_err(|_| "Portal tailnet offer origin is invalid")?;
        Ok(Self {
            captured: Arc::new(Mutex::new(CapturedIdentityLinks::default())),
            resolve_loopback_test_offer_trigger: true,
            tailnet_public_origin: Some(public_origin.to_owned()),
        })
    }

    fn enqueue(&self, link: InboundIdentityLink) -> Result<(), IdentityLinkIngressError> {
        let mut captured = self
            .captured
            .lock()
            .map_err(|_| IdentityLinkIngressError::Failed)?;
        #[cfg(feature = "loopback-test-offer-trigger")]
        if captured.trigger_fetch_in_flight {
            return Err(IdentityLinkIngressError::QueueFull);
        }
        if captured.links.len() >= IDENTITY_LINK_QUEUE_LIMIT {
            return Err(IdentityLinkIngressError::QueueFull);
        }
        captured.links.push_back(link);
        Ok(())
    }

    #[cfg(feature = "loopback-test-offer-trigger")]
    fn capture_with_trigger_resolver<F>(
        &self,
        value: String,
        resolver: F,
    ) -> Result<(), IdentityLinkIngressError>
    where
        F: FnOnce() -> Zeroizing<String> + Send + 'static,
    {
        if !loopback_test_offer_trigger::is_trigger(&value) {
            return self.enqueue(InboundIdentityLink::new(value)?);
        }
        if !self.resolve_loopback_test_offer_trigger {
            return self.enqueue(InboundIdentityLink::new(value)?);
        }

        // Validate the fixed literal before reserving the one-item queue. The
        // worker validates its fetched result again before it can enqueue.
        let literal = InboundIdentityLink::new(value)?;
        {
            let mut captured = self
                .captured
                .lock()
                .map_err(|_| IdentityLinkIngressError::Failed)?;
            if captured.trigger_fetch_in_flight || captured.links.len() >= IDENTITY_LINK_QUEUE_LIMIT
            {
                return Err(IdentityLinkIngressError::QueueFull);
            }
            captured.trigger_fetch_in_flight = true;
        }

        let captured = Arc::clone(&self.captured);
        let fallback = literal.clone();
        let worker = std::thread::Builder::new()
            .name("oxid-portal-offer-fetch".to_owned())
            .spawn(move || {
                let resolved = resolver();
                let resolved = validated_trigger_result(&resolved).unwrap_or(fallback);
                if let Ok(mut captured) = captured.lock() {
                    captured.trigger_fetch_in_flight = false;
                    if captured.links.is_empty() {
                        captured.links.push_back(resolved);
                    }
                }
            });
        if worker.is_err() {
            let mut captured = self
                .captured
                .lock()
                .map_err(|_| IdentityLinkIngressError::Failed)?;
            captured.trigger_fetch_in_flight = false;
            if captured.links.is_empty() {
                captured.links.push_back(literal);
            }
        }
        Ok(())
    }
}

#[cfg(feature = "loopback-test-offer-trigger")]
fn validated_trigger_result(value: &str) -> Option<InboundIdentityLink> {
    let link = InboundIdentityLink::new(value.to_owned()).ok()?;
    let route = StrictIdentityRequestRouter::credential_offers_only()
        .route(&link.clone().into_inner())
        .ok()?;
    if route == IdentityRequestKind::CredentialIssuance {
        Some(link)
    } else {
        None
    }
}

impl IdentityLinkIngressPort for NativeIdentityLinkIngress {
    fn capture(&self, value: String) -> Result<(), IdentityLinkIngressError> {
        #[cfg(feature = "loopback-test-offer-trigger")]
        if self.resolve_loopback_test_offer_trigger {
            #[cfg(feature = "tailnet-test-offer-trigger")]
            if let Some(public_origin) = self.tailnet_public_origin.clone() {
                return self.capture_with_trigger_resolver(value, move || {
                    loopback_test_offer_trigger::resolve_tailnet_trigger(&public_origin)
                });
            }
            return self.capture_with_trigger_resolver(
                value,
                loopback_test_offer_trigger::resolve_trigger,
            );
        }

        self.enqueue(InboundIdentityLink::new(value)?)
    }

    fn take_pending(&self) -> Result<Option<InboundIdentityLink>, IdentityLinkIngressError> {
        #[cfg(feature = "loopback-test-offer-trigger")]
        let trigger_fetch_in_flight;
        {
            let mut captured = self
                .captured
                .lock()
                .map_err(|_| IdentityLinkIngressError::Failed)?;
            if !captured.links.is_empty() {
                // A direct Tao event and Android's native Activity bridge may
                // observe the same or concurrent VIEW intents. Keep the first
                // Rust item queued while draining/rejecting any native second
                // item so there is never a two-consent handoff.
                #[cfg(all(feature = "loopback-test-offer-trigger", target_os = "android"))]
                if take_native_identity_link()?.is_some() {
                    return Err(IdentityLinkIngressError::QueueFull);
                }
                return Ok(captured.links.pop_front());
            }
            #[cfg(feature = "loopback-test-offer-trigger")]
            {
                trigger_fetch_in_flight = captured.trigger_fetch_in_flight;
            }
        }

        #[cfg(feature = "loopback-test-offer-trigger")]
        if trigger_fetch_in_flight {
            // Android's native bridge has its own one-item handoff. Drain and
            // reject any second VIEW intent while the Rust queue slot is
            // reserved so the two layers cannot retain two consent requests.
            #[cfg(target_os = "android")]
            if take_native_identity_link()?.is_some() {
                return Err(IdentityLinkIngressError::QueueFull);
            }
            return Ok(None);
        }

        let native = take_native_identity_link()?;
        #[cfg(feature = "loopback-test-offer-trigger")]
        return match native {
            Some(link) => {
                let value = link.into_inner();
                if self.resolve_loopback_test_offer_trigger
                    && loopback_test_offer_trigger::is_trigger(&value)
                {
                    self.capture(value)?;
                    Ok(None)
                } else {
                    Ok(Some(InboundIdentityLink::new(value)?))
                }
            }
            None => Ok(None),
        };

        #[cfg(not(feature = "loopback-test-offer-trigger"))]
        Ok(native)
    }
}

#[cfg(target_os = "android")]
fn take_native_identity_link() -> Result<Option<InboundIdentityLink>, IdentityLinkIngressError> {
    let response = take_identity_link_json().map_err(map_identity_link_bridge_error)?;
    let status: NativeIdentityLinkStatus =
        serde_json::from_str(&response).map_err(|_| IdentityLinkIngressError::Failed)?;
    match status.status.as_str() {
        "empty" => Ok(None),
        "succeeded" => InboundIdentityLink::new(
            status
                .payload
                .ok_or(IdentityLinkIngressError::InvalidLink)?,
        )
        .map(Some),
        "invalid" => Err(IdentityLinkIngressError::InvalidLink),
        "queue_full" => Err(IdentityLinkIngressError::QueueFull),
        "unavailable" => Err(IdentityLinkIngressError::Unavailable),
        _ => Err(IdentityLinkIngressError::Failed),
    }
}

#[cfg(not(target_os = "android"))]
fn take_native_identity_link() -> Result<Option<InboundIdentityLink>, IdentityLinkIngressError> {
    Ok(None)
}

#[cfg(any(target_os = "ios", target_os = "android"))]
async fn scan_native() -> Result<ScannedQrPayload, QrScanError> {
    let started = start_scan_json().map_err(map_qr_bridge_error)?;
    decode_scan_start(&started)?;

    for _ in 0..SCAN_POLL_LIMIT {
        tokio::time::sleep(SCAN_POLL_INTERVAL).await;
        let response = take_scan_result_json().map_err(map_qr_bridge_error)?;
        if let Some(payload) = decode_scan_poll(&response)? {
            return Ok(payload);
        }
    }

    // The timeout belongs to the Rust port contract, but the native
    // coordinator must acknowledge it so a stale capture cannot occupy the
    // one-scanner slot or deliver into a later request.
    let timed_out = timeout_scan_json().map_err(map_qr_bridge_error)?;
    match decode_scan_poll(&timed_out) {
        Ok(Some(payload)) => Ok(payload),
        Err(QrScanError::TimedOut) => Err(QrScanError::TimedOut),
        Err(error) => Err(error),
        _ => Err(QrScanError::Failed),
    }
}

#[cfg(not(any(target_os = "ios", target_os = "android")))]
async fn scan_native() -> Result<ScannedQrPayload, QrScanError> {
    Err(QrScanError::Unavailable)
}

#[cfg(any(target_os = "ios", target_os = "android", test))]
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NativeScanStatus {
    status: String,
    #[serde(default)]
    payload: Option<String>,
}

#[cfg(any(target_os = "ios", target_os = "android", test))]
fn decode_scan_start(response: &str) -> Result<(), QrScanError> {
    let status: NativeScanStatus =
        serde_json::from_str(response).map_err(|_| QrScanError::Failed)?;
    if status.payload.is_some() {
        return Err(QrScanError::Failed);
    }
    if status.status == "scanning" {
        Ok(())
    } else {
        Err(map_native_status(&status.status))
    }
}

#[cfg(any(target_os = "ios", target_os = "android", test))]
fn decode_scan_poll(response: &str) -> Result<Option<ScannedQrPayload>, QrScanError> {
    let status: NativeScanStatus =
        serde_json::from_str(response).map_err(|_| QrScanError::Failed)?;
    match (status.status.as_str(), status.payload) {
        ("scanning", None) => Ok(None),
        ("succeeded", Some(payload)) => ScannedQrPayload::new(payload).map(Some),
        ("cancelled" | "denied" | "unavailable" | "timed_out" | "invalid" | "failed", None) => {
            Err(map_native_status(&status.status))
        }
        _ => Err(QrScanError::Failed),
    }
}

#[cfg(target_os = "android")]
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NativeIdentityLinkStatus {
    status: String,
    #[serde(default)]
    payload: Option<String>,
}

#[cfg(any(target_os = "ios", target_os = "android", test))]
fn map_native_status(status: &str) -> QrScanError {
    match status {
        "cancelled" => QrScanError::Cancelled,
        "denied" => QrScanError::Denied,
        "unavailable" => QrScanError::Unavailable,
        "timed_out" => QrScanError::TimedOut,
        "invalid" => QrScanError::InvalidPayload,
        _ => QrScanError::Failed,
    }
}

#[cfg(any(target_os = "ios", target_os = "android"))]
const fn map_qr_bridge_error(error: NativeBridgeError) -> QrScanError {
    match error {
        NativeBridgeError::Unavailable => QrScanError::Unavailable,
        NativeBridgeError::Failed => QrScanError::Failed,
    }
}

#[cfg(target_os = "android")]
const fn map_identity_link_bridge_error(error: NativeBridgeError) -> IdentityLinkIngressError {
    match error {
        NativeBridgeError::Unavailable => IdentityLinkIngressError::Unavailable,
        NativeBridgeError::Failed => IdentityLinkIngressError::Failed,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const LOGIN: &str = "openid4vp://authorize?client_id=http%3A%2F%2F127.0.0.1%3A32192%2Fverifier&request_uri=http%3A%2F%2F127.0.0.1%3A32192%2Fverifier%2Frequest";
    const PRESENTATION: &str = "openid4vp://authorize?client_id=http%3A%2F%2F127.0.0.1%3A32193%2Fverifier&request_uri=http%3A%2F%2F127.0.0.1%3A32193%2Fverifier%2Frequest";

    fn router() -> StrictIdentityRequestRouter {
        StrictIdentityRequestRouter::with_registered_openid4vp_requests(LOGIN, PRESENTATION)
            .expect("registry")
    }

    #[test]
    fn routes_each_supported_identity_request_without_reading_secrets() {
        assert_eq!(
            router().route("openid-credential-offer://?credential_offer=%7B%7D"),
            Ok(IdentityRequestKind::CredentialIssuance)
        );
        assert_eq!(
            router().route(LOGIN),
            Ok(IdentityRequestKind::SelfIssuedAuthentication)
        );
        assert_eq!(
            router().route(PRESENTATION),
            Ok(IdentityRequestKind::CredentialPresentation)
        );
    }

    #[test]
    fn routes_androids_empty_authority_offer_serialization() {
        assert_eq!(
            router().route("openid-credential-offer:///?credential_offer=%7B%7D"),
            Ok(IdentityRequestKind::CredentialIssuance)
        );
        assert_eq!(
            router().route("openid-credential-offer:///unexpected?credential_offer=%7B%7D"),
            Err(IdentityRequestRoutingError::InvalidRequest)
        );
    }

    #[test]
    fn fails_closed_for_unknown_or_smuggled_openid4vp_links() {
        assert_eq!(
            router().route("openid4vp://authorize?client_id=https%3A%2F%2Funknown.example&request_uri=https%3A%2F%2Funknown.example%2Frequest"),
            Err(IdentityRequestRoutingError::AmbiguousRequest)
        );
        assert_eq!(
            router().route(&format!(
                "{LOGIN}&request_uri=https%3A%2F%2Fattacker.example"
            )),
            Err(IdentityRequestRoutingError::InvalidRequest)
        );
    }

    #[test]
    fn rejects_offer_parameter_smuggling() {
        assert_eq!(
            router().route("openid-credential-offer://?credential_offer=%7B%7D&credential_offer_uri=https%3A%2F%2Fissuer.example"),
            Err(IdentityRequestRoutingError::InvalidRequest)
        );
    }

    #[test]
    fn native_link_ingress_retains_one_consent_request_and_fails_closed() {
        let ingress = NativeIdentityLinkIngress::default();
        ingress
            .capture("openid-credential-offer://?credential_offer=%7B%7D".to_owned())
            .expect("first link");
        assert_eq!(
            ingress.capture(LOGIN.to_owned()),
            Err(IdentityLinkIngressError::QueueFull)
        );

        let first = ingress
            .take_pending()
            .expect("take first")
            .expect("first pending");
        assert!(first.into_inner().starts_with("openid-credential-offer"));
        ingress.capture(LOGIN.to_owned()).expect("next link");
        let second = ingress
            .take_pending()
            .expect("take second")
            .expect("second pending");
        assert_eq!(second.into_inner(), LOGIN);
        assert_eq!(ingress.take_pending(), Ok(None));

        ingress
            .capture("openid4vp://authorize?slot=0".to_owned())
            .expect("queue slot");
        assert_eq!(
            ingress.capture("openid4vp://authorize?slot=overflow".to_owned()),
            Err(IdentityLinkIngressError::QueueFull)
        );
        assert_eq!(
            ingress.capture(" openid4vp://authorize".to_owned()),
            Err(IdentityLinkIngressError::InvalidLink)
        );
    }

    #[cfg(feature = "tailnet-test-offer-trigger")]
    #[test]
    fn tailnet_offer_profile_accepts_only_exact_magic_dns_https_origin() {
        for origin in [
            "https://oxid-demo.tail1234.ts.net:9443",
            "https://oxid-demo.tail1234.ts.net:12001",
        ] {
            assert!(NativeIdentityLinkIngress::standalone_portal_tailnet(origin).is_ok());
        }
        for invalid in [
            "http://oxid-demo.tail1234.ts.net:9443",
            "https://oxid-demo.tail1234.ts.net",
            "https://oxid-demo.tail1234.ts.net:443",
            "https://oxid-demo.tail1234.ts.net:8443",
            "https://oxid-demo.tail1234.ts.net:10000",
            "https://oxid-demo.tail1234.ts.net:9443/offer",
            "https://127.0.0.1:9443",
            "https://oxid.example:9443",
        ] {
            assert!(
                NativeIdentityLinkIngress::standalone_portal_tailnet(invalid).is_err(),
                "{invalid}"
            );
        }
    }

    #[cfg(feature = "loopback-test-offer-trigger")]
    #[test]
    fn trigger_resolution_requires_the_explicit_portal_constructor() {
        let ingress = NativeIdentityLinkIngress::default();
        ingress
            .capture(loopback_test_offer_trigger::TRIGGER.to_owned())
            .expect("capture literal");
        let literal = ingress
            .take_pending()
            .expect("take literal")
            .expect("literal pending")
            .into_inner();
        assert_eq!(literal, loopback_test_offer_trigger::TRIGGER);
        assert_eq!(
            router().route(&literal),
            Err(IdentityRequestRoutingError::InvalidRequest)
        );
    }

    #[cfg(feature = "loopback-test-offer-trigger")]
    fn wait_for_pending(ingress: &NativeIdentityLinkIngress) -> InboundIdentityLink {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
        loop {
            if let Some(link) = ingress.take_pending().expect("take pending") {
                return link;
            }
            assert!(
                std::time::Instant::now() < deadline,
                "trigger worker did not enqueue a result"
            );
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
    }

    #[cfg(feature = "loopback-test-offer-trigger")]
    #[test]
    fn portal_trigger_worker_never_blocks_capture_and_reserves_the_only_queue_slot() {
        use std::sync::mpsc;

        let ingress = NativeIdentityLinkIngress::standalone_portal_test();
        let (started_tx, started_rx) = mpsc::sync_channel(1);
        let (release_tx, release_rx) = mpsc::sync_channel(1);
        let started_at = std::time::Instant::now();
        ingress
            .capture_with_trigger_resolver(
                loopback_test_offer_trigger::TRIGGER.to_owned(),
                move || {
                    started_tx
                        .send(std::thread::current().name().map(str::to_owned))
                        .expect("signal worker start");
                    release_rx.recv().expect("release worker");
                    Zeroizing::new("openid-credential-offer://?credential_offer=%7B%7D".to_owned())
                },
            )
            .expect("schedule trigger");
        assert!(started_at.elapsed() < std::time::Duration::from_secs(1));
        assert_eq!(
            started_rx
                .recv_timeout(std::time::Duration::from_secs(1))
                .expect("named worker started")
                .as_deref(),
            Some("oxid-portal-offer-fetch")
        );
        assert_eq!(
            ingress.capture(LOGIN.to_owned()),
            Err(IdentityLinkIngressError::QueueFull)
        );
        assert_eq!(ingress.take_pending(), Ok(None));

        release_tx.send(()).expect("release worker");
        assert_eq!(
            wait_for_pending(&ingress).into_inner(),
            "openid-credential-offer://?credential_offer=%7B%7D"
        );
        assert_eq!(ingress.take_pending(), Ok(None));
    }

    #[cfg(feature = "loopback-test-offer-trigger")]
    #[test]
    fn portal_trigger_failure_and_invalid_fetch_fail_closed_to_the_literal() {
        for resolved in [
            loopback_test_offer_trigger::TRIGGER.to_owned(),
            String::new(),
            "x".repeat(32 * 1_024 + 1),
            LOGIN.to_owned(),
        ] {
            let ingress = NativeIdentityLinkIngress::standalone_portal_test();
            ingress
                .capture_with_trigger_resolver(
                    loopback_test_offer_trigger::TRIGGER.to_owned(),
                    move || Zeroizing::new(resolved),
                )
                .expect("schedule trigger");
            let literal = wait_for_pending(&ingress).into_inner();
            assert_eq!(literal, loopback_test_offer_trigger::TRIGGER);
            assert_eq!(
                router().route(&literal),
                Err(IdentityRequestRoutingError::InvalidRequest)
            );
        }
    }

    #[test]
    fn native_scan_statuses_are_closed_payload_free_and_exact() {
        assert_eq!(decode_scan_start(r#"{"status":"scanning"}"#), Ok(()));
        assert_eq!(decode_scan_poll(r#"{"status":"scanning"}"#), Ok(None));
        assert_eq!(
            decode_scan_poll(r#"{"status":"succeeded","payload":"openid4vp://authorize"}"#)
                .map(|payload| payload.map(ScannedQrPayload::into_inner)),
            Ok(Some("openid4vp://authorize".to_owned()))
        );

        for (status, expected) in [
            ("cancelled", QrScanError::Cancelled),
            ("denied", QrScanError::Denied),
            ("unavailable", QrScanError::Unavailable),
            ("timed_out", QrScanError::TimedOut),
            ("invalid", QrScanError::InvalidPayload),
            ("failed", QrScanError::Failed),
        ] {
            assert_eq!(
                decode_scan_poll(&format!(r#"{{"status":"{status}"}}"#)),
                Err(expected)
            );
        }

        assert_eq!(
            decode_scan_start(r#"{"status":"scanning","payload":"must-not-cross"}"#),
            Err(QrScanError::Failed)
        );
        assert_eq!(
            decode_scan_poll(r#"{"status":"cancelled","payload":"must-not-cross"}"#),
            Err(QrScanError::Failed)
        );
        assert_eq!(
            decode_scan_poll(r#"{"status":"new-native-status"}"#),
            Err(QrScanError::Failed)
        );
        assert_eq!(
            decode_scan_poll(r#"{"status":"failed","detail":"native error"}"#),
            Err(QrScanError::Failed)
        );
    }

    #[test]
    fn native_scan_success_rejects_empty_and_oversized_payloads() {
        assert_eq!(
            decode_scan_poll(r#"{"status":"succeeded","payload":""}"#),
            Err(QrScanError::InvalidPayload)
        );
        let oversized = serde_json::json!({
            "status": "succeeded",
            "payload": "x".repeat(32 * 1_024 + 1),
        });
        assert_eq!(
            decode_scan_poll(&oversized.to_string()),
            Err(QrScanError::InvalidPayload)
        );
    }
}
