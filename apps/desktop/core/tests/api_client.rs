use std::io::{Read, Write};
use std::net::TcpListener;
use std::thread::JoinHandle;

use aimemorandum_core::{ApiClient, ApiError, PendingOperation, SyncApi};

fn fixture() -> &'static str {
    include_str!("../../fixtures/item.json")
}

struct RecordedRequest {
    method: String,
    target: String,
    headers: String,
    body: Vec<u8>,
}

struct TestServer {
    url: String,
    handle: JoinHandle<RecordedRequest>,
}

fn server(status: &str, body: String) -> TestServer {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let status = status.to_owned();
    let handle = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(std::time::Duration::from_secs(5)))
            .unwrap();
        let mut bytes = Vec::new();
        let header_end = loop {
            if let Some(index) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
                break index + 4;
            }
            let mut chunk = [0_u8; 4096];
            let count = stream.read(&mut chunk).unwrap();
            assert!(count > 0, "client closed before sending headers");
            bytes.extend_from_slice(&chunk[..count]);
        };
        let headers = String::from_utf8_lossy(&bytes[..header_end]).to_ascii_lowercase();
        let length = headers
            .lines()
            .find_map(|line| {
                line.strip_prefix("content-length:")
                    .map(|value| value.trim().parse::<usize>().unwrap())
            })
            .unwrap_or(0);
        while bytes.len() < header_end + length {
            let mut chunk = [0_u8; 4096];
            let count = stream.read(&mut chunk).unwrap();
            assert!(count > 0, "client closed before sending body");
            bytes.extend_from_slice(&chunk[..count]);
        }
        let first = String::from_utf8_lossy(&bytes[..header_end]);
        let mut parts = first.lines().next().unwrap().split_whitespace();
        let method = parts.next().unwrap().to_owned();
        let target = parts.next().unwrap().to_owned();
        let body_bytes = bytes[header_end..header_end + length].to_vec();
        let response = format!(
            "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
            body.len(), body
        );
        stream.write_all(response.as_bytes()).unwrap();
        RecordedRequest {
            method,
            target,
            headers,
            body: body_bytes,
        }
    });
    TestServer { url, handle }
}

fn operation(method: &str, path: &str, body: &str) -> PendingOperation {
    PendingOperation {
        id: "op-1".to_owned(),
        item_id: "11111111-1111-4111-8111-111111111111".to_owned(),
        method: method.to_owned(),
        path: path.to_owned(),
        body: body.to_owned(),
        depends_on: None,
        frozen: true,
        state: "queued".to_owned(),
        error: None,
    }
}

#[test]
fn submit_uses_frozen_method_path_headers_and_exact_bytes() {
    let server = server("200 OK", format!("{{\"item\":{}}}", fixture()));
    let client = ApiClient::new(&server.url, "secret").unwrap();
    let body = "{\"base_revision\":3,\"body_md\":\"界\",\"title\":\"标题\"}";
    let item = client
        .submit(&operation(
            "PUT",
            "/api/v1/items/11111111-1111-4111-8111-111111111111",
            body,
        ))
        .unwrap();
    assert_eq!(item.revision, 3);
    let request = server.handle.join().unwrap();
    assert_eq!(request.method, "PUT");
    assert_eq!(
        request.target,
        "/api/v1/items/11111111-1111-4111-8111-111111111111"
    );
    assert!(request.headers.contains("authorization: bearer secret"));
    assert!(request.headers.contains("content-type: application/json"));
    assert_eq!(request.body, body.as_bytes());
}

#[test]
fn enroll_uses_public_device_enrollment_wire() {
    let server = server(
        "201 Created",
        "{\"device_token\":\"token\",\"device_name\":\"Windows\"}".to_owned(),
    );
    let enrollment = ApiClient::enroll(&server.url, " code ", "Windows").unwrap();
    assert_eq!(enrollment.device_token, "token");
    let request = server.handle.join().unwrap();
    assert_eq!(request.method, "POST");
    assert_eq!(request.target, "/api/v1/device-enrollments");
    assert!(!request.headers.contains("authorization:"));
    let body: serde_json::Value = serde_json::from_slice(&request.body).unwrap();
    assert_eq!(body["enrollment_code"], " code ");
    assert_eq!(body["device_name"], "Windows");
}

#[test]
fn pull_uses_decimal_cursor_and_decodes_page() {
    let server = server(
        "200 OK",
        format!("{{\"cursor\":\"42\",\"items\":[{}]}}", fixture()),
    );
    let client = ApiClient::new(&server.url, "secret").unwrap();
    let page = client.pull("41").unwrap();
    assert_eq!(page.cursor, "42");
    assert_eq!(page.items.len(), 1);
    let request = server.handle.join().unwrap();
    assert_eq!(request.method, "GET");
    assert_eq!(request.target, "/api/v1/sync?cursor=41");
    assert!(request.body.is_empty());
}

#[test]
fn error_envelope_preserves_classification_and_current_item() {
    let server = server(
        "409 Conflict",
        format!(
            "{{\"error\":{{\"name\":\"REVISION_CONFLICT\",\"message\":\"stale\"}},\"item\":{}}}",
            fixture()
        ),
    );
    let client = ApiClient::new(&server.url, "secret").unwrap();
    let error = client
        .submit(&operation("POST", "/api/v1/items", "{}"))
        .unwrap_err();
    match error {
        ApiError::Http {
            status,
            name,
            message,
            item,
        } => {
            assert_eq!(status, 409);
            assert_eq!(name, "REVISION_CONFLICT");
            assert_eq!(message, "stale");
            assert_eq!(item.unwrap().revision, 3);
        }
        other => panic!("unexpected error: {other}"),
    }
    server.handle.join().unwrap();
}

#[test]
fn successful_invalid_json_is_a_decode_error() {
    let server = server("200 OK", "not-json".to_owned());
    let client = ApiClient::new(&server.url, "secret").unwrap();
    let error = client.pull("0").unwrap_err();
    assert!(matches!(error, ApiError::Decode(_)));
    server.handle.join().unwrap();
}

#[test]
fn download_attachment_uses_get_and_returns_original_bytes() {
    let server = server("200 OK", "abc".to_owned());
    let client = ApiClient::new(&server.url, "secret").unwrap();
    let bytes = client.download_attachment("attachment-1").unwrap();
    assert_eq!(bytes, b"abc");
    let request = server.handle.join().unwrap();
    assert_eq!(request.method, "GET");
    assert_eq!(request.target, "/api/v1/attachments/attachment-1/content");
    assert!(request.headers.contains("authorization: bearer secret"));
    assert!(request.body.is_empty());
}
