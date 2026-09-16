//! Controlled OpenSSH tunnels (v2.1 §21.5, R15, NET-07, V-G03).
//!
//! §21.5 sets the shape, and each clause rules out the easy implementation:
//!
//! > 实现首选受控、可审计的系统 OpenSSH 适配，**不拼接 shell 字符串**。
//! > local forward 使用 loopback 临时端口……**退出时仅清理自己创建的资源，不破坏用户已有共享
//! > SSH 会话**。
//!
//! ## What "controlled" rules out
//!
//! - **No shell.** Every option is an argv element. A host or a port that reaches a shell is
//!   a host or a port that can contain `;`.
//! - **No joining the user's session.** `ControlMaster=no` and `ControlPath=none` are not
//!   tidiness: without them `ssh` may attach to a multiplexed session the user already has
//!   open, and then NET-07 — 「SSH 退出 | 自有资源清理，**不破坏用户其他连接**」 — becomes
//!   impossible, because our exit would take their session with it.
//! - **No prompting and no trust-on-first-use.** `BatchMode=yes` and
//!   `StrictHostKeyChecking=yes` against a known-hosts file the profile names. A tunnel that
//!   asks "are you sure?" in a pipeline hangs; one that answers "yes" for the user is not a
//!   tunnel, it is a hole.
//!
//! ## Two modes, because one loopback forward cannot reach a cluster
//!
//! §21.5's table. A single `-L` reaches the seed and nothing else: `MOVED` and Sentinel both
//! answer with *internal* addresses that the loopback port does not go to.
//!
//! | mode | mechanism |
//! |---|---|
//! | [`Mode::PerNode`] | one `-L` per node, lazily, bounded by `max_tunnels` (default 16) |
//! | [`Mode::Socks5`] | one `-D`, every node reached through SOCKS5 |
//!
//! In both, **TLS still verifies the internal name**. The loopback port is where the bytes go;
//! the name in the certificate comes from the `TrustIdentity`, which is why [`TlsConfig`]
//! keeps them as separate fields (V-G04).
//!
//! [`TlsConfig`]: crate::tls::TlsConfig

use std::net::{SocketAddr, TcpListener};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

/// Which forwarding mode a profile selected.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// One `-L` per node. For a small, stable topology, or an `sshd` that forbids `-D`.
    PerNode,
    /// One `-D`. §21.5's recommended default for a cluster.
    Socks5,
}

impl Mode {
    /// The name used in a profile and in diagnostics.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::PerNode => "per-node",
            Self::Socks5 => "socks5",
        }
    }
}

/// How to reach the jump host.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Bastion {
    /// Hostname of the jump host.
    pub host: String,
    /// Port `sshd` listens on.
    pub port: u16,
    /// Login name.
    pub user: String,
    /// The known-hosts file this profile trusts.
    ///
    /// Named by the profile rather than defaulting to `~/.ssh/known_hosts`, so that what a
    /// tunnel will accept is a property of the profile and not of whatever the user once
    /// typed `yes` to.
    pub known_hosts: PathBuf,
    /// Identity file, if the profile names one.
    pub identity: Option<PathBuf>,
}

/// Why a tunnel could not be established.
#[derive(Debug, thiserror::Error)]
pub enum SshError {
    /// `ssh` is not installed, or could not be started.
    #[error("cannot run ssh: {0}")]
    NotRunnable(String),
    /// The known-hosts file the profile names does not exist.
    ///
    /// Refused rather than created: an empty known-hosts file with
    /// `StrictHostKeyChecking=yes` fails on the first connection, which is correct, but a
    /// *missing* one usually means the profile is pointing at the wrong path, and silently
    /// making one would turn a configuration mistake into a first-use prompt.
    #[error("{0} does not exist; a tunnel will not create a known-hosts file for you")]
    NoKnownHosts(PathBuf),
    /// The tunnel process exited before the forward was usable.
    #[error("ssh exited before the tunnel came up: {0}")]
    Exited(String),
    /// The forward did not accept a connection within the timeout.
    #[error("the tunnel did not come up within {0:?}")]
    Timeout(std::time::Duration),
    /// The per-node budget is spent.
    #[error("this profile allows {limit} tunnels and all of them are in use")]
    TunnelBudget {
        /// §21.5's `max_tunnels`.
        limit: usize,
    },
    /// The socket failed.
    #[error("{0}")]
    Io(#[from] std::io::Error),
}

/// Options every invocation carries, whatever the mode.
///
/// Built as a list rather than a string. Exposed so a test can assert the whole set — the
/// security properties of this module *are* these options, and an option that silently
/// disappears is not something a behavioural test would notice.
#[must_use]
pub fn base_options(b: &Bastion) -> Vec<String> {
    let mut o = vec![
        // Forward only. No remote command, no shell, no pty.
        "-N".to_owned(),
        "-T".to_owned(),
        // Never prompt. A tunnel that asks a question in a pipeline hangs forever.
        "-o".to_owned(),
        "BatchMode=yes".to_owned(),
        // Never trust on first use.
        "-o".to_owned(),
        "StrictHostKeyChecking=yes".to_owned(),
        "-o".to_owned(),
        format!("UserKnownHostsFile={}", b.known_hosts.display()),
        // NET-07. Without these, `ssh` may attach to a control socket the user already has
        // open, and our exit would close their session too.
        "-o".to_owned(),
        "ControlMaster=no".to_owned(),
        "-o".to_owned(),
        "ControlPath=none".to_owned(),
        // Do not read the user's config: a `ProxyCommand` in it is an executable the profile
        // never approved (§21.5 requires an extra trust step for exactly that).
        "-o".to_owned(),
        "ProxyCommand=none".to_owned(),
        // Notice a dead jump host rather than holding a forward that goes nowhere.
        "-o".to_owned(),
        "ServerAliveInterval=15".to_owned(),
        "-o".to_owned(),
        "ServerAliveCountMax=3".to_owned(),
        "-o".to_owned(),
        "ExitOnForwardFailure=yes".to_owned(),
        "-p".to_owned(),
        b.port.to_string(),
    ];
    if let Some(id) = &b.identity {
        o.push("-o".to_owned());
        o.push("IdentitiesOnly=yes".to_owned());
        o.push("-i".to_owned());
        o.push(id.display().to_string());
    }
    o
}

/// The full argv for a `-L` forward from a loopback port to an internal address.
#[must_use]
pub fn per_node_argv(
    b: &Bastion,
    local_port: u16,
    internal: &str,
    internal_port: u16,
) -> Vec<String> {
    let mut a = base_options(b);
    a.push("-L".to_owned());
    // Bound to 127.0.0.1 explicitly. Without the address, `ssh` binds per `GatewayPorts`, and
    // a forward reachable from the network is a hole into the internal address.
    a.push(format!("127.0.0.1:{local_port}:{internal}:{internal_port}"));
    a.push(format!("{}@{}", b.user, b.host));
    a
}

/// The full argv for a `-D` dynamic forward.
#[must_use]
pub fn socks5_argv(b: &Bastion, local_port: u16) -> Vec<String> {
    let mut a = base_options(b);
    a.push("-D".to_owned());
    a.push(format!("127.0.0.1:{local_port}"));
    a.push(format!("{}@{}", b.user, b.host));
    a
}

/// A running `ssh` process and the loopback port it listens on.
///
/// Killed on drop, and only this process is killed: `ControlPath=none` means it never joined
/// anything of the user's, so there is nothing else it could take with it (NET-07).
#[derive(Debug)]
pub struct Tunnel {
    child: Child,
    local_port: u16,
    mode: Mode,
    target: String,
}

impl Tunnel {
    /// The loopback port to connect to.
    #[must_use]
    pub fn local_port(self: &Tunnel) -> u16 {
        self.local_port
    }

    /// Which mode created it.
    #[must_use]
    pub fn mode(&self) -> Mode {
        self.mode
    }

    /// What it reaches: an internal address for `per-node`, the bastion for `socks5`.
    #[must_use]
    pub fn target(&self) -> &str {
        &self.target
    }

    /// Whether the `ssh` process is still alive.
    ///
    /// # Errors
    /// Propagates the wait failure.
    pub fn is_running(&mut self) -> std::io::Result<bool> {
        Ok(self.child.try_wait()?.is_none())
    }
}

impl Drop for Tunnel {
    fn drop(&mut self) {
        // Only our child. LIFE-01: never rely on "the process is exiting anyway".
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Pick a free loopback port by binding and releasing it.
///
/// # Errors
/// Propagates the bind failure.
pub fn free_loopback_port() -> std::io::Result<u16> {
    let l = TcpListener::bind(("127.0.0.1", 0))?;
    let p = l.local_addr()?.port();
    drop(l);
    Ok(p)
}

/// Start a tunnel and wait until its loopback port accepts a connection.
///
/// # Errors
/// [`SshError`].
pub fn start(
    b: &Bastion,
    mode: Mode,
    internal: Option<(&str, u16)>,
    timeout: std::time::Duration,
) -> Result<Tunnel, SshError> {
    if !b.known_hosts.exists() {
        return Err(SshError::NoKnownHosts(b.known_hosts.clone()));
    }
    let local_port = free_loopback_port()?;
    let (argv, target) = match (mode, internal) {
        (Mode::PerNode, Some((host, port))) => (
            per_node_argv(b, local_port, host, port),
            format!("{host}:{port}"),
        ),
        (Mode::PerNode, None) => {
            return Err(SshError::NotRunnable(
                "per-node mode needs the internal address to forward to".to_owned(),
            ));
        }
        (Mode::Socks5, _) => (socks5_argv(b, local_port), format!("{}:{}", b.host, b.port)),
    };

    let mut child = Command::new("ssh")
        .args(&argv)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| SshError::NotRunnable(e.to_string()))?;

    let deadline = std::time::Instant::now() + timeout;
    loop {
        if let Some(status) = child.try_wait().map_err(SshError::Io)? {
            return Err(SshError::Exited(format!("{status}")));
        }
        if std::net::TcpStream::connect_timeout(
            &SocketAddr::from(([127, 0, 0, 1], local_port)),
            std::time::Duration::from_millis(200),
        )
        .is_ok()
        {
            return Ok(Tunnel {
                child,
                local_port,
                mode,
                target,
            });
        }
        if std::time::Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(SshError::Timeout(timeout));
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
}

/// Per-node tunnels, lazily created and bounded (§21.5's `max_tunnels`).
///
/// The map is `internal_addr -> loopback_port`, which is the thing a cluster router needs:
/// a `MOVED` names an internal address, and the router has to turn that into somewhere it can
/// actually connect.
#[derive(Debug)]
pub struct PerNodeTunnels {
    bastion: Bastion,
    limit: usize,
    tunnels: Vec<(String, Tunnel)>,
}

/// §21.5's default.
pub const DEFAULT_MAX_TUNNELS: usize = 16;

impl PerNodeTunnels {
    /// A pool with §21.5's budget.
    #[must_use]
    pub fn new(bastion: Bastion, limit: usize) -> Self {
        Self {
            bastion,
            limit,
            tunnels: Vec::new(),
        }
    }

    /// How many tunnels are open.
    #[must_use]
    pub fn len(&self) -> usize {
        self.tunnels.len()
    }

    /// Whether none are open.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.tunnels.is_empty()
    }

    /// The loopback port for an internal address, opening a tunnel if there is not one.
    ///
    /// # Errors
    /// [`SshError::TunnelBudget`] when the profile's limit is reached — which is a per-node
    /// failure, not a cluster failure: §21.5 says an unreachable node is marked unreachable
    /// and the cluster is not marked unhealthy.
    pub fn port_for(
        &mut self,
        internal_host: &str,
        internal_port: u16,
        timeout: std::time::Duration,
    ) -> Result<u16, SshError> {
        let key = format!("{internal_host}:{internal_port}");
        if let Some((_, t)) = self.tunnels.iter().find(|(k, _)| *k == key) {
            return Ok(t.local_port());
        }
        if self.tunnels.len() >= self.limit {
            return Err(SshError::TunnelBudget { limit: self.limit });
        }
        let t = start(
            &self.bastion,
            Mode::PerNode,
            Some((internal_host, internal_port)),
            timeout,
        )?;
        let port = t.local_port();
        self.tunnels.push((key, t));
        Ok(port)
    }

    /// Close tunnels to nodes that are no longer in the topology.
    ///
    /// §21.5: 「拓扑变化时 per-node 模式回收未使用 tunnel」. Returns how many were closed.
    pub fn retain_nodes(&mut self, live: &[String]) -> usize {
        let before = self.tunnels.len();
        self.tunnels.retain(|(k, _)| live.iter().any(|l| l == k));
        before - self.tunnels.len()
    }

    /// The `internal_addr -> loopback_port` map, for diagnostics.
    #[must_use]
    pub fn mapping(&self) -> Vec<(String, u16)> {
        self.tunnels
            .iter()
            .map(|(k, t)| (k.clone(), t.local_port()))
            .collect()
    }
}

/// Connect to `host:port` through a SOCKS5 proxy on `proxy_port` (RFC 1928).
///
/// Written here rather than taken from a crate because it is forty lines and it is on the
/// path a credential travels: a proxy client that silently falls back to a direct connection
/// when the proxy refuses would send the connection — and the password after it — somewhere
/// the profile never authorised. This one returns an error.
///
/// No authentication is offered, because the proxy is an `ssh -D` we started ourselves on
/// loopback; offering credentials to it would be offering them to anything else that managed
/// to bind that port first.
///
/// # Errors
/// [`SshError::Io`] for the socket, or a protocol failure reported as `InvalidData`.
pub fn socks5_connect(
    proxy_port: u16,
    host: &str,
    port: u16,
    timeout: std::time::Duration,
) -> Result<std::net::TcpStream, SshError> {
    use std::io::{Read, Write};

    let mut s = std::net::TcpStream::connect_timeout(
        &SocketAddr::from(([127, 0, 0, 1], proxy_port)),
        timeout,
    )?;
    s.set_read_timeout(Some(timeout))?;
    s.set_write_timeout(Some(timeout))?;

    // Greeting: version 5, one method, "no authentication".
    s.write_all(&[0x05, 0x01, 0x00])?;
    let mut reply = [0u8; 2];
    s.read_exact(&mut reply)?;
    if reply != [0x05, 0x00] {
        return Err(SshError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("the SOCKS5 proxy refused the no-auth method: {reply:?}"),
        )));
    }

    // CONNECT to a *domain name*, not to an address we resolved. Resolving locally would
    // defeat the point: the internal name may only resolve inside the network the tunnel
    // reaches, and §21.5 wants the internal name used.
    if host.len() > 255 {
        return Err(SshError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "a SOCKS5 domain name may be at most 255 bytes",
        )));
    }
    // The length check above bounds this to 255, which is what makes the cast exact.
    #[allow(clippy::cast_possible_truncation)]
    let name_len = host.len() as u8;
    let mut req = vec![0x05, 0x01, 0x00, 0x03, name_len];
    req.extend_from_slice(host.as_bytes());
    req.extend_from_slice(&port.to_be_bytes());
    s.write_all(&req)?;

    let mut head = [0u8; 4];
    s.read_exact(&mut head)?;
    if head[0] != 0x05 {
        return Err(SshError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "not a SOCKS5 reply",
        )));
    }
    if head[1] != 0x00 {
        return Err(SshError::Io(std::io::Error::new(
            std::io::ErrorKind::ConnectionRefused,
            format!(
                "the proxy refused {host}:{port}: {}",
                socks5_reason(head[1])
            ),
        )));
    }
    // Consume the bound address, whose length depends on its type.
    match head[3] {
        0x01 => {
            let mut b = [0u8; 4 + 2];
            s.read_exact(&mut b)?;
        }
        0x04 => {
            let mut b = [0u8; 16 + 2];
            s.read_exact(&mut b)?;
        }
        0x03 => {
            let mut len = [0u8; 1];
            s.read_exact(&mut len)?;
            let mut b = vec![0u8; len[0] as usize + 2];
            s.read_exact(&mut b)?;
        }
        other => {
            return Err(SshError::Io(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!("unknown SOCKS5 address type {other}"),
            )));
        }
    }
    Ok(s)
}

/// RFC 1928's reply codes, in words.
#[must_use]
pub fn socks5_reason(code: u8) -> &'static str {
    match code {
        0x01 => "general failure",
        0x02 => "not allowed by ruleset",
        0x03 => "network unreachable",
        0x04 => "host unreachable",
        0x05 => "connection refused",
        0x06 => "TTL expired",
        0x07 => "command not supported",
        0x08 => "address type not supported",
        _ => "unknown",
    }
}

/// Whether a mode is usable on this platform.
///
/// V-G03's fallback clause: 「某模式在某平台不可用 → 该平台该模式标 unsupported，另一模式必须
/// 可用」. Both modes need the same `ssh`, so the answer is the same for both today; the shape
/// is here so a platform that loses one can say so rather than failing at connect time.
///
/// # Errors
/// A sentence naming why the mode is unavailable, suitable for showing a user.
pub fn mode_available(mode: Mode) -> Result<(), String> {
    let _ = mode;
    match Command::new("ssh")
        .arg("-V")
        .stderr(Stdio::piped())
        .output()
    {
        Ok(o) if o.status.success() => Ok(()),
        Ok(o) => Err(format!(
            "ssh -V failed: {}",
            String::from_utf8_lossy(&o.stderr).trim()
        )),
        Err(e) => Err(format!("ssh is not on PATH: {e}")),
    }
}

/// Where a profile's known-hosts file lives by default.
#[must_use]
pub fn default_known_hosts(config_dir: &Path) -> PathBuf {
    config_dir.join("known_hosts")
}
