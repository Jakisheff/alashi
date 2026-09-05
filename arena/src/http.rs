//! Минимальный HTTP/1.1 для арены: то, что нужно API, и ни строчкой больше.
//! GET/POST, Content-Length, Connection: close. Число одновременных соединений ограничено в api.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;

pub struct Request {
    pub method: String,
    pub path: String,
    pub body: Vec<u8>,
}

const MAX_HEAD: usize = 16 * 1024;
const MAX_BODY: usize = 256 * 1024;

/// Read a whole request within one deadline, including slow trickle clients.
struct DeadlineReader<'a> {
    stream: &'a TcpStream,
    deadline: std::time::Instant,
}
impl Read for DeadlineReader<'_> {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let left = self.deadline.checked_duration_since(std::time::Instant::now())
            .filter(|d| !d.is_zero())
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::TimedOut, "request deadline"))?;
        self.stream.set_read_timeout(Some(left))?;
        self.stream.read(buf)
    }
}

pub fn read_request(stream: &TcpStream) -> Option<Request> {
    parse_request(&mut BufReader::new(DeadlineReader {
        stream,
        deadline: std::time::Instant::now() + std::time::Duration::from_secs(10),
    }))
}

fn header_line(reader: &mut impl BufRead, remaining: &mut usize) -> Option<String> {
    let mut bytes = Vec::new();
    // Bound allocation before waiting for a newline, including the request line.
    let read = reader.take((*remaining + 1) as u64).read_until(b'\n', &mut bytes).ok()?;
    if read == 0 || read > *remaining || !bytes.ends_with(b"\r\n") { return None; }
    *remaining -= read;
    String::from_utf8(bytes).ok()
}

fn parse_request(reader: &mut impl BufRead) -> Option<Request> {
    let mut remaining = MAX_HEAD;
    let line = header_line(reader, &mut remaining)?;
    let mut parts = line.split_whitespace();
    let method = parts.next()?.to_string();
    let path = parts.next()?.to_string();
    if !matches!(parts.next()?, "HTTP/1.1" | "HTTP/1.0") || parts.next().is_some() { return None; }
    let mut content_length = None;
    loop {
        let h = header_line(reader, &mut remaining)?;
        if h == "\r\n" { break; }
        let (key, value) = h.trim_end().split_once(':')?;
        if key.eq_ignore_ascii_case("transfer-encoding") { return None; }
        if key.eq_ignore_ascii_case("content-length") {
            if content_length.is_some() { return None; }
            let value = value.trim();
            if value.is_empty() || !value.bytes().all(|b| b.is_ascii_digit()) { return None; }
            content_length = Some(value.parse::<usize>().ok()?);
        }
    }
    let length = content_length.unwrap_or(0);
    if length > MAX_BODY { return None; }
    let mut body = vec![0; length];
    reader.read_exact(&mut body).ok()?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn header_limits_apply_before_unterminated_line_allocation() {
        for prefix in ["GET /", "GET / HTTP/1.1\r\nX-Large: "] {
            let bytes = format!("{}{}", prefix, "x".repeat(MAX_HEAD * 4)).into_bytes();
            let mut cursor = Cursor::new(bytes);
            assert!(parse_request(&mut cursor).is_none());
            assert!(cursor.position() <= (MAX_HEAD + 1) as u64);
        }
    }

    #[test]
    fn rejects_ambiguous_or_incomplete_http_framing() {
        for bytes in [
            "POST / HTTP/1.1\r\nContent-Length: -1\r\n\r\n",
            "POST / HTTP/1.1\r\nContent-Length: no\r\n\r\n",
            "POST / HTTP/1.1\r\nContent-Length: 2\r\nContent-Length: 2\r\n\r\n{}",
            "POST / HTTP/1.1\r\nTransfer-Encoding: chunked\r\n\r\n",
            "POST / HTTP/1.1\r\nContent-Length: 262145\r\n\r\n",
            "POST / HTTP/1.1\r\nContent-Length: 2\r\n\r\n{",
            "GET / HTTP/1.1\r\nHost: demo\r\n",
        ] { assert!(parse_request(&mut Cursor::new(bytes)).is_none(), "{bytes:?}"); }
        let req = parse_request(&mut Cursor::new("POST /game/new HTTP/1.1\r\nContent-Length: 2\r\n\r\n{}")).unwrap();
        assert_eq!(req.body, b"{}");
    }
}
