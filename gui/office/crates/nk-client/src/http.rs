//! A tiny HTTP/1.1 client over TCP or a unix socket: one connection per
//! request (`Connection: close`), bodies by Content-Length, chunked or EOF.
//! It is all the core needs (loopback, small JSON, one long SSE stream) and
//! works the same on both transports, which off-the-shelf blocking clients
//! do not.

use std::fmt;
use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::time::Duration;

use anyhow::{Context as _, Result, anyhow, bail};
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::proto::ErrorBody;

/// Where the core listens.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Addr {
    /// "host:port" (loopback only).
    Tcp(String),
    Unix(PathBuf),
}

impl Addr {
    /// Parses "tcp://127.0.0.1:PORT" or "unix:///path".
    pub fn parse(spec: &str) -> Result<Addr> {
        if let Some(rest) = spec.strip_prefix("tcp://") {
            let host = rest.trim_end_matches('/');
            let (h, p) = host.rsplit_once(':').ok_or_else(|| anyhow!("endereço sem porta: {spec}"))?;
            if !matches!(h, "127.0.0.1" | "localhost" | "[::1]") {
                bail!("só loopback: {spec}");
            }
            p.parse::<u16>().with_context(|| format!("porta inválida: {spec}"))?;
            return Ok(Addr::Tcp(host.to_string()));
        }
        if let Some(path) = spec.strip_prefix("unix://")
            && !path.is_empty()
        {
            return Ok(Addr::Unix(PathBuf::from(path)));
        }
        bail!("endereço inválido {spec:?} (use tcp://127.0.0.1:PORTA ou unix:///caminho)")
    }

    pub fn is_tcp(&self) -> bool {
        matches!(self, Addr::Tcp(_))
    }

    /// The Host header / ws:// authority.
    pub fn host(&self) -> &str {
        match self {
            Addr::Tcp(h) => h,
            Addr::Unix(_) => "nk",
        }
    }
}

impl fmt::Display for Addr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Addr::Tcp(h) => write!(f, "tcp://{h}"),
            Addr::Unix(p) => write!(f, "unix://{}", p.display()),
        }
    }
}

/// A connected stream.
#[derive(Debug)]
pub(crate) enum Conn {
    Tcp(TcpStream),
    Unix(UnixStream),
}

impl Conn {
    pub fn set_read_timeout(&self, d: Option<Duration>) -> io::Result<()> {
        match self {
            Conn::Tcp(s) => s.set_read_timeout(d),
            Conn::Unix(s) => s.set_read_timeout(d),
        }
    }
}

impl Read for Conn {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self {
            Conn::Tcp(s) => s.read(buf),
            Conn::Unix(s) => s.read(buf),
        }
    }
}

impl Write for Conn {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        match self {
            Conn::Tcp(s) => s.write(buf),
            Conn::Unix(s) => s.write(buf),
        }
    }
    fn flush(&mut self) -> io::Result<()> {
        match self {
            Conn::Tcp(s) => s.flush(),
            Conn::Unix(s) => s.flush(),
        }
    }
}

/// An HTTP error answer of the core (`{"error", "code"}`), downcastable from
/// the `anyhow::Error` the client returns.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApiError {
    pub status: u16,
    /// bad_request, unauthorized, not_found, unknown_command, unknown_token,
    /// unavailable, missing_arg, expired, not_implemented, failed…
    pub code: String,
    pub message: String,
}

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} ({}, HTTP {})", self.message, self.code, self.status)
    }
}

impl std::error::Error for ApiError {}

/// The core's error code of an error returned by this crate, if it is one.
pub fn error_code(err: &anyhow::Error) -> Option<&str> {
    err.downcast_ref::<ApiError>().map(|e| e.code.as_str())
}

/// How a server can be reached: address plus the bearer token (TCP).
#[derive(Debug, Clone)]
pub(crate) struct Endpoint {
    pub addr: Addr,
    pub token: Option<String>,
}

pub(crate) const REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

impl Endpoint {
    pub fn connect(&self) -> io::Result<Conn> {
        match &self.addr {
            Addr::Tcp(h) => {
                let mut last = io::Error::new(io::ErrorKind::NotFound, "sem endereço");
                for sa in h.to_socket_addrs()? {
                    match TcpStream::connect_timeout(&sa, Duration::from_secs(3)) {
                        Ok(s) => {
                            let _ = s.set_nodelay(true);
                            return Ok(Conn::Tcp(s));
                        }
                        Err(e) => last = e,
                    }
                }
                Err(last)
            }
            Addr::Unix(p) => UnixStream::connect(p).map(Conn::Unix),
        }
    }

    /// Sends one request and returns the answer with its body unread.
    pub fn send(
        &self,
        method: &str,
        path: &str,
        body: Option<&[u8]>,
        headers: &[(&str, &str)],
        timeout: Option<Duration>,
    ) -> Result<Response> {
        let mut conn = self.connect().with_context(|| format!("conectando em {}", self.addr))?;
        conn.set_read_timeout(timeout)?;
        let mut req = format!("{method} {path} HTTP/1.1\r\nHost: {}\r\nConnection: close\r\n", self.addr.host());
        if let Some(t) = &self.token {
            req.push_str(&format!("Authorization: Bearer {t}\r\n"));
        }
        for (k, v) in headers {
            req.push_str(&format!("{k}: {v}\r\n"));
        }
        match body {
            Some(b) => req.push_str(&format!("Content-Type: application/json\r\nContent-Length: {}\r\n", b.len())),
            None if method != "GET" => req.push_str("Content-Length: 0\r\n"),
            None => {}
        }
        req.push_str("\r\n");
        conn.write_all(req.as_bytes())?;
        if let Some(b) = body {
            conn.write_all(b)?;
        }
        conn.flush()?;
        Response::read(BufReader::new(conn), method == "HEAD")
    }

    /// A JSON call: sends `body` (if any), checks the status and decodes the
    /// answer (an empty body reads as `null`).
    pub fn call<B: Serialize + ?Sized, T: DeserializeOwned>(
        &self,
        method: &str,
        path: &str,
        body: Option<&B>,
    ) -> Result<T> {
        let raw = match body {
            Some(b) => Some(serde_json::to_vec(b)?),
            None => None,
        };
        let resp = self.send(method, path, raw.as_deref(), &[("Accept", "application/json")], Some(REQUEST_TIMEOUT))?;
        let status = resp.status;
        let text = resp.text()?;
        check(status, &text)?;
        let text = if text.trim().is_empty() { "null" } else { &text };
        serde_json::from_str(text).with_context(|| format!("{method} {path}: resposta inesperada"))
    }
}

/// Turns a non-2xx answer into an [`ApiError`].
pub(crate) fn check(status: u16, body: &str) -> Result<()> {
    if (200..300).contains(&status) {
        return Ok(());
    }
    let e: ErrorBody = serde_json::from_str(body).unwrap_or_default();
    Err(ApiError {
        status,
        code: if e.code.is_empty() { "failed".into() } else { e.code },
        message: if e.error.is_empty() { body.trim().to_string() } else { e.error },
    }
    .into())
}

/// An answer whose body is read on demand.
pub(crate) struct Response {
    pub status: u16,
    #[allow(dead_code)] // read by tests (and handy when debugging)
    pub headers: Vec<(String, String)>,
    pub body: Body,
}

impl Response {
    fn read<R: BufRead + Send + 'static>(mut r: R, head: bool) -> Result<Response> {
        let mut line = String::new();
        if r.read_line(&mut line)? == 0 {
            bail!("conexão fechada sem resposta");
        }
        let mut parts = line.split_whitespace();
        let (Some(proto), Some(code)) = (parts.next(), parts.next()) else {
            bail!("linha de status inválida: {line:?}");
        };
        if !proto.starts_with("HTTP/1.") {
            bail!("linha de status inválida: {line:?}");
        }
        let status: u16 = code.parse().with_context(|| format!("status inválido: {line:?}"))?;
        let mut headers = Vec::new();
        loop {
            line.clear();
            if r.read_line(&mut line)? == 0 {
                bail!("cabeçalhos truncados");
            }
            let l = line.trim_end_matches(['\r', '\n']);
            if l.is_empty() {
                break;
            }
            if let Some((k, v)) = l.split_once(':') {
                headers.push((k.trim().to_ascii_lowercase(), v.trim().to_string()));
            }
        }
        let get = |name: &str| headers.iter().find(|(k, _)| k == name).map(|(_, v)| v.as_str());
        let boxed: Box<dyn BufRead + Send> = Box::new(r);
        let body = if head || status == 204 || status == 304 || (100..200).contains(&status) {
            Body::Empty
        } else if get("transfer-encoding").is_some_and(|v| v.to_ascii_lowercase().contains("chunked")) {
            Body::Chunked(Chunked::new(boxed))
        } else if let Some(n) = get("content-length") {
            let n: u64 = n.parse().with_context(|| format!("content-length inválido: {n}"))?;
            Body::Length(boxed.take(n))
        } else {
            Body::Eof(boxed)
        };
        Ok(Response { status, headers, body })
    }

    #[cfg(test)]
    pub fn header(&self, name: &str) -> Option<&str> {
        let name = name.to_ascii_lowercase();
        self.headers.iter().find(|(k, _)| *k == name).map(|(_, v)| v.as_str())
    }

    pub fn text(mut self) -> Result<String> {
        let mut b = Vec::new();
        self.body.read_to_end(&mut b)?;
        Ok(String::from_utf8_lossy(&b).into_owned())
    }
}

/// A response body.
pub(crate) enum Body {
    Empty,
    Length(io::Take<Box<dyn BufRead + Send>>),
    Chunked(Chunked<Box<dyn BufRead + Send>>),
    Eof(Box<dyn BufRead + Send>),
}

impl Read for Body {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self {
            Body::Empty => Ok(0),
            Body::Length(r) => r.read(buf),
            Body::Chunked(r) => r.read(buf),
            Body::Eof(r) => r.read(buf),
        }
    }
}

/// Decodes `Transfer-Encoding: chunked`.
pub(crate) struct Chunked<R> {
    r: R,
    left: u64,
    done: bool,
}

impl<R: BufRead> Chunked<R> {
    pub fn new(r: R) -> Chunked<R> {
        Chunked { r, left: 0, done: false }
    }

    fn line(&mut self) -> io::Result<String> {
        let mut l = String::new();
        if self.r.read_line(&mut l)? == 0 {
            return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "chunk truncado"));
        }
        Ok(l.trim_end_matches(['\r', '\n']).to_string())
    }
}

impl<R: BufRead> Read for Chunked<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        if self.done || buf.is_empty() {
            return Ok(0);
        }
        if self.left == 0 {
            let l = self.line()?;
            let size = l.split(';').next().unwrap_or("").trim();
            let n = u64::from_str_radix(size, 16)
                .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, format!("tamanho de chunk inválido: {l:?}")))?;
            if n == 0 {
                // trailers until the empty line
                while !self.line()?.is_empty() {}
                self.done = true;
                return Ok(0);
            }
            self.left = n;
        }
        let max = buf.len().min(self.left as usize);
        let n = self.r.read(&mut buf[..max])?;
        if n == 0 {
            return Err(io::Error::new(io::ErrorKind::UnexpectedEof, "chunk truncado"));
        }
        self.left -= n as u64;
        if self.left == 0 {
            self.line()?; // the CRLF after the data
        }
        Ok(n)
    }
}

/// Percent-encodes a query or path component.
pub(crate) fn encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    fn resp(raw: &str) -> Response {
        Response::read(Cursor::new(raw.as_bytes().to_vec()), false).unwrap()
    }

    #[test]
    fn parses_addresses() {
        assert_eq!(Addr::parse("tcp://127.0.0.1:8080").unwrap(), Addr::Tcp("127.0.0.1:8080".into()));
        assert_eq!(Addr::parse("unix:///run/x.sock").unwrap(), Addr::Unix("/run/x.sock".into()));
        for bad in ["", "ftp://x", "unix://", "tcp://10.0.0.1:80", "tcp://127.0.0.1", "tcp://127.0.0.1:xx"] {
            assert!(Addr::parse(bad).is_err(), "{bad}");
        }
        assert_eq!(Addr::parse("unix:///a").unwrap().host(), "nk");
    }

    #[test]
    fn content_length_body() {
        let r = resp("HTTP/1.1 200 OK\r\nContent-Length: 5\r\nX-A: b\r\n\r\nhelloEXTRA");
        assert_eq!(r.status, 200);
        assert_eq!(r.header("x-a"), Some("b"));
        assert_eq!(r.text().unwrap(), "hello");
    }

    #[test]
    fn chunked_body() {
        let r = resp(
            "HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n4\r\nWiki\r\n6;ext=1\r\npedia \r\nE\r\nin \r\n\r\nchunks.\r\n0\r\nTrailer: x\r\n\r\n",
        );
        assert_eq!(r.text().unwrap(), "Wikipedia in \r\n\r\nchunks.");
    }

    #[test]
    fn truncated_chunk_is_an_error() {
        let mut r = resp("HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n10\r\nabc");
        let mut s = Vec::new();
        assert!(r.body.read_to_end(&mut s).is_err());
    }

    #[test]
    fn eof_and_empty_bodies() {
        assert_eq!(resp("HTTP/1.1 200 OK\r\n\r\nall of it").text().unwrap(), "all of it");
        assert_eq!(resp("HTTP/1.1 204 No Content\r\n\r\n").text().unwrap(), "");
    }

    #[test]
    fn bad_status_line() {
        assert!(Response::read(Cursor::new(b"garbage\r\n\r\n".to_vec()), false).is_err());
        assert!(Response::read(Cursor::new(Vec::new()), false).is_err());
    }

    #[test]
    fn api_errors() {
        let e = check(404, r#"{"error":"confirmação desconhecida","code":"unknown_token"}"#).unwrap_err();
        assert_eq!(error_code(&e), Some("unknown_token"));
        let api = e.downcast_ref::<ApiError>().unwrap();
        assert_eq!(api.status, 404);
        assert_eq!(api.message, "confirmação desconhecida");
        let e = check(502, "bad gateway").unwrap_err();
        assert_eq!(error_code(&e), Some("failed"));
        assert!(check(204, "").is_ok());
    }

    #[test]
    fn percent_encoding() {
        assert_eq!(encode("a b/ç&="), "a%20b%2F%C3%A7%26%3D");
    }
}
