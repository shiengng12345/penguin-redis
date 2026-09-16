//! V-G03 — both §21.5 modes against a real jump host and a real cluster (NET-07).
//!
//! `ci/topology/ssh/up.sh` puts three Redis nodes on a bridge network and one `sshd` in front
//! of them. Only the bastion is published, so the cluster announces addresses the host cannot
//! reach — which is the whole situation §21.5 is about. With host networking there would be
//! nothing for a tunnel to do, and the test would prove nothing.
//!
//! `#[ignore]` because it needs Docker and a built image; the `ssh-tunnels` CI job runs it.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    missing_docs,
    // `f`, `b`, `h`, `p`, `c` in a test body, where the surrounding five lines say what they
    // are. Longer names here would be noise, not clarity.
    clippy::many_single_char_names
)]

use bytes::Bytes;
use pr_protocol::value::Value;
use pr_transport::Oneshot;
use pr_transport::ssh::{Bastion, Mode, PerNodeTunnels, socks5_connect, start};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

const TIMEOUT: Duration = Duration::from_secs(10);

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("repo root")
        .to_path_buf()
}

struct Fixture {
    ssh_port: u16,
    identity: PathBuf,
    known_hosts: PathBuf,
    nodes: Vec<String>,
}

/// Read what `up.sh` recorded, or explain how to create it.
fn fixture() -> Fixture {
    let env = root().join("ci/topology/ssh/.work/fixture.env");
    let text = std::fs::read_to_string(&env).unwrap_or_else(|e| {
        panic!(
            "{}: {e}\nRun `bash ci/topology/ssh/up.sh` first.",
            env.display()
        )
    });
    let kv: BTreeMap<&str, &str> = text
        .lines()
        .filter_map(|l| l.split_once('='))
        .map(|(k, v)| (k.trim(), v.trim()))
        .collect();
    let mut nodes: Vec<String> = kv
        .iter()
        .filter(|(k, _)| k.starts_with("node"))
        .map(|(_, v)| (*v).to_owned())
        .collect();
    nodes.sort();
    Fixture {
        ssh_port: kv["ssh_port"].parse().unwrap(),
        identity: PathBuf::from(kv["identity"]),
        known_hosts: PathBuf::from(kv["known_hosts"]),
        nodes,
    }
}

fn bastion(f: &Fixture) -> Bastion {
    Bastion {
        host: "127.0.0.1".into(),
        port: f.ssh_port,
        user: "penguin".into(),
        known_hosts: f.known_hosts.clone(),
        identity: Some(f.identity.clone()),
    }
}

fn split(addr: &str) -> (String, u16) {
    let (h, p) = addr.rsplit_once(':').expect("host:port");
    (h.to_owned(), p.parse().expect("port"))
}

/// Ask a node where a key lives. Returns `Some(internal_addr)` when it answers MOVED.
fn moved_target(
    conn: &mut Oneshot<impl std::io::Read + std::io::Write>,
    key: &str,
) -> Option<String> {
    let reply = conn
        .call(&[
            Bytes::from_static(b"GET"),
            Bytes::from(key.as_bytes().to_vec()),
        ])
        .ok()?;
    let Value::Error(e) = reply else {
        return None;
    };
    let text = String::from_utf8_lossy(&e).into_owned();
    // `MOVED <slot> <host>:<port>`
    text.starts_with("MOVED")
        .then(|| text.split_whitespace().nth(2).map(str::to_owned))
        .flatten()
}

/// Keys chosen so that, across three primaries, at least one is not on the seed.
const PROBE_KEYS: &[&str] = &["a", "b", "c", "d", "e", "f", "g", "h", "i", "j", "k", "l"];

// ---------------------------------------------------------------------------------------------
// The premise: without a tunnel there is nothing to reach
// ---------------------------------------------------------------------------------------------

fn docker(args: &[&str]) -> Option<String> {
    let out = std::process::Command::new("docker")
        .args(args)
        .stdin(std::process::Stdio::null())
        .output()
        .ok()?;
    out.status
        .success()
        .then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

#[test]
#[ignore = "needs the V-G03 Docker fixture; run by the ssh-tunnels CI job"]
fn the_tunnel_is_load_bearing_and_not_a_coincidence() {
    // The premise every other test in this file rests on: the bytes really go through the
    // jump host.
    //
    // The first version of this test tried to prove it by absence -- assert the cluster is
    // unreachable directly -- and that turned out to depend on the Docker runtime. Some hosts
    // route bridge addresses and some do not, and an `--internal` network did not change it
    // here. A premise that holds on one machine and not another is not a premise.
    //
    // So it is proved positively instead: stop the bastion, and the connection through the
    // tunnel must stop working. Nothing but the tunnel can explain that.
    let f = fixture();
    let (host, port) = split(&f.nodes[0]);

    // Report what this host does, because it is worth knowing which situation the other tests
    // ran in -- but do not assert it.
    let direct = Oneshot::connect(&host, port, Duration::from_millis(800)).is_ok();
    eprintln!(
        "V-G03: this host {} reach {host}:{port} directly",
        if direct { "CAN" } else { "cannot" }
    );

    let tunnel = start(&bastion(&f), Mode::Socks5, None, TIMEOUT).expect("socks5 tunnel");
    let proxy = tunnel.local_port();
    {
        let sock = socks5_connect(proxy, &host, port, TIMEOUT).expect("through the tunnel");
        let mut c = Oneshot::over(sock);
        assert!(c.call(&[Bytes::from_static(b"PING")]).is_ok());
    }

    docker(&["stop", "-t", "1", "pr-ssh-bastion"]).expect("stop the bastion");
    let after = socks5_connect(proxy, &host, port, Duration::from_secs(3));
    let restored = docker(&["start", "pr-ssh-bastion"]).is_some();

    assert!(
        after.is_err(),
        "the connection survived the bastion being stopped, so it was not going through it"
    );
    assert!(
        restored,
        "the bastion could not be restarted for the other tests"
    );
    // Give sshd a moment to come back, so a later test in the same run is not racing it.
    std::thread::sleep(Duration::from_millis(1500));
}

// ---------------------------------------------------------------------------------------------
// per-node
// ---------------------------------------------------------------------------------------------

#[test]
#[ignore = "needs the V-G03 Docker fixture; run by the ssh-tunnels CI job"]
fn per_node_mode_follows_moved_by_opening_a_tunnel_to_the_node_that_owns_the_slot() {
    let f = fixture();
    let b = bastion(&f);
    let mut pool = PerNodeTunnels::new(b.clone(), 16);

    // The seed, through its own tunnel.
    let (seed_host, seed_port) = split(&f.nodes[0]);
    let seed_local = pool
        .port_for(&seed_host, seed_port, TIMEOUT)
        .expect("seed tunnel");
    let mut seed = Oneshot::connect("127.0.0.1", seed_local, TIMEOUT).expect("connect to seed");

    // Find a key the seed does not own. A single loopback forward stops here: MOVED names an
    // internal address, and the seed's port does not go there.
    let mut redirect = None;
    for k in PROBE_KEYS {
        if let Some(target) = moved_target(&mut seed, k) {
            redirect = Some((*k, target));
            break;
        }
    }
    let (key, target) = redirect.expect("no key in the probe set was MOVED; the fixture changed");
    assert!(
        !target.starts_with("127.0.0.1"),
        "MOVED named a loopback address, so the fixture is not modelling an internal topology"
    );

    // Follow it: a second tunnel, to the address the cluster actually named.
    let (host, port) = split(&target);
    let local = pool
        .port_for(&host, port, TIMEOUT)
        .expect("tunnel to the owner");
    let mut owner = Oneshot::connect("127.0.0.1", local, TIMEOUT).expect("connect to the owner");
    let reply = owner
        .call(&[
            Bytes::from_static(b"GET"),
            Bytes::from(key.as_bytes().to_vec()),
        ])
        .expect("GET on the owner");
    assert!(
        !matches!(&reply, Value::Error(e) if e.starts_with(b"MOVED")),
        "the owner still redirected: {reply:?}"
    );

    assert_eq!(pool.len(), 2, "one tunnel per node, lazily");
    let mapping = pool.mapping();
    assert!(mapping.iter().any(|(k, _)| *k == target));
    // Every loopback port is distinct: two nodes must not share a forward.
    let ports: Vec<u16> = mapping.iter().map(|(_, p)| *p).collect();
    let mut uniq = ports.clone();
    uniq.sort_unstable();
    uniq.dedup();
    assert_eq!(ports.len(), uniq.len());

    // §21.5: a reshard recycles tunnels to nodes that left.
    let closed = pool.retain_nodes(&[format!("{seed_host}:{seed_port}")]);
    assert_eq!(closed, 1);
    assert_eq!(pool.len(), 1);
}

#[test]
#[ignore = "needs the V-G03 Docker fixture; run by the ssh-tunnels CI job"]
fn the_tunnel_budget_stops_at_the_profile_limit() {
    // A per-node failure, not a cluster failure. The pool refuses the third tunnel and the
    // first two keep working.
    let f = fixture();
    let mut pool = PerNodeTunnels::new(bastion(&f), 2);
    for n in &f.nodes[..2] {
        let (h, p) = split(n);
        pool.port_for(&h, p, TIMEOUT).expect("within budget");
    }
    let (h, p) = split(&f.nodes[2]);
    let err = pool.port_for(&h, p, TIMEOUT).unwrap_err();
    assert!(err.to_string().contains('2'), "{err}");
    assert_eq!(pool.len(), 2);

    // And an already-open node is still served from the map rather than counted again.
    let (h0, p0) = split(&f.nodes[0]);
    assert!(pool.port_for(&h0, p0, TIMEOUT).is_ok());
    assert_eq!(pool.len(), 2);
}

// ---------------------------------------------------------------------------------------------
// socks5
// ---------------------------------------------------------------------------------------------

#[test]
#[ignore = "needs the V-G03 Docker fixture; run by the ssh-tunnels CI job"]
fn socks5_mode_follows_moved_without_opening_anything_new() {
    // The difference from per-node, and why §21.5 recommends this one for a cluster: the same
    // single forward reaches every node, so following a redirect costs a CONNECT rather than
    // a new `ssh` process.
    let f = fixture();
    let tunnel = start(&bastion(&f), Mode::Socks5, None, TIMEOUT).expect("socks5 tunnel");
    let proxy = tunnel.local_port();

    let (seed_host, seed_port) = split(&f.nodes[0]);
    let sock = socks5_connect(proxy, &seed_host, seed_port, TIMEOUT).expect("connect to seed");
    let mut seed = Oneshot::over(sock);

    let mut redirect = None;
    for k in PROBE_KEYS {
        if let Some(t) = moved_target(&mut seed, k) {
            redirect = Some((*k, t));
            break;
        }
    }
    let (key, target) = redirect.expect("no key was MOVED; the fixture changed");
    let (host, port) = split(&target);

    // Follow it through the *same* proxy.
    let sock = socks5_connect(proxy, &host, port, TIMEOUT).expect("connect to the owner");
    let mut owner = Oneshot::over(sock);
    let reply = owner
        .call(&[
            Bytes::from_static(b"GET"),
            Bytes::from(key.as_bytes().to_vec()),
        ])
        .expect("GET on the owner");
    assert!(
        !matches!(&reply, Value::Error(e) if e.starts_with(b"MOVED")),
        "the owner still redirected: {reply:?}"
    );
}

#[test]
#[ignore = "needs the V-G03 Docker fixture; run by the ssh-tunnels CI job"]
fn socks5_reaches_every_node_through_one_forward() {
    let f = fixture();
    let tunnel = start(&bastion(&f), Mode::Socks5, None, TIMEOUT).expect("socks5 tunnel");
    for n in &f.nodes {
        let (h, p) = split(n);
        let sock = socks5_connect(tunnel.local_port(), &h, p, TIMEOUT)
            .unwrap_or_else(|e| panic!("{n} through the proxy: {e}"));
        let mut c = Oneshot::over(sock);
        let reply = c.call(&[Bytes::from_static(b"PING")]).expect("PING");
        assert!(
            matches!(reply, Value::Simple(ref s) if s.as_ref() == b"PONG"),
            "{reply:?}"
        );
    }
}

// ---------------------------------------------------------------------------------------------
// Host key checking, and NET-07
// ---------------------------------------------------------------------------------------------

#[test]
#[ignore = "needs the V-G03 Docker fixture; run by the ssh-tunnels CI job"]
fn an_unknown_host_key_is_refused_rather_than_accepted_on_first_use() {
    // The check that makes all the others worth anything: point the tunnel at a known-hosts
    // file that does not contain this bastion, and it must fail rather than learn.
    let f = fixture();
    let mut b = bastion(&f);
    let empty = std::env::temp_dir().join(format!("penguin-vg03-empty-{}", std::process::id()));
    std::fs::write(&empty, "# this bastion is not in here\n").unwrap();
    b.known_hosts = empty.clone();

    let err = start(&b, Mode::Socks5, None, Duration::from_secs(8)).unwrap_err();
    assert!(
        matches!(
            err,
            pr_transport::ssh::SshError::Exited(_) | pr_transport::ssh::SshError::Timeout(_)
        ),
        "an unknown host key was accepted: {err:?}"
    );
    // And it did not write the key in: a client that learns is a client that can be taught
    // once by whoever is in the middle that day.
    let after = std::fs::read_to_string(&empty).unwrap();
    assert_eq!(
        after, "# this bastion is not in here\n",
        "the key was learned"
    );
    let _ = std::fs::remove_file(&empty);
}

#[test]
#[ignore = "needs the V-G03 Docker fixture; run by the ssh-tunnels CI job"]
fn dropping_a_tunnel_stops_its_process_and_frees_its_port() {
    // NET-07's observable half: our own resources go away. That the user's other sessions
    // survive is guaranteed by ControlPath=none, which is asserted on the argv in
    // `ssh_tunnels.rs` -- a property of our process cannot observe somebody else's.
    let f = fixture();
    let port = {
        let mut t = start(&bastion(&f), Mode::Socks5, None, TIMEOUT).expect("tunnel");
        assert!(t.is_running().unwrap());
        t.local_port()
    };
    // The tunnel dropped here. The port must come back.
    let mut freed = false;
    for _ in 0..100 {
        if std::net::TcpListener::bind(("127.0.0.1", port)).is_ok() {
            freed = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(
        freed,
        "port {port} is still held after the tunnel was dropped"
    );
}
