//! This node's certificate, HTTPS, and the pinned client nodes reach each other with
//! (ADR-012 §2.3).
//!
//! Every node makes its own certificate on first start: self-signed, ECDSA P-256, kept in
//! `<system dir>/tls/`. What another node or an engineer checks is its **fingerprint**, the
//! SHA-256 of the certificate: shown in Settings → Node, announced by discovery, and pinned when
//! two nodes pair. No certificate authority is involved, and none is trusted.
//!
//! The crypto is `ring` throughout (rustls, rcgen), so it builds the same on Linux, macOS and
//! Windows.

use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, PrivatePkcs8KeyDer, ServerName, UnixTime};
use rustls::{DigitallySignedStruct, SignatureScheme};
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

/// This node's certificate and key.
pub struct NodeCert {
    pub cert_der: Vec<u8>,
    key_der: Vec<u8>,
    /// SHA-256 of the certificate, lowercase hex.
    pub fingerprint: String,
}

#[derive(Debug, thiserror::Error)]
pub enum TlsError {
    #[error("the node's certificate cannot be read or written: {0}")]
    Io(#[from] std::io::Error),
    #[error("the node's certificate cannot be made: {0}")]
    Make(String),
    #[error("TLS: {0}")]
    Tls(String),
}

/// SHA-256 of a certificate, lowercase hex: what is compared when nodes pair.
pub fn fingerprint(cert_der: &[u8]) -> String {
    hex::encode(Sha256::digest(cert_der))
}

/// The fingerprint grouped in fours, as a person reads it aloud to compare.
pub fn spoken(fingerprint: &str) -> String {
    fingerprint
        .as_bytes()
        .chunks(4)
        .map(|c| String::from_utf8_lossy(c).to_string())
        .collect::<Vec<_>>()
        .join(" ")
}

fn ring() -> Arc<rustls::crypto::CryptoProvider> {
    Arc::new(rustls::crypto::ring::default_provider())
}

impl NodeCert {
    /// Read `<system dir>/tls/node.{crt,key}`, or make them: valid for `localhost`, the
    /// loopback addresses and `names` (the node's mDNS host name).
    pub fn load_or_create(system_dir: &Path, names: &[String]) -> Result<NodeCert, TlsError> {
        let dir = system_dir.join("tls");
        let cert_path = dir.join("node.crt");
        let key_path = dir.join("node.key");
        if cert_path.is_file() && key_path.is_file() {
            let cert_der = std::fs::read(&cert_path)?;
            let key_der = std::fs::read(&key_path)?;
            return Ok(NodeCert {
                fingerprint: fingerprint(&cert_der),
                cert_der,
                key_der,
            });
        }
        let mut subject_names = vec![
            "localhost".to_string(),
            "127.0.0.1".to_string(),
            "::1".to_string(),
        ];
        subject_names.extend(names.iter().cloned());
        let key = rcgen::KeyPair::generate_for(&rcgen::PKCS_ECDSA_P256_SHA256)
            .map_err(|e| TlsError::Make(e.to_string()))?;
        let mut params = rcgen::CertificateParams::new(subject_names)
            .map_err(|e| TlsError::Make(e.to_string()))?;
        params
            .distinguished_name
            .push(rcgen::DnType::CommonName, "MythOS node");
        // Long enough never to lapse under a running plant; trust is the pinned fingerprint,
        // not the dates.
        params.not_before = rcgen::date_time_ymd(2025, 1, 1);
        params.not_after = rcgen::date_time_ymd(2125, 1, 1);
        let cert = params
            .self_signed(&key)
            .map_err(|e| TlsError::Make(e.to_string()))?;
        let cert_der = cert.der().to_vec();
        let key_der = key.serialize_der();
        std::fs::create_dir_all(&dir)?;
        write_private(&key_path, &key_der)?;
        std::fs::write(&cert_path, &cert_der)?;
        Ok(NodeCert {
            fingerprint: fingerprint(&cert_der),
            cert_der,
            key_der,
        })
    }

    /// What a TLS server presents.
    pub fn server_config(&self) -> Result<Arc<rustls::ServerConfig>, TlsError> {
        let mut config = rustls::ServerConfig::builder_with_provider(ring())
            .with_safe_default_protocol_versions()
            .map_err(|e| TlsError::Tls(e.to_string()))?
            .with_no_client_auth()
            .with_single_cert(
                vec![CertificateDer::from(self.cert_der.clone())],
                PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(self.key_der.clone())),
            )
            .map_err(|e| TlsError::Tls(e.to_string()))?;
        config.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];
        Ok(Arc::new(config))
    }
}

/// Write a private key so that only this account can read it (on Unix).
fn write_private(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(path)?;
        file.write_all(bytes)
    }
    #[cfg(not(unix))]
    {
        std::fs::write(path, bytes)
    }
}

/// The address a request over TLS came from, as its connection says (not as it claims).
#[derive(Debug, Clone, Copy)]
pub struct Remote(pub SocketAddr);

/// Serve `app` over TLS on `listener` until the task is dropped. Each request carries the
/// [`Remote`] address of its connection.
pub async fn serve(
    listener: tokio::net::TcpListener,
    config: Arc<rustls::ServerConfig>,
    app: axum::Router,
) {
    let acceptor = tokio_rustls::TlsAcceptor::from(config);
    loop {
        let (stream, remote) = match listener.accept().await {
            Ok(accepted) => accepted,
            Err(e) => {
                tracing::warn!("HTTPS accept failed: {e}");
                continue;
            }
        };
        let acceptor = acceptor.clone();
        let app = app.clone().layer(axum::Extension(Remote(remote)));
        tokio::spawn(async move {
            // A client that never finishes its handshake must not hold a task forever.
            let handshake =
                tokio::time::timeout(Duration::from_secs(10), acceptor.accept(stream)).await;
            let Ok(Ok(tls)) = handshake else { return };
            let service = hyper_util::service::TowerToHyperService::new(app);
            let _ =
                hyper_util::server::conn::auto::Builder::new(hyper_util::rt::TokioExecutor::new())
                    .serve_connection_with_upgrades(hyper_util::rt::TokioIo::new(tls), service)
                    .await;
        });
    }
}

/// Accepts exactly the certificate whose SHA-256 is `expected`, and checks that the server
/// holds its key: what pairing pins (ADR-012 §2.3). Names and dates are not what is trusted.
#[derive(Debug)]
struct Pinned {
    expected: String,
    provider: Arc<rustls::crypto::CryptoProvider>,
}

impl ServerCertVerifier for Pinned {
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, rustls::Error> {
        if fingerprint(end_entity.as_ref()) == self.expected {
            Ok(ServerCertVerified::assertion())
        } else {
            Err(rustls::Error::General(format!(
                "the node's certificate is not the one expected: it is {}, not {}",
                fingerprint(end_entity.as_ref()),
                self.expected
            )))
        }
    }

    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls12_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, rustls::Error> {
        rustls::crypto::verify_tls13_signature(
            message,
            cert,
            dss,
            &self.provider.signature_verification_algorithms,
        )
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.provider
            .signature_verification_algorithms
            .supported_schemes()
    }
}

/// What another node answered.
#[derive(Debug)]
pub struct Answer {
    pub status: u16,
    pub body: serde_json::Value,
}

/// One JSON request to another node over TLS, accepting only the certificate whose fingerprint
/// is `fingerprint`. Blocking: call it from `spawn_blocking`.
pub fn request_pinned(
    address: SocketAddr,
    fingerprint: &str,
    method: &str,
    path: &str,
    bearer: Option<&str>,
    body: Option<&serde_json::Value>,
) -> Result<Answer, String> {
    let mut stream = connect_pinned(address, fingerprint, Duration::from_secs(15))?;
    let body_text = body.map(|b| b.to_string()).unwrap_or_default();
    let mut request = format!(
        "{method} {path} HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\nAccept: application/json\r\nContent-Length: {}\r\n",
        body_text.len()
    );
    if body.is_some() {
        request.push_str("Content-Type: application/json\r\n");
    }
    if let Some(token) = bearer {
        request.push_str(&format!("Authorization: Bearer {token}\r\n"));
    }
    request.push_str("\r\n");
    request.push_str(&body_text);
    stream
        .write_all(request.as_bytes())
        .map_err(|e| format!("{address}: {e}"))?;
    let mut raw = Vec::new();
    match stream.read_to_end(&mut raw) {
        Ok(_) => {}
        // A peer that closes without close_notify after a complete answer is common.
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof && !raw.is_empty() => {}
        Err(e) => return Err(format!("{address}: {e}")),
    }
    let text = String::from_utf8_lossy(&raw);
    let (head, rest) = text
        .split_once("\r\n\r\n")
        .ok_or_else(|| format!("{address} answered something that is not HTTP"))?;
    let status: u16 = head
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| format!("{address} answered without a status"))?;
    let chunked = head.lines().any(|l| {
        l.to_ascii_lowercase()
            .starts_with("transfer-encoding: chunked")
    });
    let payload = if chunked {
        dechunk(rest)
    } else {
        rest.to_string()
    };
    let body = if payload.trim().is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::from_str(&payload).unwrap_or(serde_json::Value::String(payload))
    };
    Ok(Answer { status, body })
}

/// A TLS connection to another node, accepting only the certificate whose fingerprint is
/// `fingerprint`.
fn connect_pinned(
    address: SocketAddr,
    fingerprint: &str,
    read_timeout: Duration,
) -> Result<rustls::StreamOwned<rustls::ClientConnection, TcpStream>, String> {
    let provider = ring();
    let config = rustls::ClientConfig::builder_with_provider(Arc::clone(&provider))
        .with_safe_default_protocol_versions()
        .map_err(|e| e.to_string())?
        .dangerous()
        .with_custom_certificate_verifier(Arc::new(Pinned {
            expected: fingerprint.to_ascii_lowercase(),
            provider,
        }))
        .with_no_client_auth();
    let name = ServerName::try_from("mythos-node").map_err(|e| e.to_string())?;
    let connection =
        rustls::ClientConnection::new(Arc::new(config), name).map_err(|e| e.to_string())?;
    let tcp = TcpStream::connect_timeout(&address, Duration::from_secs(5))
        .map_err(|e| format!("{address} cannot be reached: {e}"))?;
    tcp.set_read_timeout(Some(read_timeout))
        .map_err(|e| e.to_string())?;
    Ok(rustls::StreamOwned::new(connection, tcp))
}

/// Follow a server-sent event stream on another node over TLS pinned to `fingerprint`, calling
/// `on_event(event, data)` for every event until it returns `false` (then `Ok`). Anything else
/// that ends it is the `Err`: the node cannot be reached, presents another certificate, refuses,
/// ends the stream, or says nothing for `silence`. Blocking: call it from a thread of its own.
pub fn stream_pinned(
    address: SocketAddr,
    fingerprint: &str,
    path: &str,
    bearer: Option<&str>,
    silence: Duration,
    mut on_event: impl FnMut(&str, &str) -> bool,
) -> Result<(), String> {
    let mut stream = connect_pinned(address, fingerprint, silence)?;
    let mut request = format!(
        "GET {path} HTTP/1.1\r\nHost: {address}\r\nConnection: close\r\nAccept: text/event-stream\r\n"
    );
    if let Some(token) = bearer {
        request.push_str(&format!("Authorization: Bearer {token}\r\n"));
    }
    request.push_str("\r\n");
    stream
        .write_all(request.as_bytes())
        .map_err(|e| format!("{address}: {e}"))?;
    let mut reader = std::io::BufReader::new(stream);
    let said = |e: std::io::Error| match e.kind() {
        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut => {
            format!("nothing heard from {address} for {} s", silence.as_secs())
        }
        _ => format!("{address}: {e}"),
    };
    let mut line = String::new();
    let read_line = |reader: &mut std::io::BufReader<_>, line: &mut String| {
        line.clear();
        match std::io::BufRead::read_line(reader, line) {
            Ok(0) => Err(format!("{address} ended the stream")),
            Ok(_) => Ok(()),
            Err(e) => Err(said(e)),
        }
    };
    read_line(&mut reader, &mut line)?;
    let status: u16 = line
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| format!("{address} answered something that is not HTTP"))?;
    let mut chunked = false;
    loop {
        read_line(&mut reader, &mut line)?;
        let header = line.trim_end();
        if header.is_empty() {
            break;
        }
        let lower = header.to_ascii_lowercase();
        if lower.starts_with("transfer-encoding:") && lower.contains("chunked") {
            chunked = true;
        }
    }
    if status != 200 {
        return Err(format!("{address} refused the stream: HTTP {status}"));
    }

    let mut events = SseEvents::default();
    let mut buf = vec![0u8; 16 * 1024];
    loop {
        let bytes: Vec<u8> = if chunked {
            read_line(&mut reader, &mut line)?;
            let size = line.trim().split(';').next().unwrap_or("");
            let size = usize::from_str_radix(size.trim(), 16)
                .map_err(|_| format!("{address} sent a malformed chunk"))?;
            if size == 0 {
                return Err(format!("{address} ended the stream"));
            }
            let mut chunk = vec![0u8; size];
            reader.read_exact(&mut chunk).map_err(said)?;
            let mut crlf = [0u8; 2];
            reader.read_exact(&mut crlf).map_err(said)?;
            chunk
        } else {
            let n = reader.read(&mut buf).map_err(said)?;
            if n == 0 {
                return Err(format!("{address} ended the stream"));
            }
            buf[..n].to_vec()
        };
        for (event, data) in events.feed(&bytes) {
            if !on_event(&event, &data) {
                return Ok(());
            }
        }
    }
}

/// Server-sent events, decoded from bytes as they arrive.
#[derive(Default)]
struct SseEvents {
    pending: Vec<u8>,
    event: String,
    data: Vec<String>,
}

impl SseEvents {
    fn feed(&mut self, bytes: &[u8]) -> Vec<(String, String)> {
        self.pending.extend_from_slice(bytes);
        let mut out = Vec::new();
        while let Some(end) = self.pending.iter().position(|&b| b == b'\n') {
            let raw: Vec<u8> = self.pending.drain(..=end).collect();
            let text = String::from_utf8_lossy(&raw);
            let line = text.trim_end_matches(['\n', '\r']);
            if line.is_empty() {
                if !self.data.is_empty() {
                    let event = if self.event.is_empty() {
                        "message".to_string()
                    } else {
                        std::mem::take(&mut self.event)
                    };
                    out.push((event, self.data.join("\n")));
                }
                self.event.clear();
                self.data.clear();
            } else if line.starts_with(':') {
                // A comment: the stream is alive, nothing to say.
            } else {
                let (field, value) = line.split_once(':').unwrap_or((line, ""));
                let value = value.strip_prefix(' ').unwrap_or(value);
                match field {
                    "event" => self.event = value.to_string(),
                    "data" => self.data.push(value.to_string()),
                    _ => {}
                }
            }
        }
        out
    }
}

fn dechunk(mut text: &str) -> String {
    let mut out = String::new();
    while let Some((size, rest)) = text.split_once("\r\n") {
        let Ok(size) = usize::from_str_radix(size.trim(), 16) else {
            break;
        };
        if size == 0 || rest.len() < size {
            break;
        }
        out.push_str(&rest[..size]);
        text = rest[size..].trim_start_matches("\r\n");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_node_keeps_its_certificate_and_its_fingerprint() {
        let dir = tempfile::tempdir().unwrap();
        let first =
            NodeCert::load_or_create(dir.path(), &["mythos-7f3c2a10.local".into()]).unwrap();
        assert_eq!(first.fingerprint.len(), 64);
        let again = NodeCert::load_or_create(dir.path(), &[]).unwrap();
        assert_eq!(again.fingerprint, first.fingerprint, "made once, then kept");
        assert!(first.server_config().is_ok());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(dir.path().join("tls/node.key"))
                .unwrap()
                .permissions()
                .mode();
            assert_eq!(mode & 0o077, 0, "the key is readable by its owner only");
        }
    }

    #[test]
    fn a_fingerprint_is_read_in_groups_of_four() {
        assert_eq!(spoken("0123456789abcdef"), "0123 4567 89ab cdef");
    }

    #[test]
    fn a_chunked_body_is_put_back_together() {
        assert_eq!(dechunk("4\r\n{\"a\"\r\n3\r\n:1}\r\n0\r\n\r\n"), "{\"a\":1}");
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn the_pinned_client_reaches_the_node_it_expects_and_refuses_any_other() {
        let dir = tempfile::tempdir().unwrap();
        let cert = NodeCert::load_or_create(dir.path(), &[]).unwrap();
        let other = NodeCert::load_or_create(tempfile::tempdir().unwrap().path(), &[]).unwrap();
        let app = axum::Router::new().route(
            "/hello",
            axum::routing::post(|body: String| async move {
                axum::Json(serde_json::json!({ "echo": body }))
            }),
        );
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let config = cert.server_config().unwrap();
        tokio::spawn(serve(listener, config, app));

        let right = cert.fingerprint.clone();
        let answer = tokio::task::spawn_blocking(move || {
            request_pinned(
                address,
                &right,
                "POST",
                "/hello",
                None,
                Some(&serde_json::json!({"x": 1})),
            )
        })
        .await
        .unwrap()
        .unwrap();
        assert_eq!(answer.status, 200);
        assert_eq!(answer.body["echo"], "{\"x\":1}");

        let wrong = other.fingerprint.clone();
        let refused = tokio::task::spawn_blocking(move || {
            request_pinned(address, &wrong, "GET", "/hello", None, None)
        })
        .await
        .unwrap()
        .unwrap_err();
        assert!(refused.contains("not the one expected"), "{refused}");
    }
}
