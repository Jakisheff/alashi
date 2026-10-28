use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

struct Server(Child);
impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn start_server() -> (Server, std::net::SocketAddr) {
    let reserved = TcpListener::bind("127.0.0.1:0").unwrap();
    let addr = reserved.local_addr().unwrap();
    drop(reserved);
    let server = Server(
        Command::new(env!("CARGO_BIN_EXE_chain_api"))
            .env("ALASHI_CHAIN_API_PORT", addr.port().to_string())
            .stdout(Stdio::null())
            .stderr(Stdio::inherit())
            .spawn()
            .unwrap(),
    );
    (server, addr)
}

fn connect_when_ready(server: &mut Server, addr: std::net::SocketAddr) -> TcpStream {
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        match TcpStream::connect(addr) {
            Ok(stream) => break stream,
            Err(_) if Instant::now() < deadline => {
                if let Some(status) = server.0.try_wait().unwrap() {
                    panic!("server exited: {status}");
                }
                std::thread::sleep(Duration::from_millis(20));
            }
            Err(error) => panic!("server did not start: {error}"),
        }
    }
}

#[test]
fn slow_connection_does_not_block_an_unrelated_request() {
    let (mut server, addr) = start_server();
    let mut slow = connect_when_ready(&mut server, addr);
    slow.write_all(b"GET /").unwrap();
    std::thread::sleep(Duration::from_millis(100));
    let mut other = TcpStream::connect(addr).unwrap();
    other
        .set_read_timeout(Some(Duration::from_secs(1)))
        .unwrap();
    other
        .write_all(b"GET /unrelated HTTP/1.1\r\nHost: localhost\r\n\r\n")
        .unwrap();
    let mut response = String::new();
    let result = other.read_to_string(&mut response);
    assert!(result.is_ok(), "unrelated request blocked: {result:?}");
    assert!(
        response.starts_with("HTTP/1.1 404 Not Found\r\n"),
        "{response}"
    );
}

#[test]
fn overload_is_bounded_and_service_recovers_when_connections_close() {
    let (mut server, addr) = start_server();
    let mut held = vec![connect_when_ready(&mut server, addr)];
    for _ in 1..8 {
        held.push(TcpStream::connect(addr).unwrap());
    }
    for stream in &mut held {
        stream.write_all(b"GET /").unwrap();
    }
    std::thread::sleep(Duration::from_millis(200));
    let mut extra = TcpStream::connect(addr).unwrap();
    extra
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    extra
        .write_all(b"GET /unrelated HTTP/1.1\r\nHost: localhost\r\n\r\n")
        .unwrap();
    let mut response = String::new();
    BufReader::new(extra).read_line(&mut response).unwrap();
    assert!(
        response.starts_with("HTTP/1.1 503 Service Unavailable\r\n"),
        "{response}"
    );
    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(2))
        .build()
        .unwrap();
    let status = client
        .get(format!("http://{addr}/unrelated"))
        .send()
        .unwrap()
        .status();
    assert_eq!(status.as_u16(), 503);
    drop(held);
    let deadline = Instant::now() + Duration::from_secs(3);
    loop {
        let mut client = TcpStream::connect(addr).unwrap();
        client
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        client
            .write_all(b"GET /unrelated HTTP/1.1\r\nHost: localhost\r\n\r\n")
            .unwrap();
        let mut body = String::new();
        let _ = client.read_to_string(&mut body);
        if body.starts_with("HTTP/1.1 404 Not Found\r\n") {
            break;
        }
        assert!(Instant::now() < deadline, "server did not recover: {body}");
        std::thread::sleep(Duration::from_millis(20));
    }
}
