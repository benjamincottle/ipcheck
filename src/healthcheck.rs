//! `ipcheck --healthcheck`: probe a running instance and exit 0/1. Meant for a
//! container HEALTHCHECK, since the distroless image has no shell or curl.
//!
//! The probe is an unauthenticated OPTIONS request from loopback carrying the
//! probe's User-Agent, answered with a 204 by a worker thread. That exercises
//! the listener, a worker and the response path without needing the API key.
//! Any other request, OPTIONS included, still gets the usual 401 or 200.

use std::{
    io::{Read, Write},
    net::{SocketAddr, TcpStream},
    process::exit,
    time::Duration,
};
use tiny_http::{Request, Response};

pub const BIND_ADDR: &str = "0.0.0.0:5000";
const PROBE_ADDR: &str = "127.0.0.1:5000";
const PROBE_TIMEOUT: Duration = Duration::from_secs(5);
const PROBE_USER_AGENT: &str = "ipcheck-probe";

pub fn run() -> ! {
    let addr: SocketAddr = PROBE_ADDR.parse().expect("PROBE_ADDR is a valid address");
    match probe(addr) {
        Ok(()) => exit(0),
        Err(e) => {
            eprintln!("healthcheck failed: {e}");
            exit(1);
        }
    }
}

/// Only the healthcheck's own requests: OPTIONS, from loopback, with the
/// probe's User-Agent.
pub fn is_self_probe(request: &Request) -> bool {
    request.method().as_str() == "OPTIONS"
        && request
            .headers()
            .iter()
            .any(|h| h.field.equiv("User-Agent") && h.value.as_str() == PROBE_USER_AGENT)
        && request.remote_addr().is_some_and(|a| a.ip().is_loopback())
}

/// Answer a self-probe with a 204. Not access-logged: one line per interval
/// is noise.
pub fn respond(request: Request) {
    if let Err(e) = request.respond(Response::empty(204)) {
        log::error!("[Error] Could not send healthcheck response: {}", e);
    }
}

/// Send an OPTIONS request to the listener and expect a 204.
fn probe(addr: SocketAddr) -> Result<(), String> {
    let mut stream = TcpStream::connect_timeout(&addr, PROBE_TIMEOUT)
        .map_err(|e| format!("connect to {addr}: {e}"))?;
    let _ = stream.set_read_timeout(Some(PROBE_TIMEOUT));
    let _ = stream.set_write_timeout(Some(PROBE_TIMEOUT));
    stream
        .write_all(
            format!(
                "OPTIONS /ip HTTP/1.1\r\nHost: localhost\r\nUser-Agent: {PROBE_USER_AGENT}\r\nConnection: close\r\n\r\n"
            )
            .as_bytes(),
        )
        .map_err(|e| format!("write to {addr}: {e}"))?;
    let mut buf = [0u8; 64];
    let mut filled = 0;
    while filled < buf.len() && !buf[..filled].contains(&b'\n') {
        match stream.read(&mut buf[filled..]) {
            Ok(0) => break,
            Ok(n) => filled += n,
            Err(e) => return Err(format!("read from {addr}: {e}")),
        }
    }
    let status_line = String::from_utf8_lossy(&buf[..filled]);
    let status_line = status_line.lines().next().unwrap_or("");
    if status_line.starts_with("HTTP/1.1 204") || status_line.starts_with("HTTP/1.0 204") {
        Ok(())
    } else {
        let shown: String = status_line
            .chars()
            .take(64)
            .map(|c| {
                if c.is_ascii_graphic() || c == ' ' {
                    c
                } else {
                    '?'
                }
            })
            .collect();
        Err(format!("unexpected status line: {shown}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn probe_accepts_a_204_and_rejects_anything_else() {
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let addr = server.server_addr().to_ip().unwrap();
        let worker = std::thread::spawn(move || {
            let request = server.recv().unwrap();
            assert!(is_self_probe(&request));
            respond(request);
            let request = server.recv().unwrap();
            request.respond(Response::empty(401)).unwrap();
        });
        assert_eq!(probe(addr), Ok(()));
        assert!(probe(addr).unwrap_err().contains("401"));
        worker.join().unwrap();
    }

    #[test]
    fn other_options_requests_are_not_self_probes() {
        let server = tiny_http::Server::http("127.0.0.1:0").unwrap();
        let addr = server.server_addr().to_ip().unwrap();
        let client = std::thread::spawn(move || {
            let mut stream = TcpStream::connect(addr).unwrap();
            stream
                .write_all(b"OPTIONS /ip HTTP/1.1\r\nHost: localhost\r\nUser-Agent: curl\r\nConnection: close\r\n\r\n")
                .unwrap();
            let mut sink = Vec::new();
            let _ = stream.read_to_end(&mut sink);
        });
        let request = server.recv().unwrap();
        assert!(!is_self_probe(&request));
        request.respond(Response::empty(401)).unwrap();
        client.join().unwrap();
    }
}
