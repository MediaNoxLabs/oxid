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

#[cfg(any(target_os = "ios", target_os = "android"))]
const SCAN_POLL_INTERVAL: Duration = Duration::from_millis(100);
#[cfg(any(target_os = "ios", target_os = "android"))]
const SCAN_POLL_LIMIT: usize = 600;
// A second request must not replace a consent screen that the holder is
// already reviewing.
const IDENTITY_LINK_QUEUE_LIMIT: usize = 1;

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

/// Issue #124 / ADR-0103: the standalone-portal mobile suite must never let
/// the real, single-use Portal offer touch a host/device process argument, OS
/// intent/URL state, log, evidence file, or retained staging artifact.
/// `simctl openurl`/`am start -d` deliver only the fixed, non-secret
/// [`loopback_test_offer_trigger::TRIGGER`] string. A named worker fetches the
/// real offer over bounded loopback HTTP and enqueues it into the normal
/// one-item ingress; Tao/Wry's OS-event callback never performs network I/O.
/// Fetch/validation failure instead enqueues the literal trigger, which the
/// strict credential-offer router rejects as malformed. This is a single
/// literal trigger, not a generic command or URL-fetch channel.
#[cfg(feature = "loopback-test-offer-trigger")]
mod loopback_test_offer_trigger {
    use std::io::{Read, Write};
    use std::net::{SocketAddr, TcpStream};
    use std::time::{Duration, Instant};

    pub const TRIGGER: &str = "openid-credential-offer://standalone-portal-test-fetch";

    const CONTROL_TIMEOUT: Duration = Duration::from_secs(10);
    const MAX_RESPONSE_BYTES: usize = 32 * 1_024;

    pub fn is_trigger(value: &str) -> bool {
        value == TRIGGER
    }

    pub fn resolve_trigger() -> String {
        let control_address = SocketAddr::from(([127, 0, 0, 1], 18091));
        fetch_offer(control_address, CONTROL_TIMEOUT).unwrap_or_else(|()| TRIGGER.to_owned())
    }

    fn fetch_offer(address: SocketAddr, timeout: Duration) -> Result<String, ()> {
        let started = Instant::now();
        let mut stream = TcpStream::connect_timeout(&address, timeout).map_err(|_| ())?;
        set_remaining_timeouts(&stream, started, timeout)?;
        stream
            .write_all(b"GET /offer HTTP/1.1\r\nHost: 127.0.0.1:18091\r\nConnection: close\r\n\r\n")
            .map_err(|_| ())?;

        let mut buffer = Vec::new();
        let mut chunk = [0_u8; 4_096];
        loop {
            set_remaining_timeouts(&stream, started, timeout)?;
            let read = stream.read(&mut chunk).map_err(|_| ())?;
            if read == 0 {
                break;
            }
            buffer.extend_from_slice(&chunk[..read]);
            if buffer.len() > MAX_RESPONSE_BYTES {
                return Err(());
            }
        }
        let response = String::from_utf8(buffer).map_err(|_| ())?;
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

    fn parse_offer_response(response: &str) -> Option<String> {
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
        Some(body.to_owned())
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
            assert_eq!(parse_offer_response(&response), Some(offer.to_owned()));

            for rejected in [
                "HTTP/1.1 503 Service Unavailable\r\nContent-Length: 1\r\n\r\nx",
                "HTTP/1.1 2000 Not A Status\r\nContent-Length: 1\r\n\r\nx",
                "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n1\r\nx\r\n0\r\n\r\n",
                "HTTP/1.1 200 OK\r\nContent-Length: 2\r\n\r\nx",
                "HTTP/1.1 200 OK\r\nContent-Length: 0\r\n\r\n",
                "not an http response",
            ] {
                assert_eq!(parse_offer_response(rejected), None);
            }
            assert_eq!(
                parse_offer_response(&format!(
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\n\r\nx",
                    MAX_RESPONSE_BYTES + 1
                )),
                None
            );
        }

        #[test]
        fn loopback_fetch_accepts_only_the_fixed_bounded_http_exchange() {
            use std::net::TcpListener;

            let listener = TcpListener::bind(("127.0.0.1", 0)).expect("loopback listener");
            let address = listener.local_addr().expect("listener address");
            let server = std::thread::spawn(move || {
                let (mut stream, _) = listener.accept().expect("accept fetch");
                let mut request = [0_u8; 256];
                let _ = stream.read(&mut request).expect("read request");
                let offer = "openid-credential-offer://?credential_offer=abc";
                write!(
                    stream,
                    "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{offer}",
                    offer.len()
                )
                .expect("write response");
            });

            assert_eq!(
                fetch_offer(address, Duration::from_secs(1)),
                Ok("openid-credential-offer://?credential_offer=abc".to_owned())
            );
            server.join().expect("server thread");
        }

        #[test]
        fn loopback_fetch_has_one_closed_wall_clock_deadline() {
            use std::net::TcpListener;

            let listener = TcpListener::bind(("127.0.0.1", 0)).expect("loopback listener");
            let address = listener.local_addr().expect("listener address");
            let server = std::thread::spawn(move || {
                let (_stream, _) = listener.accept().expect("accept fetch");
                std::thread::sleep(Duration::from_millis(100));
            });
            let started = Instant::now();
            assert_eq!(fetch_offer(address, Duration::from_millis(20)), Err(()));
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
}

#[derive(Default)]
struct CapturedIdentityLinks {
    links: VecDeque<InboundIdentityLink>,
    #[cfg(feature = "loopback-test-offer-trigger")]
    trigger_fetch_in_flight: bool,
}

impl NativeIdentityLinkIngress {
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
        F: FnOnce() -> String + Send + 'static,
    {
        if !loopback_test_offer_trigger::is_trigger(&value) {
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
                let resolved = validated_trigger_result(resolver()).unwrap_or(fallback);
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
fn validated_trigger_result(value: String) -> Option<InboundIdentityLink> {
    let link = InboundIdentityLink::new(value).ok()?;
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
        return self
            .capture_with_trigger_resolver(value, loopback_test_offer_trigger::resolve_trigger);

        #[cfg(not(feature = "loopback-test-offer-trigger"))]
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
                if loopback_test_offer_trigger::is_trigger(&value) {
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

        let ingress = NativeIdentityLinkIngress::default();
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
                    "openid-credential-offer://?credential_offer=%7B%7D".to_owned()
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
            let ingress = NativeIdentityLinkIngress::default();
            ingress
                .capture_with_trigger_resolver(
                    loopback_test_offer_trigger::TRIGGER.to_owned(),
                    move || resolved,
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
