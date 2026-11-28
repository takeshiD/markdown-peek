//! Shared helpers for the adaptive-selection integration tests: a scripted
//! mock Jev HTTP server (std only), a minimal HTTP GET and WebSocket client.
#![allow(dead_code)]

use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub const TEST_KEY: &str = "tsk-test-SECRET-4f1c9a";

#[derive(Clone)]
pub struct Reply {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: String,
    pub delay_ms: u64,
}

impl Reply {
    pub fn json(status: u16, body: serde_json::Value) -> Self {
        Reply {
            status,
            headers: vec![],
            body: body.to_string(),
            delay_ms: 0,
        }
    }
    pub fn raw(status: u16, body: &str) -> Self {
        Reply {
            status,
            headers: vec![],
            body: body.to_string(),
            delay_ms: 0,
        }
    }
    pub fn header(mut self, k: &str, v: &str) -> Self {
        self.headers.push((k.into(), v.into()));
        self
    }
    pub fn delay(mut self, ms: u64) -> Self {
        self.delay_ms = ms;
        self
    }
}

#[derive(Clone, Debug)]
pub struct Received {
    pub method: String,
    pub path: String,
    pub headers: Vec<(String, String)>,
    pub body: String,
}

impl Received {
    pub fn header(&self, k: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(h, _)| h.eq_ignore_ascii_case(k))
            .map(|(_, v)| v.as_str())
    }
    pub fn json(&self) -> serde_json::Value {
        serde_json::from_str(&self.body).unwrap()
    }
}

/// Mock server answering with `replies` in order (the last one repeats).
pub struct Mock {
    pub port: u16,
    pub received: Arc<Mutex<Vec<Received>>>,
}

impl Mock {
    pub fn start(replies: Vec<Reply>) -> Mock {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let received = Arc::new(Mutex::new(Vec::new()));
        let rec = Arc::clone(&received);
        std::thread::spawn(move || {
            let mut n = 0usize;
            for stream in listener.incoming() {
                let Ok(stream) = stream else { continue };
                let reply = replies[n.min(replies.len() - 1)].clone();
                n += 1;
                let rec = Arc::clone(&rec);
                std::thread::spawn(move || handle(stream, reply, rec));
            }
        });
        Mock { port, received }
    }
    pub fn base_url(&self) -> String {
        format!("http://127.0.0.1:{}", self.port)
    }
    pub fn count(&self) -> usize {
        self.received.lock().unwrap().len()
    }
    pub fn requests(&self) -> Vec<Received> {
        self.received.lock().unwrap().clone()
    }
}

fn handle(stream: TcpStream, reply: Reply, rec: Arc<Mutex<Vec<Received>>>) {
    let mut reader = BufReader::new(stream.try_clone().unwrap());
    let mut line = String::new();
    if reader.read_line(&mut line).is_err() {
        return;
    }
    let mut parts = line.split_whitespace();
    let method = parts.next().unwrap_or("").to_string();
    let path = parts.next().unwrap_or("").to_string();
    let mut headers = Vec::new();
    let mut len = 0usize;
    loop {
        let mut h = String::new();
        if reader.read_line(&mut h).unwrap_or(0) == 0 {
            break;
        }
        let h = h.trim_end();
        if h.is_empty() {
            break;
        }
        if let Some((k, v)) = h.split_once(':') {
            if k.eq_ignore_ascii_case("content-length") {
                len = v.trim().parse().unwrap_or(0);
            }
            headers.push((k.trim().to_string(), v.trim().to_string()));
        }
    }
    let mut body = vec![0u8; len];
    let _ = reader.read_exact(&mut body);
    rec.lock().unwrap().push(Received {
        method,
        path,
        headers,
        body: String::from_utf8_lossy(&body).to_string(),
    });
    if reply.delay_ms > 0 {
        std::thread::sleep(Duration::from_millis(reply.delay_ms));
    }
    let mut out = stream;
    let mut resp = format!(
        "HTTP/1.1 {} X\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n",
        reply.status,
        reply.body.len()
    );
    for (k, v) in &reply.headers {
        resp.push_str(&format!("{k}: {v}\r\n"));
    }
    resp.push_str("\r\n");
    resp.push_str(&reply.body);
    let _ = out.write_all(resp.as_bytes());
}

/// A port with nothing listening (bind, then drop).
pub fn closed_port() -> u16 {
    let l = TcpListener::bind("127.0.0.1:0").unwrap();
    l.local_addr().unwrap().port()
}

pub fn write(dir: &std::path::Path, name: &str, body: &str) -> std::path::PathBuf {
    let p = dir.join(name);
    std::fs::write(&p, body).unwrap();
    p
}

/// Config pointing Jev at `base_url`.
pub fn jev_config(dir: &std::path::Path, base_url: &str, extra: &str) -> std::path::PathBuf {
    write(
        dir,
        "config.toml",
        &format!(
            "[model]\nuse = \"jev\"\n{extra}\n\n[models.jev]\napi = \"systemone\"\nmodel = \"jev-1.14.0\"\nbase_url = \"{base_url}\"\n"
        ),
    )
}

/// Plain HTTP/1.1 GET returning (status, body).
pub fn http_get(port: u16, path: &str) -> (u16, String) {
    let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
    s.set_read_timeout(Some(Duration::from_secs(30))).unwrap();
    write!(
        s,
        "GET {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n"
    )
    .unwrap();
    let mut buf = Vec::new();
    s.read_to_end(&mut buf).unwrap();
    let text = String::from_utf8_lossy(&buf).to_string();
    let status = text
        .split_whitespace()
        .nth(1)
        .unwrap_or("0")
        .parse()
        .unwrap_or(0);
    let body = text
        .split_once("\r\n\r\n")
        .map(|x| x.1.to_string())
        .unwrap_or_default();
    (status, body)
}

/// Plain HTTP/1.1 POST with a JSON body, returning (status, body).
pub fn http_post(port: u16, path: &str, body: &str) -> (u16, String) {
    let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
    s.set_read_timeout(Some(Duration::from_secs(30))).unwrap();
    write!(
        s,
        "POST {path} HTTP/1.1\r\nHost: localhost\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        body.len()
    )
    .unwrap();
    let mut buf = Vec::new();
    s.read_to_end(&mut buf).unwrap();
    let text = String::from_utf8_lossy(&buf).to_string();
    let status = text
        .split_whitespace()
        .nth(1)
        .unwrap_or("0")
        .parse()
        .unwrap_or(0);
    let body = text
        .split_once("\r\n\r\n")
        .map(|x| x.1.to_string())
        .unwrap_or_default();
    (status, body)
}

/// Minimal WebSocket client (text frames from the server only).
pub struct Ws {
    s: TcpStream,
}

impl Ws {
    pub fn connect(port: u16) -> Ws {
        let mut s = TcpStream::connect(("127.0.0.1", port)).unwrap();
        write!(
            s,
            "GET /ws HTTP/1.1\r\nHost: localhost\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==\r\nSec-WebSocket-Version: 13\r\n\r\n"
        )
        .unwrap();
        // Read the 101 response headers byte by byte.
        let mut hdr = Vec::new();
        let mut b = [0u8; 1];
        while !hdr.ends_with(b"\r\n\r\n") {
            s.read_exact(&mut b).unwrap();
            hdr.push(b[0]);
        }
        assert!(
            String::from_utf8_lossy(&hdr).contains("101"),
            "ws upgrade failed"
        );
        Ws { s }
    }

    /// Next text message, or None on timeout.
    pub fn next_text(&mut self, timeout: Duration) -> Option<String> {
        let deadline = Instant::now() + timeout;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            if left.is_zero() {
                return None;
            }
            self.s.set_read_timeout(Some(left)).unwrap();
            let mut h = [0u8; 2];
            if self.s.read_exact(&mut h).is_err() {
                return None;
            }
            let op = h[0] & 0x0f;
            let mut len = (h[1] & 0x7f) as u64;
            self.s
                .set_read_timeout(Some(Duration::from_secs(10)))
                .unwrap();
            if len == 126 {
                let mut e = [0u8; 2];
                self.s.read_exact(&mut e).unwrap();
                len = u16::from_be_bytes(e) as u64;
            } else if len == 127 {
                let mut e = [0u8; 8];
                self.s.read_exact(&mut e).unwrap();
                len = u64::from_be_bytes(e);
            }
            let mut payload = vec![0u8; len as usize];
            self.s.read_exact(&mut payload).unwrap();
            if op == 1 {
                return Some(String::from_utf8_lossy(&payload).to_string());
            }
        }
    }

    /// Collect `adaptive` messages for `dur`.
    pub fn adaptive_for(&mut self, dur: Duration) -> Vec<serde_json::Value> {
        let end = Instant::now() + dur;
        let mut out = Vec::new();
        while Instant::now() < end {
            let left = end.saturating_duration_since(Instant::now());
            match self.next_text(left) {
                Some(t) => {
                    let v: serde_json::Value = serde_json::from_str(&t).unwrap_or_default();
                    if v["type"] == "adaptive" {
                        out.push(v);
                    }
                }
                None => break,
            }
        }
        out
    }

    /// Wait for an `adaptive` message whose run has ended (done / stopped / error).
    pub fn wait_settled(&mut self, timeout: Duration) -> Option<serde_json::Value> {
        let end = Instant::now() + timeout;
        while Instant::now() < end {
            let v = self.wait_adaptive(end.saturating_duration_since(Instant::now()))?;
            let st = v["layout"]["progress"]["state"].as_str().unwrap_or("");
            if st != "running" && st != "idle" {
                return Some(v);
            }
        }
        None
    }

    /// Wait for the first `adaptive` message.
    pub fn wait_adaptive(&mut self, timeout: Duration) -> Option<serde_json::Value> {
        let end = Instant::now() + timeout;
        while Instant::now() < end {
            let t = self.next_text(end.saturating_duration_since(Instant::now()))?;
            let v: serde_json::Value = serde_json::from_str(&t).unwrap_or_default();
            if v["type"] == "adaptive" {
                return Some(v);
            }
        }
        None
    }
}

/// A running `mdpeek serve` child; killed on drop.
pub struct Server {
    pub child: std::process::Child,
    pub port: u16,
    pub log: Arc<Mutex<String>>,
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

pub fn spawn_serve(
    dir: &std::path::Path,
    file: &std::path::Path,
    config: &std::path::Path,
    env: &[(&str, &str)],
) -> Server {
    use std::process::{Command, Stdio};
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_mdpeek"));
    cmd.current_dir(dir)
        .arg("--config")
        .arg(config)
        .arg("serve")
        .arg(file)
        .arg("--port")
        .arg("0")
        .env("HOME", dir)
        .env("XDG_CONFIG_HOME", dir)
        // The disk cache must land in $HOME/.cache, not an inherited shared dir.
        .env_remove("XDG_CACHE_HOME")
        .env("RUST_LOG", "info")
        .env_remove(super_key())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (k, v) in env {
        cmd.env(k, v);
    }
    let mut child = cmd.spawn().unwrap();
    let log = Arc::new(Mutex::new(String::new()));
    let (ptx, prx) = std::sync::mpsc::channel::<u16>();
    for pipe in [
        Box::new(child.stdout.take().unwrap()) as Box<dyn Read + Send>,
        Box::new(child.stderr.take().unwrap()) as Box<dyn Read + Send>,
    ] {
        let log = Arc::clone(&log);
        let ptx = ptx.clone();
        std::thread::spawn(move || {
            for line in BufReader::new(pipe).lines().map_while(Result::ok) {
                if let Some(i) = line.find("http://") {
                    let rest = &line[i + 7..];
                    let addr: String = rest
                        .chars()
                        .take_while(|c| c.is_ascii_digit() || *c == '.' || *c == ':')
                        .collect();
                    if let Some(p) = addr.rsplit(':').next().and_then(|p| p.parse().ok()) {
                        let _ = ptx.send(p);
                    }
                }
                let mut l = log.lock().unwrap();
                l.push_str(&line);
                l.push('\n');
            }
        });
    }
    let port = prx
        .recv_timeout(Duration::from_secs(20))
        .expect("server did not start");
    Server { child, port, log }
}

fn super_key() -> &'static str {
    "TYPESAFE_API_KEY"
}
