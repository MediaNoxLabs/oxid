// SPDX-License-Identifier: Apache-2.0

//! Bounded loopback HTTP adapter for the development-only standalone faucet.

use std::{
    io::{self, Read as _, Write as _},
    net::{Shutdown, SocketAddr, TcpListener, TcpStream},
    time::Duration,
};

use serde_json::{Value, json};

use crate::faucet::StandaloneFaucet;

pub const DEFAULT_HTTP_ADDRESS: &str = "127.0.0.1:36301";

const MAX_BODY_BYTES: usize = 4 * 1024;
const MAX_HEADER_BYTES: usize = 8 * 1024;
const IO_TIMEOUT: Duration = Duration::from_secs(10);
const JSON_CONTENT_TYPE: &str = "application/json";

pub fn run_loopback_http(
    faucet: &mut StandaloneFaucet,
    address: &str,
    setup_svg: Option<&[u8]>,
) -> Result<(), HttpServerError> {
    let address = address
        .parse::<SocketAddr>()
        .map_err(|_| HttpServerError::InvalidAddress)?;
    if !address.ip().is_loopback() {
        return Err(HttpServerError::NonLoopbackAddress);
    }
    let listener = TcpListener::bind(address).map_err(HttpServerError::Io)?;
    eprintln!("Standalone faucet HTTP ready on {address}; loopback only.");
    for connection in listener.incoming() {
        let mut connection = connection.map_err(HttpServerError::Io)?;
        // A client disconnect must not terminate the development authority.
        // The request is independently framed and the connection is discarded.
        let _ = serve_connection(&mut connection, faucet, setup_svg);
    }
    Ok(())
}

#[derive(Debug)]
pub enum HttpServerError {
    InvalidAddress,
    NonLoopbackAddress,
    Io(io::Error),
}

impl std::fmt::Display for HttpServerError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidAddress => formatter.write_str("HTTP address is invalid"),
            Self::NonLoopbackAddress => formatter.write_str("HTTP address must be loopback"),
            Self::Io(_) => formatter.write_str("loopback HTTP I/O failed"),
        }
    }
}

impl std::error::Error for HttpServerError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            _ => None,
        }
    }
}

fn serve_connection(
    connection: &mut TcpStream,
    faucet: &mut StandaloneFaucet,
    setup_svg: Option<&[u8]>,
) -> io::Result<()> {
    connection.set_read_timeout(Some(IO_TIMEOUT))?;
    connection.set_write_timeout(Some(IO_TIMEOUT))?;
    let response = match read_request(connection) {
        Ok(request) => handle(faucet, request, setup_svg),
        Err(error) => {
            // Discard any unread oversized/malformed input so closing the
            // connection does not replace the closed JSON error with a reset.
            let _ = connection.shutdown(Shutdown::Read);
            error.into_response()
        }
    };
    write_response(connection, response)
}

struct HttpRequest {
    method: String,
    path: String,
    content_type: Option<String>,
    body: Vec<u8>,
}

struct HttpResponse {
    status: u16,
    content_type: &'static str,
    body: Vec<u8>,
}

impl HttpResponse {
    fn json(status: u16, body: Value) -> Self {
        Self {
            status,
            content_type: "application/json",
            body: serde_json::to_vec(&body).expect("faucet responses are serializable"),
        }
    }

    fn html(body: &'static str) -> Self {
        Self {
            status: 200,
            content_type: "text/html; charset=utf-8",
            body: body.as_bytes().to_vec(),
        }
    }
}

fn read_request(connection: &mut TcpStream) -> Result<HttpRequest, RequestError> {
    let header_bytes = read_headers(connection)?;
    let header_text = std::str::from_utf8(&header_bytes)
        .map_err(|_| RequestError::bad_request("headers must be UTF-8"))?;
    let mut lines = header_text
        .strip_suffix("\r\n\r\n")
        .ok_or_else(|| RequestError::bad_request("headers are incomplete"))?
        .split("\r\n");
    let request_line = lines
        .next()
        .ok_or_else(|| RequestError::bad_request("request line is missing"))?;
    let fields = request_line.split(' ').collect::<Vec<_>>();
    if fields.len() != 3 || fields[2] != "HTTP/1.1" || fields.iter().any(|field| field.is_empty()) {
        return Err(RequestError::bad_request("request line must use HTTP/1.1"));
    }

    let mut content_length = None;
    let mut content_type = None;
    let mut host_count = 0_u8;
    for line in lines {
        if line.starts_with(' ') || line.starts_with('\t') {
            return Err(RequestError::bad_request(
                "folded headers are not supported",
            ));
        }
        let (name, value) = line
            .split_once(':')
            .ok_or_else(|| RequestError::bad_request("header is malformed"))?;
        if name.is_empty()
            || !name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        {
            return Err(RequestError::bad_request("header name is invalid"));
        }
        let value = value.trim();
        if name.eq_ignore_ascii_case("host") {
            host_count = host_count.saturating_add(1);
        } else if name.eq_ignore_ascii_case("content-length") {
            if content_length.is_some() {
                return Err(RequestError::bad_request(
                    "duplicate content length is not supported",
                ));
            }
            content_length = Some(
                value
                    .parse::<usize>()
                    .map_err(|_| RequestError::bad_request("content length is invalid"))?,
            );
        } else if name.eq_ignore_ascii_case("content-type") {
            if content_type.replace(value.to_owned()).is_some() {
                return Err(RequestError::bad_request(
                    "duplicate content type is not supported",
                ));
            }
        } else if name.eq_ignore_ascii_case("transfer-encoding")
            || name.eq_ignore_ascii_case("expect")
        {
            return Err(RequestError::bad_request(
                "streaming request features are not supported",
            ));
        }
    }
    if host_count != 1 {
        return Err(RequestError::bad_request(
            "exactly one Host header is required",
        ));
    }

    let content_length = content_length.unwrap_or_default();
    if content_length > MAX_BODY_BYTES {
        return Err(RequestError::new(
            413,
            "request_too_large",
            "request exceeds 4096 bytes",
        ));
    }
    let mut body = vec![0; content_length];
    connection
        .read_exact(&mut body)
        .map_err(|_| RequestError::bad_request("request body is incomplete"))?;
    Ok(HttpRequest {
        method: fields[0].to_owned(),
        path: fields[1].to_owned(),
        content_type,
        body,
    })
}

fn read_headers(reader: &mut impl io::Read) -> Result<Vec<u8>, RequestError> {
    let mut bytes = Vec::with_capacity(512);
    loop {
        let mut byte = [0_u8; 1];
        reader
            .read_exact(&mut byte)
            .map_err(|_| RequestError::bad_request("headers are incomplete"))?;
        bytes.push(byte[0]);
        if bytes.ends_with(b"\r\n\r\n") {
            return Ok(bytes);
        }
        if bytes.len() > MAX_HEADER_BYTES {
            drain_header_tail(reader);
            return Err(RequestError::new(
                431,
                "headers_too_large",
                "request headers exceed 8192 bytes",
            ));
        }
    }
}

fn drain_header_tail(reader: &mut impl io::Read) {
    let mut tail = [0_u8; 4];
    for index in 0..MAX_HEADER_BYTES {
        let mut byte = [0_u8; 1];
        if reader.read_exact(&mut byte).is_err() {
            return;
        }
        tail.rotate_left(1);
        tail[3] = byte[0];
        if index >= 3 && tail == *b"\r\n\r\n" {
            return;
        }
    }
}

fn handle(
    faucet: &mut StandaloneFaucet,
    request: HttpRequest,
    setup_svg: Option<&[u8]>,
) -> HttpResponse {
    match (request.method.as_str(), request.path.as_str()) {
        ("GET", "/") if request.body.is_empty() => HttpResponse::html(DISCOVERY_PAGE),
        ("GET", "/setup.svg") if request.body.is_empty() && setup_svg.is_some() => HttpResponse {
            status: 200,
            content_type: "image/svg+xml",
            body: setup_svg.expect("guarded setup asset").to_vec(),
        },
        ("GET", "/setup.svg") if request.body.is_empty() => {
            rejected(404, "not_found", "setup QR is not configured")
        }
        ("GET", "/health") if request.body.is_empty() => {
            HttpResponse::json(200, StandaloneFaucet::health_http())
        }
        ("GET", "/" | "/health" | "/setup.svg") => {
            rejected(400, "invalid_request", "GET request body must be empty")
        }
        ("POST", "/fund") => fund(faucet, request.content_type.as_deref(), &request.body),
        (_, "/" | "/health" | "/fund" | "/setup.svg") => rejected(
            405,
            "method_not_allowed",
            "method is not supported for this path",
        ),
        _ => rejected(404, "not_found", "path is not supported"),
    }
}

fn fund(faucet: &mut StandaloneFaucet, content_type: Option<&str>, body: &[u8]) -> HttpResponse {
    let media_type = content_type
        .and_then(|value| value.split(';').next())
        .map(str::trim);
    if !media_type.is_some_and(|value| value.eq_ignore_ascii_case(JSON_CONTENT_TYPE)) {
        return rejected(
            415,
            "unsupported_media_type",
            "content type must be application/json",
        );
    }
    let params = match serde_json::from_slice(body) {
        Ok(params) => params,
        Err(_) => return rejected(400, "invalid_request", "request body must be JSON"),
    };
    let body = faucet.fund_http(params);
    let status = match body["error"]["code"].as_str() {
        None => 200,
        Some("idempotency_conflict") => 409,
        Some("authority_not_ready" | "unavailable") => 503,
        Some("outcome_unknown") => 504,
        Some(_) => 400,
    };
    HttpResponse::json(status, body)
}

fn write_response(connection: &mut TcpStream, response: HttpResponse) -> io::Result<()> {
    write!(
        connection,
        "HTTP/1.1 {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nCache-Control: no-store\r\nConnection: close\r\nX-Content-Type-Options: nosniff\r\n\r\n",
        status_line(response.status),
        response.content_type,
        response.body.len()
    )?;
    connection.write_all(&response.body)?;
    connection.flush()
}

const fn status_line(status: u16) -> &'static str {
    match status {
        200 => "200 OK",
        400 => "400 Bad Request",
        404 => "404 Not Found",
        405 => "405 Method Not Allowed",
        409 => "409 Conflict",
        413 => "413 Payload Too Large",
        415 => "415 Unsupported Media Type",
        431 => "431 Request Header Fields Too Large",
        503 => "503 Service Unavailable",
        504 => "504 Gateway Timeout",
        _ => "500 Internal Server Error",
    }
}

struct RequestError {
    status: u16,
    code: &'static str,
    message: &'static str,
}

impl RequestError {
    const fn new(status: u16, code: &'static str, message: &'static str) -> Self {
        Self {
            status,
            code,
            message,
        }
    }

    const fn bad_request(message: &'static str) -> Self {
        Self::new(400, "invalid_request", message)
    }

    fn into_response(self) -> HttpResponse {
        rejected(self.status, self.code, self.message)
    }
}

fn rejected(status: u16, code: &'static str, message: &'static str) -> HttpResponse {
    HttpResponse::json(
        status,
        json!({
            "protocol": "oxid.standalone-faucet.v1",
            "id": null,
            "ok": false,
            "error": { "code": code, "message": message }
        }),
    )
}

// This page contains no Tailnet identity. Serve supplies the HTTPS origin and
// the owner-generated QR at /setup.svg; the route cannot select policy.
const DISCOVERY_PAGE: &str = r#"<!doctype html>
<meta name="viewport" content="width=device-width,initial-scale=1">
<title>Standalone development funding</title>
<style>
body{font:16px system-ui;margin:auto;max-width:38rem;padding:1.25rem;background:#10151c;color:#edf2f7}
main{display:grid;gap:1rem}input,button{font:inherit;padding:.65rem;width:100%;box-sizing:border-box}
button{background:#78d6b1;border:0;border-radius:.4rem;color:#102018;font-weight:700}
button:disabled{opacity:.6}img{max-width:14rem;background:#fff;padding:.5rem}small{color:#b8c4d1}
</style>
<main>
<h1>Development NIGHT funding</h1>
<p>Undeployed realm · fixed 50,000 NIGHT grant</p>
<img src="/setup.svg" alt="Setup QR">
<small id="route"></small>
<input id="address" placeholder="Undeployed unshielded address" autocomplete="off">
<button id="fund">Request fixed grant</button>
<output id="result" aria-live="polite"></output>
<small>Tailnet HTTPS is private transport, not a Midnight network or public faucet.</small>
</main>
<script>
const route=document.querySelector('#route');
const address=document.querySelector('#address');
const fund=document.querySelector('#fund');
const result=document.querySelector('#result');
route.textContent=location.origin;
fund.onclick=async()=>{
  fund.disabled=true;
  result.textContent='Requesting grant…';
  try {
    const response=await fetch('/fund',{method:'POST',headers:{'content-type':'application/json'},body:JSON.stringify({requestId:crypto.randomUUID(),recipientAddress:address.value.trim()})});
    const body=await response.json();
    result.textContent=body.ok?'Grant included':'Request rejected';
  } catch (_) {
    result.textContent='Funding service unavailable';
  } finally {
    fund.disabled=false;
  }
};
</script>"#;

#[cfg(test)]
mod tests {
    use std::{
        io::{Read as _, Write as _},
        net::{Shutdown, TcpListener, TcpStream},
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        },
        thread,
    };

    use super::*;
    use crate::faucet::{GrantError, GrantOutcome, NightGrantPort};

    struct Grant(Arc<AtomicUsize>);

    impl NightGrantPort for Grant {
        fn grant(&self, _: &str) -> Result<GrantOutcome, GrantError> {
            self.0.fetch_add(1, Ordering::SeqCst);
            Ok(GrantOutcome {
                transaction_id: "transaction".into(),
                block_id: "block".into(),
            })
        }
    }

    fn faucet() -> (StandaloneFaucet, Arc<AtomicUsize>) {
        let calls = Arc::new(AtomicUsize::new(0));
        (
            StandaloneFaucet::with_grant(Box::new(Grant(calls.clone()))),
            calls,
        )
    }

    fn exchange(faucet: StandaloneFaucet, requests: &[&[u8]]) -> Vec<String> {
        exchange_with_setup(faucet, requests, None)
    }

    fn exchange_with_setup(
        faucet: StandaloneFaucet,
        requests: &[&[u8]],
        setup_svg: Option<&'static [u8]>,
    ) -> Vec<String> {
        let listener = TcpListener::bind("127.0.0.1:0").expect("loopback listener");
        let address = listener.local_addr().expect("listener address");
        let count = requests.len();
        let server = thread::spawn(move || {
            let mut faucet = faucet;
            for connection in listener.incoming().take(count) {
                serve_connection(
                    &mut connection.expect("test connection"),
                    &mut faucet,
                    setup_svg,
                )
                .expect("serve response");
            }
        });
        let responses = requests
            .iter()
            .map(|request| {
                let mut client = TcpStream::connect(address).expect("connect to listener");
                client.write_all(request).expect("write request");
                client.shutdown(Shutdown::Write).expect("finish request");
                let mut response = String::new();
                client.read_to_string(&mut response).expect("read response");
                response
            })
            .collect();
        server.join().expect("server joins");
        responses
    }

    fn request(method: &str, path: &str, content_type: Option<&str>, body: &[u8]) -> Vec<u8> {
        let content_type = content_type
            .map(|value| format!("Content-Type: {value}\r\n"))
            .unwrap_or_default();
        format!(
            "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1\r\n{content_type}Content-Length: {}\r\n\r\n{}",
            body.len(),
            String::from_utf8_lossy(body)
        )
        .into_bytes()
    }

    #[test]
    fn socket_funding_reuses_dispatcher_idempotency() {
        let (faucet, calls) = faucet();
        let body =
            br#"{"requestId":"request-1","recipientAddress":"mn_addr_undeployed1recipient"}"#;
        let request = request(
            "POST",
            "/fund",
            Some("application/json; charset=utf-8"),
            body,
        );
        let responses = exchange(faucet, &[&request, &request]);
        assert!(
            responses
                .iter()
                .all(|response| response.starts_with("HTTP/1.1 200 OK"))
        );
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert!(responses[1].contains("\"deduplicated\":true"));
    }

    #[test]
    fn discovery_page_is_responsive_and_has_no_personal_tailnet_identity() {
        let (faucet, _) = faucet();
        let response = exchange(faucet, &[&request("GET", "/", None, b"")]).remove(0);
        assert!(response.starts_with("HTTP/1.1 200 OK"));
        assert!(response.contains("Content-Type: text/html; charset=utf-8"));
        assert!(response.contains("Undeployed realm"));
        assert!(response.contains("fixed 50,000 NIGHT grant"));
        assert!(response.contains("/setup.svg"));
        assert!(response.contains("location.origin"));
        assert!(!response.contains(".ts.net"));
    }

    #[test]
    fn setup_qr_is_available_only_when_supplied_by_the_owner_lifecycle() {
        const SVG: &[u8] = b"<svg>fixture</svg>";
        let request = request("GET", "/setup.svg", None, b"");
        let (configured_faucet, _) = faucet();
        let response = exchange_with_setup(configured_faucet, &[&request], Some(SVG)).remove(0);
        assert!(response.starts_with("HTTP/1.1 200 OK"));
        assert!(response.contains("Content-Type: image/svg+xml"));
        assert!(response.ends_with("<svg>fixture</svg>"));

        let (plain_faucet, _) = faucet();
        let missing = exchange(plain_faucet, &[&request]).remove(0);
        assert!(missing.starts_with("HTTP/1.1 404 Not Found"));
    }

    #[test]
    fn socket_health_is_closed_and_payload_free() {
        let (faucet, _) = faucet();
        let request = request("GET", "/health", None, b"");
        let response = exchange(faucet, &[&request]).remove(0);
        assert!(response.starts_with("HTTP/1.1 200 OK"));
        assert!(response.contains("\"protocol\":\"oxid.standalone-faucet.v1\""));
        assert!(response.contains("\"networkId\":\"undeployed\""));
        assert!(!response.contains("seed"));
    }

    #[test]
    fn rejects_wrong_method_type_and_oversize() {
        let (faucet, _) = faucet();
        let wrong_method = request("PUT", "/fund", None, b"");
        let wrong_type = request("POST", "/fund", Some("text/plain"), b"{}");
        let oversize = format!(
            "POST /fund HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n",
            MAX_BODY_BYTES + 1
        );
        let responses = exchange(
            faucet,
            &[
                wrong_method.as_slice(),
                wrong_type.as_slice(),
                oversize.as_bytes(),
            ],
        );
        assert!(responses[0].starts_with("HTTP/1.1 405 Method Not Allowed"));
        assert!(responses[1].starts_with("HTTP/1.1 415 Unsupported Media Type"));
        assert!(responses[2].starts_with("HTTP/1.1 413 Payload Too Large"));
    }

    #[test]
    fn rejects_ambiguous_or_streaming_headers() {
        let (faucet, _) = faucet();
        let duplicate = b"POST /fund HTTP/1.1\r\nHost: localhost\r\nContent-Length: 0\r\nContent-Length: 0\r\n\r\n";
        let chunked =
            b"POST /fund HTTP/1.1\r\nHost: localhost\r\nTransfer-Encoding: chunked\r\n\r\n";
        let responses = exchange(faucet, &[duplicate, chunked]);
        assert!(
            responses
                .iter()
                .all(|response| response.starts_with("HTTP/1.1 400 Bad Request"))
        );
    }

    #[test]
    fn rejects_headers_beyond_the_fixed_limit() {
        let (faucet, _) = faucet();
        let request = format!(
            "GET /health HTTP/1.1\r\nHost: localhost\r\nX-Fill: {}\r\n\r\n",
            "x".repeat(MAX_HEADER_BYTES)
        );
        let response = exchange(faucet, &[request.as_bytes()]).remove(0);
        assert!(response.starts_with("HTTP/1.1 431 Request Header Fields Too Large"));
    }

    #[test]
    fn public_server_rejects_non_loopback_binding() {
        let (mut faucet, _) = faucet();
        assert!(matches!(
            run_loopback_http(&mut faucet, "0.0.0.0:36301", None),
            Err(HttpServerError::NonLoopbackAddress)
        ));
        assert!(matches!(
            run_loopback_http(&mut faucet, "not-an-address", None),
            Err(HttpServerError::InvalidAddress)
        ));
    }
}
