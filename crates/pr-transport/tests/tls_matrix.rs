//! V-G04 / NET-01 — the TLS matrix (v2.1 §21.1, §21.3, ADR-032).
//!
//! > NET-01 | TLS 错名/过期/未知 CA | **拒绝或显式配置；无静默 insecure**
//!
//! Every certificate here is generated at test time from a self-signed CA. Committing them
//! would be worse than pointless: a fixture with a fixed `notAfter` becomes a test that fails
//! on a date nobody chose, and the "expired" case would eventually be indistinguishable from
//! the "valid" one.
//!
//! The negative cases are the point. A TLS test that only proves a good certificate connects
//! has proved that the library works, not that we use it correctly — the whole value is in
//! what gets refused, and in the refusal *saying which check failed*, because an operator told
//! only "handshake failed" reaches for an insecure flag.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, missing_docs)]

use pr_transport::tls::{TlsConfig, TlsError};
use rcgen::{
    BasicConstraints, CertificateParams, DnType, IsCa, Issuer, KeyPair, KeyUsagePurpose, SanType,
};
use rustls::pki_types::pem::PemObject as _;
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Arc;
use std::time::Duration;

const TIMEOUT: Duration = Duration::from_secs(5);

/// A generated certificate and its key, in PEM.
struct Pem {
    cert: String,
    key: String,
}

struct Ca {
    issuer: Issuer<'static, KeyPair>,
    pem: String,
}

fn ca(name: &str) -> Ca {
    let key = KeyPair::generate().unwrap();
    let mut params = CertificateParams::new(Vec::new()).unwrap();
    params.is_ca = IsCa::Ca(BasicConstraints::Constrained(1));
    params.key_usages = vec![KeyUsagePurpose::KeyCertSign, KeyUsagePurpose::CrlSign];
    params
        .distinguished_name
        .push(DnType::CommonName, name.to_owned());
    let cert = params.self_signed(&key).unwrap();
    let pem = cert.pem();
    Ca {
        issuer: Issuer::new(params, key),
        pem,
    }
}

/// Issue a leaf for `names`, optionally back-dated so it is already expired.
fn leaf(ca: &Ca, names: &[&str], expired: bool) -> Pem {
    let key = KeyPair::generate().unwrap();
    let mut params = CertificateParams::new(Vec::new()).unwrap();
    for n in names {
        params.subject_alt_names.push(match n.parse() {
            Ok(ip) => SanType::IpAddress(ip),
            Err(_) => SanType::DnsName((*n).to_owned().try_into().unwrap()),
        });
    }
    params
        .distinguished_name
        .push(DnType::CommonName, names[0].to_owned());
    if expired {
        // Two years ago to one year ago: unambiguously past, and it stays past.
        params.not_before = rcgen::date_time_ymd(2020, 1, 1);
        params.not_after = rcgen::date_time_ymd(2021, 1, 1);
    }
    let cert = params.signed_by(&key, &ca.issuer).unwrap();
    Pem {
        cert: cert.pem(),
        key: key.serialize_pem(),
    }
}

/// A TLS server that completes one handshake and echoes `+PONG`.
///
/// Real enough to prove the handshake succeeded end to end: a test that stopped at "no error
/// yet" would pass against a connection that was never actually usable.
struct Server {
    port: u16,
    handle: std::thread::JoinHandle<Option<String>>,
}

fn server(leaf: &Pem, client_ca: Option<&str>) -> Server {
    let certs: Vec<_> = CertificateDer::pem_slice_iter(leaf.cert.as_bytes())
        .collect::<Result<_, _>>()
        .unwrap();
    let key = PrivateKeyDer::from_pem_slice(leaf.key.as_bytes()).unwrap();

    let config = match client_ca {
        None => rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(certs, key)
            .unwrap(),
        Some(pem) => {
            let mut roots = rustls::RootCertStore::empty();
            for c in CertificateDer::pem_slice_iter(pem.as_bytes()) {
                roots.add(c.unwrap()).unwrap();
            }
            let verifier = rustls::server::WebPkiClientVerifier::builder(Arc::new(roots))
                .build()
                .unwrap();
            rustls::ServerConfig::builder()
                .with_client_cert_verifier(verifier)
                .with_single_cert(certs, key)
                .unwrap()
        }
    };

    let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = listener.local_addr().unwrap().port();
    let config = Arc::new(config);

    let handle = std::thread::spawn(move || {
        let (sock, _) = listener.accept().ok()?;
        let conn = rustls::ServerConnection::new(config).ok()?;
        let mut tls = rustls::StreamOwned::new(conn, sock);
        // Drive the handshake; a client that is about to reject us makes this fail, which is
        // the server's own view of the same event.
        while tls.conn.is_handshaking() {
            if tls.conn.complete_io(&mut tls.sock).is_err() {
                return Some("handshake refused".to_owned());
            }
        }
        let mut buf = [0u8; 64];
        let n = tls.read(&mut buf).ok()?;
        tls.write_all(b"+PONG\r\n").ok()?;
        tls.flush().ok()?;
        Some(String::from_utf8_lossy(&buf[..n]).into_owned())
    });

    Server { port, handle }
}

/// Connect, verify, and prove the connection actually carries data.
fn ping(port: u16, cfg: &TlsConfig) -> Result<String, TlsError> {
    let tcp = TcpStream::connect(("127.0.0.1", port))?;
    tcp.set_read_timeout(Some(TIMEOUT))?;
    tcp.set_write_timeout(Some(TIMEOUT))?;
    let mut stream = pr_transport::tls::handshake(tcp, cfg)?;
    stream.write_all(b"*1\r\n$4\r\nPING\r\n")?;
    stream.flush()?;
    let mut buf = [0u8; 64];
    let n = stream.read(&mut buf)?;
    Ok(String::from_utf8_lossy(&buf[..n]).into_owned())
}

// ---------------------------------------------------------------------------------------------
// The matrix
// ---------------------------------------------------------------------------------------------

#[test]
fn a_correct_certificate_from_the_configured_ca_connects() {
    let ca = ca("Penguin Test CA");
    let leaf = leaf(&ca, &["redis.internal"], false);
    let srv = server(&leaf, None);

    let cfg = TlsConfig::new(ca.pem.as_bytes(), "redis.internal").unwrap();
    let reply = ping(srv.port, &cfg).expect("the handshake should succeed");
    assert_eq!(
        reply, "+PONG\r\n",
        "the connection must actually carry data"
    );
    assert_eq!(
        srv.handle.join().unwrap().as_deref(),
        Some("*1\r\n$4\r\nPING\r\n")
    );
}

#[test]
fn a_certificate_for_a_different_name_is_refused_and_the_error_says_so() {
    // NET-01's first case. The operator must be able to read the error and fix the *name*,
    // which is why the reason is kept rather than flattened to "handshake failed".
    let ca = ca("Penguin Test CA");
    let leaf = leaf(&ca, &["other.example"], false);
    let srv = server(&leaf, None);

    let cfg = TlsConfig::new(ca.pem.as_bytes(), "redis.internal").unwrap();
    let err = ping(srv.port, &cfg).expect_err("a wrong name must be refused");
    let TlsError::Verification(reason) = &err else {
        panic!("a name mismatch must be a verification failure, got {err:?}");
    };
    assert!(
        reason.to_lowercase().contains("name"),
        "the error does not point at the name: {reason}"
    );
    let _ = srv.handle.join();
}

#[test]
fn an_expired_certificate_is_refused_and_the_error_says_so() {
    let ca = ca("Penguin Test CA");
    let leaf = leaf(&ca, &["redis.internal"], true);
    let srv = server(&leaf, None);

    let cfg = TlsConfig::new(ca.pem.as_bytes(), "redis.internal").unwrap();
    let err = ping(srv.port, &cfg).expect_err("an expired certificate must be refused");
    let TlsError::Verification(reason) = &err else {
        panic!("expiry must be a verification failure, got {err:?}");
    };
    assert!(
        reason.to_lowercase().contains("expired") || reason.to_lowercase().contains("time"),
        "the error does not point at the validity period: {reason}"
    );
    let _ = srv.handle.join();
}

#[test]
fn a_certificate_from_an_unknown_ca_is_refused() {
    // The one that matters most operationally: a perfectly valid certificate, for the right
    // name, in date — signed by somebody else.
    let ours = ca("Penguin Test CA");
    let theirs = ca("Somebody Else");
    let leaf = leaf(&theirs, &["redis.internal"], false);
    let srv = server(&leaf, None);

    let cfg = TlsConfig::new(ours.pem.as_bytes(), "redis.internal").unwrap();
    let err = ping(srv.port, &cfg).expect_err("an unknown issuer must be refused");
    let TlsError::Verification(reason) = &err else {
        panic!("an unknown CA must be a verification failure, got {err:?}");
    };
    assert!(
        reason.to_lowercase().contains("issuer") || reason.to_lowercase().contains("unknown"),
        "the error does not point at the issuer: {reason}"
    );
    let _ = srv.handle.join();
}

#[test]
fn a_client_certificate_is_presented_when_configured() {
    let ca = ca("Penguin Test CA");
    let leaf_cert = leaf(&ca, &["redis.internal"], false);
    let client = leaf(&ca, &["penguin-client"], false);
    let srv = server(&leaf_cert, Some(&ca.pem));

    let cfg = TlsConfig::new(ca.pem.as_bytes(), "redis.internal")
        .unwrap()
        .with_client_certificate(client.cert.as_bytes(), client.key.as_bytes())
        .unwrap();
    let reply = ping(srv.port, &cfg).expect("mutual TLS should succeed");
    assert_eq!(reply, "+PONG\r\n");
    let _ = srv.handle.join();
}

#[test]
fn a_server_that_requires_a_client_certificate_refuses_one_that_has_none() {
    // The negative half. Without it, the previous test passes against a server that never
    // actually asked.
    let ca = ca("Penguin Test CA");
    let leaf_cert = leaf(&ca, &["redis.internal"], false);
    let srv = server(&leaf_cert, Some(&ca.pem));

    let cfg = TlsConfig::new(ca.pem.as_bytes(), "redis.internal").unwrap();
    let outcome = ping(srv.port, &cfg);
    assert!(
        outcome.is_err(),
        "the server demanded a client certificate and we sent none, yet: {outcome:?}"
    );
    let _ = srv.handle.join();
}

#[test]
fn the_verified_name_is_independent_of_the_address_connected_to() {
    // §21.3: 「TCP 目标、TLS 验证名和凭证适用范围分开配置；不能为了可达就关闭 TLS 校验」.
    //
    // A cluster node announcing an address you can reach does not get to choose the name its
    // certificate is checked against. Connecting to 127.0.0.1 while verifying
    // `redis.internal` is the supported, configured way to do this — and it is not the same
    // as not checking, which the second half of this test shows.
    let ca = ca("Penguin Test CA");
    let leaf_cert = leaf(&ca, &["redis.internal"], false);

    let srv = server(&leaf_cert, None);
    let cfg = TlsConfig::new(ca.pem.as_bytes(), "redis.internal").unwrap();
    assert_eq!(cfg.server_name(), "redis.internal");
    assert_eq!(ping(srv.port, &cfg).unwrap(), "+PONG\r\n");
    let _ = srv.handle.join();

    // Same certificate, same address, name taken from the address instead: refused.
    let srv = server(&leaf_cert, None);
    let cfg = TlsConfig::new(ca.pem.as_bytes(), "127.0.0.1").unwrap();
    assert!(
        matches!(ping(srv.port, &cfg), Err(TlsError::Verification(_))),
        "verifying against the address must fail when the certificate names a host"
    );
    let _ = srv.handle.join();
}

// ---------------------------------------------------------------------------------------------
// "无静默 insecure"
// ---------------------------------------------------------------------------------------------

#[test]
fn there_is_no_configuration_that_turns_verification_off() {
    // The structural half of NET-01. `TlsConfig` has two constructors and one builder method;
    // none of them takes a flag, and the CA set is always something the caller supplied.
    //
    // This is asserted over the source because the absence of an API cannot be asserted by
    // calling it. If somebody adds `danger_accept_invalid_certs`, this fails on the same
    // commit rather than on the day it is first used.
    let code = strip_comments_and_strings(include_str!("../src/tls.rs"));
    for banned in [
        "danger_accept_invalid_certs",
        "dangerous",
        "set_certificate_verifier",
        "ServerCertVerifier",
        "insecure",
        "skip_verify",
        "accept_invalid",
        "NoServerAuth",
    ] {
        assert!(
            !code.contains(banned),
            "{banned} appears in the TLS module's code"
        );
    }
    // And the module still *says* so, which is what a reader adding a flag would see first.
    assert!(pr_transport::tls::VERIFICATION_IS_NOT_OPTIONAL.contains("no insecure mode"));
}

/// Remove comments and string literals so a ban applies to code rather than to prose.
///
/// Without this, documenting "there is no insecure mode" would trip the check that there is no
/// insecure mode — and the fix people reach for is to stop documenting it.
fn strip_comments_and_strings(src: &str) -> String {
    let b = src.as_bytes();
    let mut out = String::with_capacity(src.len());
    let mut i = 0usize;
    while i < b.len() {
        if b[i] == b'/' && i + 1 < b.len() && b[i + 1] == b'/' {
            while i < b.len() && b[i] != b'\n' {
                i += 1;
            }
        } else if b[i] == b'"' {
            i += 1;
            while i < b.len() && b[i] != b'"' {
                if b[i] == b'\\' {
                    i += 1;
                }
                i += 1;
            }
            i += 1;
        } else {
            out.push(b[i] as char);
            i += 1;
        }
    }
    out
}

#[test]
fn an_empty_or_unreadable_ca_bundle_is_an_error_not_a_fallback() {
    // The quiet way an insecure mode gets built: an unreadable CA file that falls back to the
    // system roots, or to nothing. Both must be refusals.
    assert!(matches!(
        TlsConfig::new(b"", "redis.internal"),
        Err(TlsError::EmptyCaBundle)
    ));
    assert!(matches!(
        TlsConfig::new(b"not a pem file at all", "redis.internal"),
        Err(TlsError::EmptyCaBundle)
    ));
    let ca = ca("Penguin Test CA");
    assert!(matches!(
        TlsConfig::new(ca.pem.as_bytes(), "not a valid name"),
        Err(TlsError::ServerName(_))
    ));
}

#[test]
fn the_ca_set_hash_is_what_binds_a_credential_to_an_issuer() {
    // §21.3's `TrustIdentity::Ca { ca_set_hash, server_name }`. Two different CAs vouching for
    // the same name are two different trust identities, so a credential saved for one is not
    // automatically sent to the other.
    let a = ca("CA A");
    let b = ca("CA B");
    let ha = TlsConfig::new(a.pem.as_bytes(), "redis.internal")
        .unwrap()
        .ca_set_hash();
    let hb = TlsConfig::new(b.pem.as_bytes(), "redis.internal")
        .unwrap()
        .ca_set_hash();
    assert_ne!(ha, hb, "different CAs must produce different identities");

    // Stable across construction, or the identity would change every process start.
    let ha2 = TlsConfig::new(a.pem.as_bytes(), "other.internal")
        .unwrap()
        .ca_set_hash();
    assert_eq!(
        ha, ha2,
        "the CA set hash must not depend on the server name"
    );

    // Adding a CA changes it: widening who may vouch is a change of identity, not a detail.
    let both = format!("{}{}", a.pem, b.pem);
    let hboth = TlsConfig::new(both.as_bytes(), "redis.internal")
        .unwrap()
        .ca_set_hash();
    assert_ne!(hboth, ha);
    assert_ne!(hboth, hb);
}

#[test]
fn the_private_key_never_reaches_a_debug_line() {
    // §23.4: a diagnostic bundle, a panic message and a log line all go through `Debug`.
    let ca = ca("Penguin Test CA");
    let client = leaf(&ca, &["penguin-client"], false);
    let cfg = TlsConfig::new(ca.pem.as_bytes(), "redis.internal")
        .unwrap()
        .with_client_certificate(client.cert.as_bytes(), client.key.as_bytes())
        .unwrap();

    let rendered = format!("{cfg:?}");
    assert!(rendered.contains("redis.internal"));
    assert!(rendered.contains("client_auth: true"));
    assert!(
        !rendered.contains("PRIVATE KEY") && !rendered.contains("BEGIN"),
        "the key leaked into Debug: {rendered}"
    );
    // And nothing from the key's own bytes either.
    let key_body: String = client
        .key
        .lines()
        .filter(|l| !l.starts_with("-----"))
        .collect();
    assert!(!key_body.is_empty());
    assert!(!rendered.contains(&key_body[..32.min(key_body.len())]));
}
