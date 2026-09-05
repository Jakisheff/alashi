//! Минимальный HTTP/1.1 для арены: то, что нужно API, и ни строчкой больше.
//! GET/POST, Content-Length, Connection: close. Пулов нет — поток на соединение.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;

pub struct Request {
    pub method: String,
    pub path: String,
    pub body: Vec<u8>,
}

const MAX_HEAD: usize = 16 * 1024;
const MAX_BODY: usize = 256 * 1024;

pub fn read_request(stream: &TcpStream) -> Option<Request> {
    let mut reader = BufReader::new(stream.try_clone().ok()?);
    let mut line = String::new();
    if reader.read_line(&mut line).ok()? == 0 {
        return None;
    }
    let mut parts = line.split_whitespace();
    let method = parts.next()?.to_string();
    let path = parts.next()?.to_string();
    let mut content_length = 0usize;
    let mut head_bytes = line.len();
    loop {
        let mut h = String::new();
        if reader.read_line(&mut h).ok()? == 0 {
            break;
        }
        head_bytes += h.len();
        if head_bytes > MAX_HEAD {
            return None;
        }
        let h = h.trim();
        if h.is_empty() {
            break;
        }
        if let Some((k, v)) = h.split_once(':') {
            if k.trim().eq_ignore_ascii_case("content-length") {
                content_length = v.trim().parse().unwrap_or(0);
            }
        }
    }
    if content_length > MAX_BODY {
        return None;
    }
    let mut body = vec![0u8; content_length];
    if content_length > 0 && reader.read_exact(&mut body).is_err() {
        return None;
    }
    Some(Request { method, path, body })
}

pub fn respond(stream: &mut TcpStream, status: &str, body: &str) {
    let _ = stream.write_all(
        format!(
            "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nAccess-Control-Allow-Origin: *\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        )
        .as_bytes(),
    );
    let _ = stream.flush();
}

/// Раздача статики (зрительский экран): CORS + текстовый content-type.
pub fn respond_html(stream: &mut TcpStream, status: &str, body: &str, ctype: &str) {
    let _ = stream.write_all(
        format!(
            "HTTP/1.1 {status}\r\nContent-Type: {ctype}\r\nContent-Length: {}\r\nAccess-Control-Allow-Origin: *\r\nConnection: close\r\n\r\n{}",
            body.len(),
            body
        )
        .as_bytes(),
    );
    let _ = stream.flush();
}
