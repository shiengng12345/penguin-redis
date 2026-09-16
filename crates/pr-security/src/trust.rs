//! `TrustIdentity` — what a credential and an approval are actually bound to
//! (v2.1 §21.3, ADR-010, R14).
//!
//! Everywhere the blueprint said "endpoint fingerprint", "trust fingerprint" or
//! "service identity", it means this structure. The three-way review found those terms
//! undefined, which left two bad options in the field: bind to `host:port` and ship
//! credentials to a replaced service, or bind to a certificate and break on every legitimate
//! rotation or failover.
//!
//! The split here: credentials bind to `(tls_identity, server_identity)` — **not** to the
//! network address — so addresses may move freely while identity changes force a re-bind.

use blake3::Hasher;
use std::collections::BTreeSet;

/// Where we connect to. Changing this alone never invalidates a credential.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Endpoint {
    /// TCP host:port. The host is kept as written (DNS name or literal).
    Tcp {
        /// Hostname or IP literal.
        host: String,
        /// Port.
        port: u16,
    },
    /// Unix domain socket.
    Unix {
        /// Socket path.
        path: String,
    },
    /// Reached through an SSH tunnel.
    SshTunnel {
        /// Jump host spec.
        jump: String,
        /// Remote address as seen from the jump host.
        remote: String,
    },
}

/// How the server proves who it is at the TLS layer.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TlsIdentity {
    /// No TLS. Credentials bound to this are only as strong as the network.
    None,
    /// Verified against a CA set, for a given server name.
    Ca {
        /// Stable hash of the trusted CA set.
        ca_set_hash: String,
        /// Expected server name (SNI / certificate name).
        server_name: String,
    },
    /// Pinned to a specific public key (survives certificate renewal with the same key).
    SpkiPin {
        /// SHA-256 of the `SubjectPublicKeyInfo`.
        sha256: String,
    },
}

/// Which logical Redis service this is, independent of which node answers today.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ServerIdentity {
    /// A single server.
    Standalone {
        /// Optional `run_id` prefix. Advisory only: it changes on restart, so it must never
        /// be a reason to refuse a connection.
        run_id_prefix: Option<String>,
    },
    /// A Sentinel-managed master.
    Sentinel {
        /// The master name. Changing this means a different service.
        master_name: String,
        /// Hash of the configured sentinel set. Changes warn but do not invalidate.
        sentinel_set_hash: String,
    },
    /// A cluster, identified by its node-id set.
    Cluster {
        /// Hash of the known node-id set.
        node_id_set_hash: String,
        /// Seed hosts as configured.
        seed_hosts: Vec<String>,
    },
}

/// Which principal we authenticate as.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct AuthIdentity {
    /// ACL username, if any.
    pub username: Option<String>,
    /// Opaque reference into the OS secret store. Never the secret itself (v2.1 §4.3).
    pub secret_ref: String,
}

/// The full identity a profile resolves to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrustIdentity {
    /// Network location.
    pub endpoint: Endpoint,
    /// TLS proof.
    pub tls_identity: TlsIdentity,
    /// Logical service.
    pub server_identity: ServerIdentity,
    /// Principal.
    pub auth_identity: AuthIdentity,
}

/// Something worth saying that is not worth refusing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Advisory {
    /// The set of Sentinels changed while the master name did not.
    ///
    /// Not a different service, so the credential stands. Worth saying because a Sentinel set
    /// that changed without anyone doing it deliberately is a configuration drift, and
    /// noticing it here is cheaper than noticing it during a failover.
    SentinelSetChanged,
}

impl Advisory {
    /// The line to show the user.
    #[must_use]
    pub fn message(self) -> &'static str {
        match self {
            Self::SentinelSetChanged => {
                "the Sentinel set has changed; the master is the same, so the credential still \
                 applies"
            }
        }
    }
}

/// Why a credential must be re-bound (or why it need not be).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RebindVerdict {
    /// Nothing relevant changed.
    NoChange,
    /// Only the address moved — DNS, port mapping, failover to a known node. Keep binding.
    AddressOnly,
    /// Something changed that the user should be told about but which does not invalidate the
    /// credential — §21.3's "警告不阻断".
    ///
    /// Today this is the Sentinel set growing or shrinking. It is a distinct verdict rather
    /// than `NoChange` because a caller cannot warn about something it was told was nothing:
    /// the spec asks for a warning, so the type has to be able to carry one.
    AdvisoryChange(Advisory),
    /// TLS identity changed — re-bind required.
    TlsIdentityChanged,
    /// The logical service changed — re-bind required.
    ServerIdentityChanged,
    /// The principal changed — re-bind required.
    AuthIdentityChanged,
}

impl RebindVerdict {
    /// Whether the credential must be re-confirmed before use.
    #[must_use]
    pub fn requires_rebind(self) -> bool {
        !matches!(
            self,
            Self::NoChange | Self::AddressOnly | Self::AdvisoryChange(_)
        )
    }

    /// The advisory to show the user, if this verdict carries one.
    ///
    /// Separate from [`RebindVerdict::requires_rebind`] on purpose: "tell the user" and "stop
    /// and ask" are different obligations, and collapsing them is how a warning becomes either
    /// a prompt nobody reads or a silence nobody notices.
    #[must_use]
    pub fn advisory(self) -> Option<Advisory> {
        match self {
            Self::AdvisoryChange(a) => Some(a),
            _ => None,
        }
    }
}

fn hash_endpoint(h: &mut Hasher, e: &Endpoint) {
    match e {
        Endpoint::Tcp { host, port } => {
            h.update(b"tcp\0");
            h.update(host.as_bytes());
            h.update(&port.to_le_bytes());
        }
        Endpoint::Unix { path } => {
            h.update(b"unix\0");
            h.update(path.as_bytes());
        }
        Endpoint::SshTunnel { jump, remote } => {
            h.update(b"ssh\0");
            h.update(jump.as_bytes());
            h.update(b"\0");
            h.update(remote.as_bytes());
        }
    }
}

fn hash_tls(h: &mut Hasher, t: &TlsIdentity) {
    match t {
        TlsIdentity::None => h.update(b"tls-none\0"),
        TlsIdentity::Ca {
            ca_set_hash,
            server_name,
        } => {
            h.update(b"tls-ca\0");
            h.update(ca_set_hash.as_bytes());
            h.update(b"\0");
            h.update(server_name.as_bytes())
        }
        TlsIdentity::SpkiPin { sha256 } => {
            h.update(b"tls-spki\0");
            h.update(sha256.as_bytes())
        }
    };
}

fn hash_server(h: &mut Hasher, s: &ServerIdentity) {
    match s {
        // `run_id` is deliberately excluded: it changes on every restart and must not be a
        // reason to invalidate a credential or an approval.
        ServerIdentity::Standalone { .. } => h.update(b"standalone\0"),
        ServerIdentity::Sentinel { master_name, .. } => {
            // The sentinel set is excluded too: adding a sentinel warns, it does not re-bind.
            h.update(b"sentinel\0");
            h.update(master_name.as_bytes())
        }
        ServerIdentity::Cluster {
            node_id_set_hash, ..
        } => {
            h.update(b"cluster\0");
            h.update(node_id_set_hash.as_bytes())
        }
    };
}

impl TrustIdentity {
    /// Stable hash over the whole identity. This is what an [`crate::ApprovalToken`] binds to.
    #[must_use]
    pub fn hash(&self) -> String {
        let mut h = Hasher::new();
        h.update(b"penguin.trust-identity.v1\0");
        hash_endpoint(&mut h, &self.endpoint);
        hash_tls(&mut h, &self.tls_identity);
        hash_server(&mut h, &self.server_identity);
        h.update(
            self.auth_identity
                .username
                .as_deref()
                .unwrap_or("")
                .as_bytes(),
        );
        h.update(b"\0");
        h.update(self.auth_identity.secret_ref.as_bytes());
        h.finalize().to_hex().to_string()
    }

    /// Hash of only the parts a stored credential is bound to (ADR-010): TLS identity plus
    /// logical service. Address is excluded on purpose.
    #[must_use]
    pub fn credential_binding_hash(&self) -> String {
        let mut h = Hasher::new();
        h.update(b"penguin.credential-binding.v1\0");
        hash_tls(&mut h, &self.tls_identity);
        hash_server(&mut h, &self.server_identity);
        h.finalize().to_hex().to_string()
    }

    /// Compare against a previously trusted identity.
    #[must_use]
    pub fn compare(&self, previous: &Self) -> RebindVerdict {
        if self.auth_identity != previous.auth_identity {
            return RebindVerdict::AuthIdentityChanged;
        }
        if self.tls_identity != previous.tls_identity {
            return RebindVerdict::TlsIdentityChanged;
        }
        if core::mem::discriminant(&self.server_identity)
            != core::mem::discriminant(&previous.server_identity)
        {
            return RebindVerdict::ServerIdentityChanged;
        }
        // Same kind: compare only the fields that define the *logical service*. run_id and
        // the sentinel set are deliberately excluded (see hash_server).
        let changed = match (&self.server_identity, &previous.server_identity) {
            (
                ServerIdentity::Sentinel { master_name: a, .. },
                ServerIdentity::Sentinel { master_name: b, .. },
            )
            | (
                ServerIdentity::Cluster {
                    node_id_set_hash: a,
                    ..
                },
                ServerIdentity::Cluster {
                    node_id_set_hash: b,
                    ..
                },
            ) => a != b,
            _ => false,
        };
        if changed {
            return RebindVerdict::ServerIdentityChanged;
        }
        if self.endpoint != previous.endpoint {
            return RebindVerdict::AddressOnly;
        }
        // §21.3: the Sentinel set changing warns rather than blocks. Reported *after* the
        // address check so that a failover, which is the common case, keeps its own clearer
        // verdict.
        if let (
            ServerIdentity::Sentinel {
                sentinel_set_hash: a,
                ..
            },
            ServerIdentity::Sentinel {
                sentinel_set_hash: b,
                ..
            },
        ) = (&self.server_identity, &previous.server_identity)
            && a != b
        {
            return RebindVerdict::AdvisoryChange(Advisory::SentinelSetChanged);
        }
        RebindVerdict::NoChange
    }
}

/// A certificate or CA rotation, mid-flight (v2.1 §21.3).
///
/// §21.3 configures rotation as "accept both for N days". That window is not a convenience: a
/// fleet does not swap certificates atomically, so during a rollout some nodes present the old
/// identity and some the new. Without a window the client would demand re-binding on every
/// second connection, which trains the user to confirm re-binds without reading them — which
/// is the failure the binding exists to prevent.
///
/// The window is bounded and one-directional: the *previous* identity expires, the current one
/// does not. A rotation that never finishes is a rotation nobody completed.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RotationWindow {
    /// The identity being rotated to.
    pub current: TlsIdentity,
    /// The identity being rotated from, if a rotation is in progress.
    pub previous: Option<TlsIdentity>,
    /// When the previous identity stops being accepted, ms since the Unix epoch.
    pub previous_until_ms: u64,
}

/// What a presented TLS identity means during a rotation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RotationVerdict {
    /// Matches the identity being rotated to.
    Current,
    /// Matches the one being rotated from, and the window is still open.
    ///
    /// Accepted, and worth telling the user about: a node still on the old certificate is a
    /// node the rollout has not reached.
    PreviousWithinWindow,
    /// Matches the previous identity, but the window has closed. Re-bind.
    PreviousExpired,
    /// Matches neither.
    Unknown,
}

impl RotationVerdict {
    /// Whether the connection may proceed on the existing credential binding.
    #[must_use]
    pub fn accepts(self) -> bool {
        matches!(self, Self::Current | Self::PreviousWithinWindow)
    }
}

impl RotationWindow {
    /// A profile with no rotation in progress.
    #[must_use]
    pub fn settled(current: TlsIdentity) -> Self {
        Self {
            current,
            previous: None,
            previous_until_ms: 0,
        }
    }

    /// Judge the identity a server just presented.
    #[must_use]
    pub fn judge(&self, presented: &TlsIdentity, now_ms: u64) -> RotationVerdict {
        if *presented == self.current {
            return RotationVerdict::Current;
        }
        match &self.previous {
            Some(p) if p == presented && now_ms < self.previous_until_ms => {
                RotationVerdict::PreviousWithinWindow
            }
            Some(p) if p == presented => RotationVerdict::PreviousExpired,
            _ => RotationVerdict::Unknown,
        }
    }
}

/// What to do about a redirect to a node the profile has not seen (v2.1 §21.3).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RedirectDecision {
    /// Already a known node. Follow it.
    Known,
    /// New, but inside the profile's declared bounds. Follow it and record it.
    WithinBounds,
    /// Outside the declared bounds. **Stop and ask.**
    ///
    /// Following a `MOVED` to an arbitrary host and authenticating there is how a compromised
    /// or misconfigured cluster collects a password: the server chooses the address, and the
    /// client would be trusting it because it asked nicely.
    ConfirmationRequired,
}

/// The nodes a profile has accepted.
#[derive(Clone, Debug, Default)]
pub struct KnownNodes {
    nodes: Vec<(String, u16)>,
}

impl KnownNodes {
    /// Start from the nodes already recorded in the profile.
    #[must_use]
    pub fn new(nodes: Vec<(String, u16)>) -> Self {
        Self { nodes }
    }

    /// Whether this address has been accepted before.
    #[must_use]
    pub fn contains(&self, host: &str, port: u16) -> bool {
        self.nodes.iter().any(|(h, p)| h == host && *p == port)
    }

    /// Decide what a redirect to this address means.
    #[must_use]
    pub fn decide(&self, bounds: &DiscoveryBounds, host: &str, port: u16) -> RedirectDecision {
        if self.contains(host, port) {
            RedirectDecision::Known
        } else if bounds.permits(host, port) {
            RedirectDecision::WithinBounds
        } else {
            RedirectDecision::ConfirmationRequired
        }
    }

    /// Record a node that a decision allows without asking.
    ///
    /// Takes the decision rather than a bare address, so recording an out-of-bounds node is
    /// impossible without going through [`KnownNodes::accept_confirmed`] — the type makes the
    /// dangerous path the longer one. Returns whether the node may be used.
    pub fn accept(&mut self, decision: RedirectDecision, host: &str, port: u16) -> bool {
        match decision {
            RedirectDecision::Known => true,
            RedirectDecision::WithinBounds => {
                self.nodes.push((host.to_owned(), port));
                true
            }
            RedirectDecision::ConfirmationRequired => false,
        }
    }

    /// Record a node after the user confirmed it explicitly (§21.3).
    pub fn accept_confirmed(&mut self, host: &str, port: u16) {
        if !self.contains(host, port) {
            self.nodes.push((host.to_owned(), port));
        }
    }

    /// How many nodes are recorded.
    #[must_use]
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Whether none are recorded.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }
}

/// Bounds within which cluster discovery may add nodes without asking (v2.1 §21.3).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DiscoveryBounds {
    /// Exact `host:port` entries that are always allowed.
    pub allowed_hosts: BTreeSet<String>,
    /// Domain suffixes, e.g. `.redis-prod.internal`.
    pub allowed_suffixes: Vec<String>,
}

impl DiscoveryBounds {
    /// Whether a redirect target may be connected to without explicit confirmation.
    #[must_use]
    pub fn permits(&self, host: &str, port: u16) -> bool {
        if self.allowed_hosts.contains(&format!("{host}:{port}"))
            || self.allowed_hosts.contains(host)
        {
            return true;
        }
        self.allowed_suffixes
            .iter()
            .any(|s| host.ends_with(s.as_str()))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn base() -> TrustIdentity {
        TrustIdentity {
            endpoint: Endpoint::Tcp {
                host: "redis-dev.internal".into(),
                port: 6379,
            },
            tls_identity: TlsIdentity::Ca {
                ca_set_hash: "ca-abc".into(),
                server_name: "redis-dev.internal".into(),
            },
            server_identity: ServerIdentity::Standalone {
                run_id_prefix: Some("aaaa".into()),
            },
            auth_identity: AuthIdentity {
                username: Some("dev".into()),
                secret_ref: "credential:uuid-1".into(),
            },
        }
    }

    #[test]
    fn address_change_alone_does_not_rebind() {
        // NET-06 / failover: the service moved, the identity did not.
        let a = base();
        let mut b = base();
        b.endpoint = Endpoint::Tcp {
            host: "10.0.0.7".into(),
            port: 6380,
        };
        assert_eq!(b.compare(&a), RebindVerdict::AddressOnly);
        assert!(!b.compare(&a).requires_rebind());
        assert_eq!(a.credential_binding_hash(), b.credential_binding_hash());
    }

    #[test]
    fn restart_changing_run_id_does_not_rebind() {
        let a = base();
        let mut b = base();
        b.server_identity = ServerIdentity::Standalone {
            run_id_prefix: Some("zzzz".into()),
        };
        assert_eq!(b.compare(&a), RebindVerdict::NoChange);
        assert_eq!(a.credential_binding_hash(), b.credential_binding_hash());
    }

    #[test]
    fn tls_identity_change_forces_rebind() {
        // NET-02: credentials must not follow a replaced service.
        let a = base();
        let mut b = base();
        b.tls_identity = TlsIdentity::Ca {
            ca_set_hash: "ca-OTHER".into(),
            server_name: "redis-dev.internal".into(),
        };
        assert_eq!(b.compare(&a), RebindVerdict::TlsIdentityChanged);
        assert!(b.compare(&a).requires_rebind());
        assert_ne!(a.credential_binding_hash(), b.credential_binding_hash());
    }

    #[test]
    fn disabling_tls_forces_rebind() {
        let a = base();
        let mut b = base();
        b.tls_identity = TlsIdentity::None;
        assert!(b.compare(&a).requires_rebind());
    }

    #[test]
    fn sentinel_master_rename_is_a_different_service_but_new_sentinels_are_not() {
        let a = TrustIdentity {
            server_identity: ServerIdentity::Sentinel {
                master_name: "mymaster".into(),
                sentinel_set_hash: "s1".into(),
            },
            ..base()
        };
        let mut same_master = a.clone();
        same_master.server_identity = ServerIdentity::Sentinel {
            master_name: "mymaster".into(),
            sentinel_set_hash: "s2-added".into(),
        };
        assert_eq!(
            same_master.compare(&a),
            RebindVerdict::AdvisoryChange(Advisory::SentinelSetChanged),
            "adding a sentinel warns, not rebinds — and a warning has to be sayable"
        );
        assert!(!same_master.compare(&a).requires_rebind());

        let mut other_master = a.clone();
        other_master.server_identity = ServerIdentity::Sentinel {
            master_name: "othermaster".into(),
            sentinel_set_hash: "s1".into(),
        };
        assert_eq!(
            other_master.compare(&a),
            RebindVerdict::ServerIdentityChanged
        );
    }

    #[test]
    fn cluster_node_set_change_forces_rebind() {
        let a = TrustIdentity {
            server_identity: ServerIdentity::Cluster {
                node_id_set_hash: "n1".into(),
                seed_hosts: vec![],
            },
            ..base()
        };
        let mut b = a.clone();
        b.server_identity = ServerIdentity::Cluster {
            node_id_set_hash: "n2".into(),
            seed_hosts: vec![],
        };
        assert_eq!(b.compare(&a), RebindVerdict::ServerIdentityChanged);
    }

    #[test]
    fn changing_topology_kind_forces_rebind() {
        let a = base();
        let mut b = base();
        b.server_identity = ServerIdentity::Cluster {
            node_id_set_hash: "n1".into(),
            seed_hosts: vec![],
        };
        assert_eq!(b.compare(&a), RebindVerdict::ServerIdentityChanged);
    }

    #[test]
    fn different_user_is_a_different_identity() {
        let a = base();
        let mut b = base();
        b.auth_identity.username = Some("readonly".into());
        assert_eq!(b.compare(&a), RebindVerdict::AuthIdentityChanged);
        assert_ne!(a.hash(), b.hash());
    }

    #[test]
    fn hash_is_stable_and_distinguishes_every_field() {
        let a = base();
        assert_eq!(a.hash(), a.hash());
        let mut b = base();
        b.endpoint = Endpoint::Unix {
            path: "/tmp/r.sock".into(),
        };
        assert_ne!(
            a.hash(),
            b.hash(),
            "endpoint is part of the full identity hash"
        );
    }

    #[test]
    fn discovery_bounds_gate_redirect_targets() {
        // NET-03/05: a MOVED to an unlisted host must stop and ask.
        let b = DiscoveryBounds {
            allowed_hosts: ["10.0.0.1:6379".to_string()].into_iter().collect(),
            allowed_suffixes: vec![".redis-prod.internal".into()],
        };
        assert!(b.permits("10.0.0.1", 6379));
        assert!(b.permits("node7.redis-prod.internal", 6379));
        assert!(!b.permits("evil.example.com", 6379));
        assert!(!b.permits("10.0.0.2", 6379));
    }
}
