//! TLS with an explicit trust anchor set (v2.1 §21.1, §21.3, NET-01, ADR-032).
//!
//! §21.1 requires certificate *and name* verification, a configurable CA, client certificates
//! and an independent SNI. NET-01's pass criterion adds the part that shapes this module:
//!
//! > 拒绝或显式配置；**无静默 insecure**
//!
//! So there is no `danger_accept_invalid_certs`, no `insecure: bool`, and no builder method
//! that turns verification off. Not "off by default" — absent. A flag that exists is a flag
//! that gets set in a hurry at 2am, and §21.3 is explicit that reachability is never a reason
//! to stop verifying:
//!
//! > 支持明确的 endpoint mapping，但 TCP 目标、TLS 验证名和凭证适用范围分开配置；**不能为了
//! > 可达就关闭 TLS 校验**。
//!
//! That separation is why [`TlsConfig::server_name`] is its own field rather than being taken
//! from the host you connected to: a cluster node announcing `10.0.0.7` is still verified
//! against the name the operator configured.
//!
//! ## What replaces an insecure switch
//!
//! Two things, because "just verify properly" is not advice if the operator cannot see what
//! failed:
//!
//! 1. **An explicit CA set.** A private CA is configured, not worked around. The set is
//!    hashed into [`TlsConfig::ca_set_hash`], which is what §21.3's `TrustIdentity::Ca`
//!    carries — so a credential is bound to *which* CA vouched for the server, and moving to
//!    a different CA is a different identity rather than the same one reached differently.
//! 2. **Errors that name the failing check.** [`TlsError::Verification`] keeps rustls's
//!    reason. An operator told "handshake failed" reaches for `--insecure`; one told "the
//!    certificate is for `other.example`, not `redis.internal`" fixes the name.

use rustls::pki_types::pem::PemObject as _;
use rustls::pki_types::{CertificateDer, PrivateKeyDer, ServerName};
use rustls::{ClientConfig, ClientConnection, RootCertStore, StreamOwned};
use std::error::Error as _;
use std::io::Write;
use std::net::TcpStream;
use std::sync::Arc;
use std::time::Duration;

/// Why a TLS connection could not be made.
#[derive(Debug, thiserror::Error)]
pub enum TlsError {
    /// The configured CA set could not be read.
    #[error("the CA bundle is not valid PEM: {0}")]
    CaBundle(String),
    /// The CA bundle parsed but contained no certificate.
    #[error("the CA bundle contains no certificate")]
    EmptyCaBundle,
    /// The client certificate or key could not be read.
    #[error("the client certificate is not usable: {0}")]
    ClientCert(String),
    /// The name to verify is not a valid DNS name or IP address.
    #[error("{0:?} is not a name a certificate can be checked against")]
    ServerName(String),
    /// The handshake failed a verification check. The reason is kept verbatim.
    #[error("TLS verification failed: {0}")]
    Verification(String),
    /// The socket failed.
    #[error("{0}")]
    Io(#[from] std::io::Error),
    /// The TCP connection could not be made.
    #[error("{0}")]
    Connect(#[from] crate::oneshot::CallError),
    /// rustls rejected the configuration itself.
    #[error("TLS configuration rejected: {0}")]
    Config(String),
}

/// A verified-by-construction TLS client configuration.
///
/// There is no constructor that skips verification, and none that falls back to the system
/// roots silently: the CA set is always something the caller passed in.
#[derive(Clone)]
pub struct TlsConfig {
    roots: Arc<RootCertStore>,
    ca_set_hash: [u8; 32],
    server_name: ServerName<'static>,
    client_auth: Option<(Vec<CertificateDer<'static>>, Arc<PrivateKeyDer<'static>>)>,
}

// Written rather than derived, and `missing_fields_in_debug` is allowed on purpose: omitting
// the private key is the whole point (§23.4). A derived `Debug` would put it in every panic
// message, log line and diagnostic bundle.
#[allow(clippy::missing_fields_in_debug)]
impl std::fmt::Debug for TlsConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // The private key never reaches a log, a panic message or a diagnostic bundle
        // (§23.4), so `Debug` is written rather than derived.
        f.debug_struct("TlsConfig")
            .field("server_name", &self.server_name)
            .field("ca_set_hash", &hex(&self.ca_set_hash))
            .field("client_auth", &self.client_auth.is_some())
            .finish()
    }
}

impl TlsConfig {
    /// Build a configuration from a PEM CA bundle and the name to verify.
    ///
    /// `server_name` is deliberately separate from the host you connect to (§21.3).
    ///
    /// # Errors
    /// [`TlsError::CaBundle`], [`TlsError::EmptyCaBundle`] or [`TlsError::ServerName`].
    pub fn new(ca_pem: &[u8], server_name: &str) -> Result<Self, TlsError> {
        let mut roots = RootCertStore::empty();
        // The DER bytes are hashed in the order they appear, so two operators listing the same
        // CAs in a different order get different identities. That is the conservative
        // direction: it can only cause a re-confirmation, never a silent acceptance.
        let mut hasher = blake3::Hasher::new();
        let mut count = 0usize;
        for cert in CertificateDer::pem_slice_iter(ca_pem) {
            let cert = cert.map_err(|e| TlsError::CaBundle(e.to_string()))?;
            hasher.update(&(cert.len() as u64).to_le_bytes());
            hasher.update(&cert);
            roots
                .add(cert)
                .map_err(|e| TlsError::CaBundle(e.to_string()))?;
            count += 1;
        }
        if count == 0 {
            return Err(TlsError::EmptyCaBundle);
        }

        let name = ServerName::try_from(server_name.to_owned())
            .map_err(|_| TlsError::ServerName(server_name.to_owned()))?;

        Ok(Self {
            roots: Arc::new(roots),
            ca_set_hash: *hasher.finalize().as_bytes(),
            server_name: name,
            client_auth: None,
        })
    }

    /// Present a client certificate during the handshake.
    ///
    /// # Errors
    /// [`TlsError::ClientCert`] if the PEM is unusable.
    pub fn with_client_certificate(
        mut self,
        cert_pem: &[u8],
        key_pem: &[u8],
    ) -> Result<Self, TlsError> {
        let chain: Vec<CertificateDer<'static>> = CertificateDer::pem_slice_iter(cert_pem)
            .collect::<Result<_, _>>()
            .map_err(|e| TlsError::ClientCert(e.to_string()))?;
        if chain.is_empty() {
            return Err(TlsError::ClientCert("no certificate in the PEM".to_owned()));
        }
        let key = PrivateKeyDer::from_pem_slice(key_pem)
            .map_err(|e| TlsError::ClientCert(e.to_string()))?;
        self.client_auth = Some((chain, Arc::new(key)));
        Ok(self)
    }

    /// The name that will be verified, which may differ from the TCP host (§21.3).
    #[must_use]
    pub fn server_name(&self) -> String {
        match &self.server_name {
            ServerName::DnsName(d) => d.as_ref().to_owned(),
            ServerName::IpAddress(ip) => {
                let ip: std::net::IpAddr = (*ip).into();
                ip.to_string()
            }
            _ => "<unknown>".to_owned(),
        }
    }

    /// blake3 over the CA set's DER bytes — §21.3's `TrustIdentity::Ca { ca_set_hash, .. }`.
    ///
    /// A credential is bound to *which* CA vouched for the server. Changing the CA set is a
    /// different trust identity, not the same one reached a different way.
    #[must_use]
    pub fn ca_set_hash(&self) -> [u8; 32] {
        self.ca_set_hash
    }

    fn build(&self) -> Result<ClientConfig, TlsError> {
        let builder = ClientConfig::builder().with_root_certificates((*self.roots).clone());
        match &self.client_auth {
            None => Ok(builder.with_no_client_auth()),
            Some((chain, key)) => builder
                .with_client_auth_cert(chain.clone(), key.clone_key())
                .map_err(|e| TlsError::Config(e.to_string())),
        }
    }
}

/// A TLS connection.
pub type TlsStream = StreamOwned<ClientConnection, TcpStream>;

/// Connect to `host:port` and complete a TLS handshake verified against `cfg`.
///
/// The TCP target and the verified name are separate arguments on purpose. Connecting to an
/// address while checking a different name is a supported, configured thing (§21.3); it is not
/// the same as not checking.
///
/// # Errors
/// [`TlsError::Verification`] when a certificate check fails — with rustls's own reason kept,
/// because an operator who is told only "handshake failed" reaches for an insecure flag.
pub fn connect(
    host: &str,
    port: u16,
    cfg: &TlsConfig,
    timeout: Duration,
) -> Result<TlsStream, TlsError> {
    let tcp = crate::oneshot::connect_tcp(host, port, timeout)?;
    handshake(tcp, cfg)
}

/// Complete a handshake over an already-connected socket.
///
/// # Errors
/// As [`connect`].
pub fn handshake(tcp: TcpStream, cfg: &TlsConfig) -> Result<TlsStream, TlsError> {
    let client = cfg.build()?;
    let conn = ClientConnection::new(Arc::new(client), cfg.server_name.clone())
        .map_err(|e| TlsError::Config(e.to_string()))?;
    let mut stream = StreamOwned::new(conn, tcp);

    // Drive the handshake here rather than lazily on first use, so a verification failure is
    // reported by `connect` instead of surfacing later as a confusing read error.
    if let Err(e) = stream.flush() {
        return Err(classify(e, &stream));
    }
    while stream.conn.is_handshaking() {
        match stream.conn.complete_io(&mut stream.sock) {
            Ok(_) => {}
            Err(e) => return Err(classify(e, &stream)),
        }
    }
    Ok(stream)
}

/// Turn an I/O error that is really a TLS alert into one that says which check failed.
fn classify(e: std::io::Error, _s: &TlsStream) -> TlsError {
    // rustls reports verification failures as `io::Error` with the `rustls::Error` inside.
    let mut src: Option<&(dyn std::error::Error + 'static)> = e.source();
    while let Some(s) = src {
        if let Some(r) = s.downcast_ref::<rustls::Error>() {
            return TlsError::Verification(r.to_string());
        }
        src = s.source();
    }
    if let Some(r) = e.get_ref().and_then(|r| r.downcast_ref::<rustls::Error>()) {
        return TlsError::Verification(r.to_string());
    }
    // `InvalidData` from rustls without a typed source is still a protocol/verification
    // failure rather than a network one, and saying "network error" would send an operator
    // debugging the wrong thing.
    if e.kind() == std::io::ErrorKind::InvalidData {
        return TlsError::Verification(e.to_string());
    }
    TlsError::Io(e)
}

fn hex(b: &[u8]) -> String {
    use std::fmt::Write as _;
    let mut s = String::with_capacity(b.len() * 2);
    for x in b {
        let _ = write!(s, "{x:02x}");
    }
    s
}

/// A compile-time statement of NET-01's "no silent insecure".
///
/// This module exposes no way to disable verification. The test suite checks the behaviour;
/// this constant exists so the intent is visible at the point someone would add one.
pub const VERIFICATION_IS_NOT_OPTIONAL: &str = "NET-01: there is no insecure mode. Configure the CA (TlsConfig::new) or the name \
     (§21.3 separates the TCP target from the verified name). Reachability is never a reason \
     to stop verifying.";
