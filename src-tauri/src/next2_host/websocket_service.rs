//! Bounded WebSocket handles. Poll drives nonblocking I/O and protocol control frames.
use super::{error, Execution, Manager};
use base64::{engine::general_purpose::STANDARD, Engine};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    io::{Read, Write},
    net::{Shutdown, TcpStream, ToSocketAddrs},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};
use tungstenite::{
    client::IntoClientRequest, protocol::WebSocketConfig, stream::MaybeTlsStream, Connector, Error,
    Message, WebSocket,
};
struct Transport {
    tcp: TcpStream,
    deadline: Option<Instant>,
}
impl Transport {
    fn remaining(&self) -> std::io::Result<Option<Duration>> {
        self.deadline
            .map(|deadline| {
                let remaining = deadline.saturating_duration_since(Instant::now());
                if remaining.is_zero() {
                    Err(std::io::Error::new(
                        std::io::ErrorKind::TimedOut,
                        "WebSocket handshake deadline",
                    ))
                } else {
                    Ok(remaining.min(Duration::from_secs(5)))
                }
            })
            .transpose()
    }
}
impl Read for Transport {
    fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
        if let Some(timeout) = self.remaining()? {
            self.tcp.set_read_timeout(Some(timeout))?;
        }
        self.tcp.read(bytes)
    }
}
impl Write for Transport {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if let Some(timeout) = self.remaining()? {
            self.tcp.set_write_timeout(Some(timeout))?;
        }
        self.tcp.write(bytes)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.tcp.flush()
    }
}
type Socket = WebSocket<MaybeTlsStream<Transport>>;
struct Connection {
    socket: Socket,
    owner: String,
    recipient: String,
    session: String,
    failure: Option<String>,
    closed: bool,
}
impl Drop for Connection {
    fn drop(&mut self) {
        let _ = tcp(self.socket.get_mut()).shutdown(Shutdown::Both);
    }
}
#[derive(Default)]
pub(super) struct WebSockets(Mutex<HashMap<String, Connection>>);
impl WebSockets {
    pub(super) fn clear_session(&self, session: &str) -> Result<(), String> {
        self.0
            .lock()
            .map_err(|_| "WEBSOCKET_STATE_UNAVAILABLE")?
            .retain(|_, value| value.session != session);
        Ok(())
    }
    pub(super) fn clear_owner(&self, owner: &str) -> Result<(), String> {
        self.0
            .lock()
            .map_err(|_| "WEBSOCKET_STATE_UNAVAILABLE")?
            .retain(|_, value| value.owner != owner && value.recipient != owner);
        Ok(())
    }
    pub(super) fn clear(&self) -> Result<(), String> {
        self.0
            .lock()
            .map_err(|_| "WEBSOCKET_STATE_UNAVAILABLE")?
            .clear();
        Ok(())
    }
}
fn transport(stream: &mut MaybeTlsStream<Transport>) -> &mut Transport {
    match stream {
        MaybeTlsStream::Plain(stream) => stream,
        MaybeTlsStream::Rustls(stream) => &mut stream.sock,
        _ => unreachable!("only plain and rustls connectors are compiled"),
    }
}
fn tcp(stream: &mut MaybeTlsStream<Transport>) -> &mut TcpStream {
    &mut transport(stream).tcp
}
fn blocked(error: &Error) -> bool {
    matches!(error, Error::Io(error) if error.kind() == std::io::ErrorKind::WouldBlock)
}
fn credentials(proxy: &url::Url) -> Result<(String, String), String> {
    let decode = |value: &str| {
        percent_encoding::percent_decode_str(value)
            .decode_utf8()
            .map(|value| value.into_owned())
            .map_err(|_| "PROXY_CREDENTIAL_INVALID".to_string())
    };
    Ok((
        decode(proxy.username())?,
        decode(proxy.password().unwrap_or_default())?,
    ))
}
fn connect_tcp(host: &str, port: u16, deadline: Instant) -> Result<Transport, String> {
    let addresses = (host, port)
        .to_socket_addrs()
        .map_err(|_| "NETWORK_DNS_FAILED")?;
    for address in addresses.take(8) {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            break;
        }
        if let Ok(stream) = TcpStream::connect_timeout(&address, remaining) {
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .map_err(|error| error.to_string())?;
            stream
                .set_write_timeout(Some(Duration::from_secs(5)))
                .map_err(|error| error.to_string())?;
            return Ok(Transport {
                tcp: stream,
                deadline: Some(deadline),
            });
        }
    }
    Err("NETWORK_CONNECT_FAILED".into())
}
fn proxy_tunnel(
    proxy: &url::Url,
    host: &str,
    port: u16,
    deadline: Instant,
) -> Result<Transport, String> {
    let mut stream = connect_tcp(
        proxy.host_str().ok_or("PROXY_ADDRESS_INVALID")?,
        proxy
            .port()
            .unwrap_or(if proxy.scheme() == "socks5" { 1080 } else { 80 }),
        deadline,
    )?;
    let (user, password) = credentials(proxy)?;
    if proxy.scheme() == "http" {
        let authority = if host.contains(':') {
            format!("[{host}]:{port}")
        } else {
            format!("{host}:{port}")
        };
        let auth = if user.is_empty() {
            String::new()
        } else {
            format!(
                "Proxy-Authorization: Basic {}\r\n",
                STANDARD.encode(format!("{user}:{password}"))
            )
        };
        write!(
            stream,
            "CONNECT {authority} HTTP/1.1\r\nHost: {authority}\r\n{auth}\r\n"
        )
        .map_err(|_| "PROXY_WRITE_FAILED")?;
        let mut header = Vec::new();
        let mut byte = [0];
        while !header.ends_with(b"\r\n\r\n") {
            if header.len() >= 8192 {
                return Err("PROXY_RESPONSE_TOO_LARGE".into());
            }
            stream
                .read_exact(&mut byte)
                .map_err(|_| "PROXY_HANDSHAKE_FAILED")?;
            header.push(byte[0]);
        }
        let line = std::str::from_utf8(&header)
            .map_err(|_| "PROXY_RESPONSE_INVALID")?
            .lines()
            .next()
            .ok_or("PROXY_RESPONSE_INVALID")?;
        if line.split_whitespace().nth(1) != Some("200") {
            return Err("PROXY_CONNECT_REJECTED".into());
        }
    } else if proxy.scheme() == "socks5" {
        // RFC 1928/1929, over the already bounded TCP connection. No direct fallback.
        let method = if user.is_empty() { 0 } else { 2 };
        stream
            .write_all(&[5, 1, method])
            .map_err(|_| "PROXY_WRITE_FAILED")?;
        let mut reply = [0; 2];
        stream
            .read_exact(&mut reply)
            .map_err(|_| "PROXY_HANDSHAKE_FAILED")?;
        if reply != [5, method] {
            return Err("PROXY_AUTH_METHOD_REJECTED".into());
        }
        if method == 2 {
            if user.len() > 255 || password.len() > 255 {
                return Err("PROXY_CREDENTIAL_INVALID".into());
            }
            let mut auth = vec![1, user.len() as u8];
            auth.extend_from_slice(user.as_bytes());
            auth.push(password.len() as u8);
            auth.extend_from_slice(password.as_bytes());
            stream.write_all(&auth).map_err(|_| "PROXY_WRITE_FAILED")?;
            stream
                .read_exact(&mut reply)
                .map_err(|_| "PROXY_HANDSHAKE_FAILED")?;
            if reply != [1, 0] {
                return Err("PROXY_AUTH_REJECTED".into());
            }
        }
        if host.len() > 255 {
            return Err("NETWORK_HOST_INVALID".into());
        }
        let mut request = vec![5, 1, 0, 3, host.len() as u8];
        request.extend_from_slice(host.as_bytes());
        request.extend_from_slice(&port.to_be_bytes());
        stream
            .write_all(&request)
            .map_err(|_| "PROXY_WRITE_FAILED")?;
        let mut response = [0; 4];
        stream
            .read_exact(&mut response)
            .map_err(|_| "PROXY_HANDSHAKE_FAILED")?;
        if response[..3] != [5, 0, 0] {
            return Err("PROXY_CONNECT_REJECTED".into());
        }
        let bytes = match response[3] {
            1 => 4,
            4 => 16,
            3 => {
                stream
                    .read_exact(&mut reply[..1])
                    .map_err(|_| "PROXY_HANDSHAKE_FAILED")?;
                reply[0] as usize
            }
            _ => return Err("PROXY_RESPONSE_INVALID".into()),
        };
        let mut address = vec![0; bytes + 2];
        stream
            .read_exact(&mut address)
            .map_err(|_| "PROXY_HANDSHAKE_FAILED")?;
    } else {
        return Err("PROXY_SCHEME_UNSUPPORTED".into());
    }
    Ok(stream)
}
fn connect(params: &Value, execution: Execution<'_>) -> Result<(Socket, String), String> {
    let url = params["url"].as_str().unwrap();
    let target = url::Url::parse(url).map_err(|_| "NETWORK_URL_INVALID")?;
    if !matches!(target.scheme(), "ws" | "wss")
        || !target.username().is_empty()
        || target.password().is_some()
        || target.fragment().is_some()
    {
        return Err("NETWORK_URL_DENIED".into());
    }
    let mut routing = target.clone();
    routing
        .set_scheme(if target.scheme() == "wss" {
            "https"
        } else {
            "http"
        })
        .map_err(|_| "NETWORK_URL_INVALID")?;
    let route = crate::network_policy::current().resolve(routing.as_str())?;
    let host = target
        .host_str()
        .ok_or("NETWORK_HOST_INVALID")?
        .trim_matches(['[', ']']);
    let port = target
        .port_or_known_default()
        .ok_or("NETWORK_PORT_INVALID")?;
    execution.check()?;
    let deadline = execution
        .deadline
        .map_or(Instant::now() + Duration::from_secs(10), |deadline| {
            deadline.min(Instant::now() + Duration::from_secs(10))
        });
    let stream = match route.proxy_endpoint {
        Some(proxy) => proxy_tunnel(
            &url::Url::parse(&proxy).map_err(|_| "PROXY_ADDRESS_INVALID")?,
            host,
            port,
            deadline,
        )?,
        None => connect_tcp(host, port, deadline)?,
    };
    execution.check()?;
    let mut request = url
        .into_client_request()
        .map_err(|_| "WEBSOCKET_REQUEST_INVALID")?;
    if let Some(headers) = params["headers"].as_object() {
        if headers.len() > 16 {
            return Err("HTTP_HEADER_LIMIT".into());
        }
        for (name, value) in headers {
            let lower = name.to_ascii_lowercase();
            if lower.starts_with("sec-websocket-")
                || matches!(
                    lower.as_str(),
                    "host"
                        | "connection"
                        | "upgrade"
                        | "proxy-authorization"
                        | "content-length"
                        | "transfer-encoding"
                )
            {
                return Err("HTTP_HEADER_RESERVED".into());
            }
            let name = tungstenite::http::HeaderName::try_from(name.as_str())
                .map_err(|_| "HTTP_HEADER_INVALID")?;
            let value = tungstenite::http::HeaderValue::try_from(value.as_str().unwrap())
                .map_err(|_| "HTTP_HEADER_INVALID")?;
            request.headers_mut().insert(name, value);
        }
    }
    if let Some(protocols) = params["protocols"]
        .as_array()
        .filter(|items| !items.is_empty())
    {
        if protocols.iter().any(|value| {
            !value
                .as_str()
                .unwrap()
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&byte))
        }) {
            return Err("WEBSOCKET_PROTOCOL_INVALID".into());
        }
        request.headers_mut().insert(
            "Sec-WebSocket-Protocol",
            protocols
                .iter()
                .map(|value| value.as_str().unwrap())
                .collect::<Vec<_>>()
                .join(", ")
                .parse()
                .map_err(|_| "WEBSOCKET_PROTOCOL_INVALID")?,
        );
    }
    let connector = if target.scheme() == "wss" {
        let mut roots = ureq::rustls::RootCertStore::empty();
        roots.add_parsable_certificates(rustls_native_certs::load_native_certs().certs);
        if roots.is_empty() {
            return Err("NETWORK_TRUST_STORE_UNAVAILABLE".into());
        }
        let config = ureq::rustls::ClientConfig::builder_with_provider(Arc::new(
            ureq::rustls::crypto::ring::default_provider(),
        ))
        .with_safe_default_protocol_versions()
        .map_err(|_| "NETWORK_TLS_CONFIGURATION_FAILED")?
        .with_root_certificates(roots)
        .with_no_client_auth();
        Connector::Rustls(Arc::new(config))
    } else {
        Connector::Plain
    };
    let config = WebSocketConfig::default()
        .read_buffer_size(4096)
        .write_buffer_size(0)
        .max_write_buffer_size(65536)
        .max_message_size(Some(16384))
        .max_frame_size(Some(16384));
    let (mut socket, response) =
        tungstenite::client_tls_with_config(request, stream, Some(config), Some(connector))
            .map_err(|_| "WEBSOCKET_HANDSHAKE_FAILED")?;
    execution.check()?;
    transport(socket.get_mut()).deadline = None;
    tcp(socket.get_mut())
        .set_nonblocking(true)
        .map_err(|error| error.to_string())?;
    let protocol = response
        .headers()
        .get("Sec-WebSocket-Protocol")
        .map(|value| value.to_str())
        .transpose()
        .map_err(|_| "WEBSOCKET_PROTOCOL_INVALID")?
        .unwrap_or_default()
        .to_owned();
    Ok((socket, protocol))
}

impl Manager {
    pub(super) fn websocket_operation(
        &self,
        owner: &str,
        row: &crate::db::PluginRow,
        request: &cruciblebox_next2_protocol::Request,
        execution: Execution<'_>,
    ) -> Result<Value, String> {
        let recipient = execution.caller.unwrap_or(owner);
        if request.method == "network.websocket.open" {
            let (socket, protocol) = connect(&request.params, execution)?;
            self.check(row)?;
            execution.check()?;
            let mut connections = self
                .websockets
                .0
                .lock()
                .map_err(|_| "WEBSOCKET_STATE_UNAVAILABLE")?;
            if connections.len() >= 16
                || connections
                    .values()
                    .filter(|value| value.session == request.session)
                    .count()
                    >= 4
            {
                return Err("WEBSOCKET_LIMIT".into());
            }
            let id = crate::rand_token::random_token_hex()?;
            connections.insert(
                id.clone(),
                Connection {
                    socket,
                    owner: owner.into(),
                    recipient: recipient.into(),
                    session: request.session.clone(),
                    failure: None,
                    closed: false,
                },
            );
            drop(connections);
            // Session removal precedes cleanup. This check closes the insertion race with disposal.
            let live = self.sessions.lock().map_err(|_| "INTERNAL_ERROR")?.contains_key(&request.session) || self.workers.lock().map_err(|_| "INTERNAL_ERROR")?.values().any(|value| matches!(value,super::Instance::Ready(slot) if slot.token == request.session));
            if !live {
                self.websockets.clear_session(&request.session)?;
                return Err("SESSION_EXPIRED".into());
            }
            Ok(json!({"socketId":id,"protocol":protocol}))
        } else {
            let id = request.params["socketId"].as_str().unwrap();
            let mut connections = self
                .websockets
                .0
                .lock()
                .map_err(|_| "WEBSOCKET_STATE_UNAVAILABLE")?;
            let connection = connections
                .get_mut(id)
                .filter(|value| {
                    value.owner == owner
                        && value.recipient == recipient
                        && value.session == request.session
                })
                .ok_or("WEBSOCKET_AUTHORITY_DENIED")?;
            if request.method == "network.websocket.close" {
                let closed = connection.socket.close(None);
                if let Err(error) = closed {
                    if !blocked(&error)
                        && !matches!(error, Error::ConnectionClosed | Error::AlreadyClosed)
                    {
                        return Err("WEBSOCKET_CLOSE_FAILED".into());
                    }
                }
                connections.remove(id);
                return Ok(json!(true));
            }
            if request.method == "network.websocket.send" {
                if connection.closed || connection.failure.is_some() {
                    return Err("WEBSOCKET_CLOSED".into());
                }
                let message = if let Some(text) = request.params["text"].as_str() {
                    Message::Text(text.to_owned().into())
                } else {
                    let data = STANDARD
                        .decode(request.params["data"].as_str().unwrap())
                        .map_err(|_| "WEBSOCKET_DATA_INVALID")?;
                    if data.len() > 16384 {
                        return Err("WEBSOCKET_MESSAGE_TOO_LARGE".into());
                    }
                    Message::Binary(data.into())
                };
                return match connection.socket.write(message) {
                    Ok(()) => Ok(json!(true)),
                    Err(error) if blocked(&error) => Ok(json!(true)), // Accepted into tungstenite's bounded output buffer; poll flushes it.
                    Err(Error::WriteBufferFull(_)) => Err("WEBSOCKET_BUSY".into()),
                    Err(_) => {
                        connection.failure = Some("WEBSOCKET_WRITE_FAILED".into());
                        Err("WEBSOCKET_WRITE_FAILED".into())
                    }
                };
            }
            let mut messages = Vec::new();
            if !connection.closed && connection.failure.is_none() {
                if let Err(error) = connection.socket.flush() {
                    if !blocked(&error) {
                        connection.failure = Some("WEBSOCKET_WRITE_FAILED".into());
                    }
                }
                for _ in 0..16 {
                    if messages.len() == 2 || connection.failure.is_some() {
                        break;
                    }
                    match connection.socket.read() {
                        Ok(Message::Text(text)) => messages
                            .push(json!({"kind":"text","data":STANDARD.encode(text.as_bytes())})),
                        Ok(Message::Binary(data)) => {
                            messages.push(json!({"kind":"binary","data":STANDARD.encode(data)}))
                        }
                        Ok(Message::Close(_))
                        | Err(Error::ConnectionClosed)
                        | Err(Error::AlreadyClosed) => {
                            connection.closed = true;
                            break;
                        }
                        Ok(_) => {}
                        Err(error) if blocked(&error) => break,
                        Err(_) => {
                            connection.failure = Some("WEBSOCKET_READ_FAILED".into());
                            break;
                        }
                    }
                }
            }
            let mut result = json!({"state":if connection.failure.is_some() {"failed"} else if connection.closed {"closed"} else {"open"},"messages":messages});
            if let Some(failure) = &connection.failure {
                result["error"] = json!(error(failure.clone()));
            }
            Ok(result)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{net::TcpListener, thread};

    #[test]
    #[allow(clippy::result_large_err)] // tungstenite's server callback fixes the response error type.
    fn websocket_proxy_tunnels_authenticate_and_carry_real_frames() {
        for scheme in ["http", "socks5"] {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let proxy = url::Url::parse(&format!(
                "{scheme}://u%40ser:p%3Ass@{}",
                listener.local_addr().unwrap()
            ))
            .unwrap();
            let server = thread::spawn(move || {
                let (mut stream, _) = listener.accept().unwrap();
                stream
                    .set_read_timeout(Some(Duration::from_secs(3)))
                    .unwrap();
                if scheme == "http" {
                    let mut header = Vec::new();
                    while !header.ends_with(b"\r\n\r\n") {
                        let mut byte = [0];
                        stream.read_exact(&mut byte).unwrap();
                        header.push(byte[0]);
                    }
                    let header = String::from_utf8(header).unwrap();
                    assert!(header.starts_with("CONNECT unreachable.invalid:9443 HTTP/1.1\r\n"));
                    assert!(header.contains(&format!(
                        "Proxy-Authorization: Basic {}\r\n",
                        STANDARD.encode("u@ser:p:ss")
                    )));
                    stream
                        .write_all(b"HTTP/1.1 200 Connection established\r\n\r\n")
                        .unwrap();
                } else {
                    let mut greeting = [0; 3];
                    stream.read_exact(&mut greeting).unwrap();
                    assert_eq!(greeting, [5, 1, 2]);
                    stream.write_all(&[5, 2]).unwrap();
                    let mut auth = [0; 2];
                    stream.read_exact(&mut auth).unwrap();
                    assert_eq!(auth, [1, 5]);
                    let mut user = [0; 5];
                    stream.read_exact(&mut user).unwrap();
                    assert_eq!(&user, b"u@ser");
                    let mut length = [0];
                    stream.read_exact(&mut length).unwrap();
                    let mut password = vec![0; length[0] as usize];
                    stream.read_exact(&mut password).unwrap();
                    assert_eq!(&password, b"p:ss");
                    stream.write_all(&[1, 0]).unwrap();
                    let mut request = [0; 5];
                    stream.read_exact(&mut request).unwrap();
                    assert_eq!(&request[..4], &[5, 1, 0, 3]);
                    let mut destination = vec![0; request[4] as usize + 2];
                    stream.read_exact(&mut destination).unwrap();
                    assert_eq!(
                        &destination[..destination.len() - 2],
                        b"unreachable.invalid"
                    );
                    assert_eq!(
                        &destination[destination.len() - 2..],
                        &9443u16.to_be_bytes()
                    );
                    stream.write_all(&[5, 0, 0, 1, 127, 0, 0, 1, 0, 0]).unwrap();
                }
                let mut socket = tungstenite::accept_hdr(
                    stream,
                    |request: &tungstenite::handshake::server::Request, response| {
                        assert!(!request.headers().contains_key("Proxy-Authorization"));
                        Ok(response)
                    },
                )
                .unwrap();
                assert_eq!(socket.read().unwrap(), Message::Text("经代理".into()));
                socket.send(Message::Binary(vec![0, 255].into())).unwrap();
            });
            let stream = proxy_tunnel(
                &proxy,
                "unreachable.invalid",
                9443,
                Instant::now() + Duration::from_secs(3),
            )
            .unwrap();
            let (mut socket, _) =
                tungstenite::client("ws://unreachable.invalid:9443/echo", stream).unwrap();
            socket.send(Message::Text("经代理".into())).unwrap();
            assert_eq!(socket.read().unwrap(), Message::Binary(vec![0, 255].into()));
            server.join().unwrap();
        }
    }

    #[test]
    fn websocket_proxy_rejection_and_deadline_fail_explicitly() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let proxy = url::Url::parse(&format!("http://{}", listener.local_addr().unwrap())).unwrap();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut header = Vec::new();
            while !header.ends_with(b"\r\n\r\n") {
                let mut byte = [0];
                stream.read_exact(&mut byte).unwrap();
                header.push(byte[0]);
            }
            stream
                .write_all(b"HTTP/1.1 407 Proxy Authentication Required\r\n\r\n")
                .unwrap();
        });
        assert!(
            matches!(proxy_tunnel(&proxy, "unreachable.invalid", 80, Instant::now()+Duration::from_secs(2)), Err(error) if error == "PROXY_CONNECT_REJECTED")
        );
        server.join().unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener.local_addr().unwrap();
        let server = thread::spawn(move || {
            let (_stream, _) = listener.accept().unwrap();
            thread::sleep(Duration::from_millis(250));
        });
        let mut stream = connect_tcp(
            "127.0.0.1",
            address.port(),
            Instant::now() + Duration::from_millis(80),
        )
        .unwrap();
        assert!(stream.read(&mut [0]).is_err());
        thread::sleep(Duration::from_millis(85));
        assert_eq!(
            stream.read(&mut [0]).unwrap_err().kind(),
            std::io::ErrorKind::TimedOut
        );
        server.join().unwrap();
    }

    #[test]
    #[ignore = "requires isolated self-signed localhost DER certificate and PKCS8 key"]
    fn websocket_tls_rejects_untrusted_certificate() {
        let root = std::path::PathBuf::from(std::env::var_os("CRUCIBLEBOX_TEST_TLS_ROOT").unwrap());
        let cert = ureq::rustls::pki_types::CertificateDer::from(
            std::fs::read(root.join("cert.der")).unwrap(),
        );
        let key = ureq::rustls::pki_types::PrivatePkcs8KeyDer::from(
            std::fs::read(root.join("key.der")).unwrap(),
        );
        let config = ureq::rustls::ServerConfig::builder_with_provider(Arc::new(
            ureq::rustls::crypto::ring::default_provider(),
        ))
        .with_safe_default_protocol_versions()
        .unwrap()
        .with_no_client_auth()
        .with_single_cert(vec![cert], key.into())
        .unwrap();
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("wss://localhost:{}/", listener.local_addr().unwrap().port());
        let server = thread::spawn(move || {
            let (stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(3)))
                .unwrap();
            let connection = ureq::rustls::ServerConnection::new(Arc::new(config)).unwrap();
            let mut tls = ureq::rustls::StreamOwned::new(connection, stream);
            assert!(
                tls.read(&mut [0; 1]).is_err(),
                "untrusted handshake was accepted"
            );
        });
        assert!(
            matches!(connect(&json!({"url":url}), Execution::default()), Err(error) if error == "WEBSOCKET_HANDSHAKE_FAILED")
        );
        server.join().unwrap();
    }
}
