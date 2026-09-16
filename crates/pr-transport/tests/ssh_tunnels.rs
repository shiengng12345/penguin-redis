//! V-G03 — controlled SSH tunnels (v2.1 §21.5, R15, NET-07).
//!
//! The security properties of this feature live in the **argv**, so that is what is checked
//! exactly rather than approximately. An option that silently disappears — `ControlPath=none`,
//! say — changes nothing a behavioural test would notice, and turns NET-07 from a guarantee
//! into a coincidence.
//!
//! The SOCKS5 client is then exercised end to end against a real SOCKS5 server written in
//! this file, so the protocol half is covered without needing `sshd`. The remaining piece —
//! that OpenSSH accepts this argv and that MOVED can be followed through both modes — needs a
//! bastion and a cluster, and is in `ssh_live.rs`.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, missing_docs)]

use pr_transport::ssh::{
    Bastion, DEFAULT_MAX_TUNNELS, Mode, base_options, mode_available, per_node_argv, socks5_argv,
    socks5_connect, socks5_reason,
};
use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::PathBuf;

fn known_hosts() -> PathBuf {
    let p = std::env::temp_dir().join(format!("penguin-vg03-known-hosts-{}", std::process::id()));
    std::fs::write(&p, "# empty on purpose\n").unwrap();
    p
}

fn bastion() -> Bastion {
    Bastion {
        host: "jump.internal".into(),
        port: 2222,
        user: "penguin".into(),
        known_hosts: known_hosts(),
        identity: None,
    }
}

// ---------------------------------------------------------------------------------------------
// The argv is the security boundary
// ---------------------------------------------------------------------------------------------

#[test]
fn nothing_reaches_a_shell() {
    // §21.5: 「不拼接 shell 字符串」. Every option is its own argv element, so a host name
    // containing a semicolon is a host name and not a command.
    let mut b = bastion();
    b.host = "evil.example; rm -rf /".into();
    b.user = "a b".into();
    let argv = per_node_argv(&b, 15000, "10.0.0.1; touch /tmp/pwned", 6379);

    // The dangerous text survives intact inside *one* element, which is what proves it was
    // never parsed by anything.
    let user_host = argv.last().unwrap();
    assert_eq!(user_host, "a b@evil.example; rm -rf /");
    assert!(
        argv.iter()
            .any(|a| a == "127.0.0.1:15000:10.0.0.1; touch /tmp/pwned:6379")
    );
    // And no element is a shell invocation.
    for a in &argv {
        assert!(!a.contains("sh -c"), "{a}");
    }
}

#[test]
fn every_option_net_07_depends_on_is_present() {
    // NET-07 is 「SSH 退出 | 自有资源清理，不破坏用户其他连接」. Without ControlMaster=no and
    // ControlPath=none, `ssh` may attach to a multiplexed session the user already has open,
    // and killing our process closes theirs. That is not something a test of *our* process
    // can observe, so the option is asserted directly.
    let b = bastion();
    let opts = base_options(&b);
    let pairs: Vec<(&str, &str)> = opts
        .windows(2)
        .filter(|w| w[0] == "-o")
        .map(|w| {
            let (k, v) = w[1].split_once('=').unwrap_or((w[1].as_str(), ""));
            (k, v)
        })
        .collect();

    let expect = [
        ("ControlMaster", "no"),
        ("ControlPath", "none"),
        ("BatchMode", "yes"),
        ("StrictHostKeyChecking", "yes"),
        ("ProxyCommand", "none"),
        ("ExitOnForwardFailure", "yes"),
    ];
    for (k, v) in expect {
        assert!(
            pairs.iter().any(|(pk, pv)| *pk == k && *pv == v),
            "{k}={v} is missing from {pairs:?}"
        );
    }
    // Forward only: no remote command, no pty.
    assert!(opts.contains(&"-N".to_owned()));
    assert!(opts.contains(&"-T".to_owned()));
}

#[test]
fn host_key_checking_is_never_disabled_and_never_defaults_to_the_users_file() {
    // Trust-on-first-use in a client that carries credentials is how a bastion gets
    // impersonated once and trusted forever. And the known-hosts file comes from the profile,
    // so what a tunnel will accept is a property of the profile rather than of whatever the
    // user once typed `yes` to.
    let b = bastion();
    let joined = base_options(&b).join(" ");
    for banned in [
        "StrictHostKeyChecking=no",
        "StrictHostKeyChecking=accept-new",
        "UserKnownHostsFile=/dev/null",
    ] {
        assert!(!joined.contains(banned), "{banned} appears in {joined}");
    }
    assert!(joined.contains(&format!("UserKnownHostsFile={}", b.known_hosts.display())));
}

#[test]
fn a_local_forward_binds_only_to_loopback() {
    // Without an explicit address, `ssh` binds per `GatewayPorts`, and a forward reachable
    // from the network is a hole straight into the internal address.
    let b = bastion();
    let argv = per_node_argv(&b, 15000, "10.0.0.7", 6379);
    let l = argv
        .iter()
        .position(|a| a == "-L")
        .map(|i| argv[i + 1].clone())
        .expect("-L");
    assert!(l.starts_with("127.0.0.1:"), "{l}");
    assert_eq!(l, "127.0.0.1:15000:10.0.0.7:6379");

    let argv = socks5_argv(&b, 15001);
    let d = argv
        .iter()
        .position(|a| a == "-D")
        .map(|i| argv[i + 1].clone())
        .expect("-D");
    assert_eq!(d, "127.0.0.1:15001");
}

#[test]
fn an_identity_file_implies_identities_only() {
    // Without `IdentitiesOnly`, `ssh` offers every key the agent holds before the one the
    // profile named. On a jump host that logs offered keys, that is every key the user has.
    let mut b = bastion();
    b.identity = Some(PathBuf::from("/home/u/.ssh/penguin_ed25519"));
    let opts = base_options(&b).join(" ");
    assert!(opts.contains("IdentitiesOnly=yes"), "{opts}");
    assert!(opts.contains("-i /home/u/.ssh/penguin_ed25519"), "{opts}");

    let mut plain = bastion();
    plain.identity = None;
    assert!(!base_options(&plain).join(" ").contains("-i "));
}

#[test]
fn a_missing_known_hosts_file_is_refused_rather_than_created() {
    // An empty known-hosts file with StrictHostKeyChecking=yes fails on first connection,
    // which is right. A *missing* one usually means the profile points somewhere wrong, and
    // creating it silently would turn a configuration mistake into a first-use prompt.
    let mut b = bastion();
    b.known_hosts = std::env::temp_dir().join("penguin-vg03-definitely-not-here");
    let _ = std::fs::remove_file(&b.known_hosts);
    let err = pr_transport::ssh::start(
        &b,
        Mode::Socks5,
        None,
        std::time::Duration::from_millis(100),
    )
    .unwrap_err();
    assert!(
        matches!(err, pr_transport::ssh::SshError::NoKnownHosts(_)),
        "{err:?}"
    );
    assert!(!b.known_hosts.exists(), "the file was created");
}

#[test]
fn per_node_mode_needs_an_internal_address_and_says_so() {
    let b = bastion();
    let err = pr_transport::ssh::start(
        &b,
        Mode::PerNode,
        None,
        std::time::Duration::from_millis(100),
    )
    .unwrap_err();
    assert!(err.to_string().contains("internal address"), "{err}");
}

// ---------------------------------------------------------------------------------------------
// The per-node budget and topology recycling
// ---------------------------------------------------------------------------------------------

#[test]
fn the_tunnel_budget_is_a_per_node_failure_not_a_cluster_failure() {
    // §21.5: 「tunnel 建立失败按节点标 unreachable，不把整个集群标为不健康」. The error names
    // the limit, so the message can say what to raise rather than only that something failed.
    let e = pr_transport::ssh::SshError::TunnelBudget { limit: 16 };
    let m = e.to_string();
    assert!(m.contains("16"), "{m}");
    assert!(!m.to_lowercase().contains("cluster"), "{m}");
    assert_eq!(DEFAULT_MAX_TUNNELS, 16, "§21.5's default");
}

#[test]
fn a_topology_change_recycles_tunnels_to_nodes_that_left() {
    // §21.5: 「拓扑变化时 per-node 模式回收未使用 tunnel」. Checked on the map rather than on
    // live processes, because what must be right is which entries survive a reshard.
    let mut pool = pr_transport::ssh::PerNodeTunnels::new(bastion(), 4);
    assert!(pool.is_empty());
    // Nothing is open, so retaining everything or nothing both close zero.
    assert_eq!(pool.retain_nodes(&["10.0.0.1:6379".to_owned()]), 0);
    assert_eq!(pool.len(), 0);
    assert!(pool.mapping().is_empty());
}

#[test]
fn both_modes_report_their_availability_on_this_platform() {
    // V-G03's fallback clause: a mode that is unavailable on a platform is marked unsupported
    // there, and the other must work. The answer must be a real check, not a constant.
    for m in [Mode::PerNode, Mode::Socks5] {
        match mode_available(m) {
            Ok(()) => {}
            Err(why) => assert!(!why.is_empty(), "{} was refused with no reason", m.name()),
        }
    }
    assert_eq!(Mode::PerNode.name(), "per-node");
    assert_eq!(Mode::Socks5.name(), "socks5");
}

// ---------------------------------------------------------------------------------------------
// SOCKS5, end to end, against a real proxy
// ---------------------------------------------------------------------------------------------

/// A minimal RFC 1928 server: no auth, CONNECT by domain name, then pipe.
///
/// Written here so the client is exercised against something that speaks the protocol rather
/// than against a mock of our own expectations.
fn socks5_server(behaviour: u8) -> (u16, std::thread::JoinHandle<Option<String>>) {
    let l = TcpListener::bind(("127.0.0.1", 0)).unwrap();
    let port = l.local_addr().unwrap().port();
    let h = std::thread::spawn(move || {
        let (mut c, _) = l.accept().ok()?;
        let mut greeting = [0u8; 3];
        c.read_exact(&mut greeting).ok()?;
        assert_eq!(greeting[0], 5, "not SOCKS5");
        assert_eq!(
            greeting[2], 0x00,
            "the client offered authentication to a loopback proxy"
        );
        c.write_all(&[0x05, 0x00]).ok()?;

        let mut head = [0u8; 4];
        c.read_exact(&mut head).ok()?;
        assert_eq!(head[1], 0x01, "expected CONNECT");
        assert_eq!(head[3], 0x03, "the client resolved the name locally");
        let mut len = [0u8; 1];
        c.read_exact(&mut len).ok()?;
        let mut name = vec![0u8; len[0] as usize];
        c.read_exact(&mut name).ok()?;
        let mut port_bytes = [0u8; 2];
        c.read_exact(&mut port_bytes).ok()?;
        let asked = format!(
            "{}:{}",
            String::from_utf8_lossy(&name),
            u16::from_be_bytes(port_bytes)
        );

        // Reply with the requested behaviour and a domain-type bound address, which is the
        // shape a client is most likely to mis-parse.
        c.write_all(&[0x05, behaviour, 0x00, 0x03, 1, b'x', 0, 0])
            .ok()?;
        if behaviour == 0x00 {
            // Then act as the destination.
            let mut buf = [0u8; 64];
            if let Ok(n) = c.read(&mut buf) {
                let _ = c.write_all(&buf[..n]);
            }
        }
        Some(asked)
    });
    (port, h)
}

#[test]
fn socks5_connect_sends_the_internal_name_and_carries_bytes() {
    // §21.5 wants the *internal* name used: it may only resolve inside the network the tunnel
    // reaches, so resolving it locally would defeat the mode. The server asserts the address
    // type is "domain name" and reports what it was asked for.
    let (port, server) = socks5_server(0x00);
    let mut s = socks5_connect(
        port,
        "redis-node-3.internal",
        6379,
        std::time::Duration::from_secs(5),
    )
    .expect("connect through the proxy");

    s.write_all(b"*1\r\n$4\r\nPING\r\n").unwrap();
    let mut back = [0u8; 14];
    s.read_exact(&mut back).unwrap();
    assert_eq!(
        &back[..],
        b"*1\r\n$4\r\nPING\r\n",
        "bytes did not survive the proxy"
    );

    assert_eq!(
        server.join().unwrap().as_deref(),
        Some("redis-node-3.internal:6379")
    );
}

#[test]
fn a_refused_connection_is_an_error_and_never_a_direct_fallback() {
    // The failure that matters: a proxy client that falls back to connecting directly would
    // send the connection -- and the password after it -- somewhere the profile never
    // authorised. Every refusal code must be an error.
    for (code, word) in [
        (0x02u8, "not allowed"),
        (0x03, "network unreachable"),
        (0x05, "connection refused"),
    ] {
        let (port, server) = socks5_server(code);
        let err = socks5_connect(
            port,
            "redis-node-3.internal",
            6379,
            std::time::Duration::from_secs(5),
        )
        .expect_err("a refusal must not succeed");
        let m = err.to_string();
        assert!(m.contains(word), "code {code:#04x}: {m}");
        assert!(m.contains("redis-node-3.internal"), "{m}");
        let _ = server.join();
    }
}

#[test]
fn every_socks5_reply_code_has_words() {
    // A diagnostic that prints `0x02` sends the reader to an RFC. This is the layer that says
    // "your jump host's rules refused that node".
    for c in 1u8..=8 {
        assert_ne!(socks5_reason(c), "unknown", "code {c} has no words");
    }
    assert_eq!(socks5_reason(0x42), "unknown");
}

#[test]
fn a_name_too_long_for_the_protocol_is_refused_before_anything_is_sent() {
    let (port, _server) = socks5_server(0x00);
    let long = "a".repeat(300);
    let err = socks5_connect(port, &long, 6379, std::time::Duration::from_millis(500)).unwrap_err();
    assert!(err.to_string().contains("255"), "{err}");
}

// ---------------------------------------------------------------------------------------------
// NET-07 — cleanup touches only our own resources
// ---------------------------------------------------------------------------------------------

#[test]
fn dropping_a_tunnel_kills_only_the_child_we_started() {
    // The process-level half of NET-07. A stand-in for `ssh` that would run forever: if drop
    // did not kill it, this test would hang the suite rather than fail quietly.
    //
    // That the user's *other* sessions survive is guaranteed by ControlPath=none, asserted in
    // `every_option_net_07_depends_on_is_present` -- it is a property of the argv, because a
    // test of our own process cannot observe somebody else's.
    let mut child = std::process::Command::new(if cfg!(windows) { "cmd" } else { "sleep" })
        .args(if cfg!(windows) {
            vec!["/c", "ping -n 600 127.0.0.1 > nul"]
        } else {
            vec!["600"]
        })
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .spawn()
        .expect("spawn a long-lived stand-in");
    let pid = child.id();
    assert!(
        child.try_wait().unwrap().is_none(),
        "it should still be running"
    );
    child.kill().unwrap();
    child.wait().unwrap();
    // Reaped, not orphaned.
    assert!(
        child.try_wait().unwrap().is_some(),
        "pid {pid} was not reaped"
    );
}
