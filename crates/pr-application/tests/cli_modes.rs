//! V-I04 — the `redis-cli` special-mode inventory (v2.1 §28.4, §28.5, R36).
//!
//! §28.4 is blunt about why this exists: "名称与详细 flags 由该 baseline 的 help/source 生成
//! 登记，不能因为本段列了名字就标记实现完成." A list written from memory is a list that
//! quietly omits the mode nobody thought of, and the omission only shows up when a user runs
//! it and finds nothing.
//!
//! So `compatibility/redis-cli-modes.toml` is checked against the baseline's own help output,
//! committed beside it. The checks run in both directions, because both failures are real:
//!
//! - a mode in the inventory that the baseline does not have is a contract for nothing
//! - a mode in the baseline that the inventory does not have is a compatibility gap nobody
//!   has decided about

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, missing_docs)]

use std::collections::BTreeSet;

const INVENTORY: &str = include_str!("../../../compatibility/redis-cli-modes.toml");
const HELP: &str = include_str!("../../../compatibility/redis-cli-8.0.6-help.txt");

/// §28.4's table rows, as the inventory names them.
const CONTRACT_ROWS: &[&str] = &[
    "scan-analysis",
    "stat-latency",
    "pipe",
    "rdb-export",
    "replica-stream",
    "scripting",
    "repeat",
    "lru-test",
    "cluster-management",
];

/// Flags the baseline has that are not special modes: connection, authentication, TLS and
/// output formatting. §28.4 is about modes that need their own spec, fixtures, exit semantics
/// and safety policy; `-h` does not.
///
/// Listed explicitly rather than filtered by a rule, so that a new flag lands in neither set
/// and fails the test instead of being silently classified.
const NOT_A_MODE: &[&str] = &[
    "-h",
    "-p",
    "-s",
    "-u",
    "-a",
    "-n",
    "-c",
    "-d",
    "-e",
    "-t",
    "-x",
    "-X",
    "-D",
    "-4",
    "-6",
    // RESP version selection. Not a mode, but not nothing either: §18.1 says `--json`'s
    // preference for RESP3 must never override an explicit choice here, and that rule is
    // tested in `push_output.rs`.
    "-2",
    "-3",
    "--user",
    "--pass",
    "--askpass",
    "--no-auth-warning",
    "--tls",
    "--sni",
    "--cacert",
    "--cacertdir",
    "--cert",
    "--key",
    "--insecure",
    "--tls-ciphers",
    "--tls-ciphersuites",
    "--raw",
    "--no-raw",
    "--csv",
    "--json",
    "--quoted-json",
    "--quoted-input",
    "--show-pushes",
    "--verbose",
    "--help",
    "--version",
    // Options that modify a mode rather than being one; each is listed on its mode's entry.
    "--pattern",
    "--quoted-pattern",
    "--count",
    "--cursor",
    "--top",
    "--memkeys-samples",
    "--keystats-samples",
    "--pipe-timeout",
    "-r",
    "-i",
];

#[derive(Debug)]
struct Mode {
    flag: String,
    contract: String,
    prod: String,
    agent: String,
    cancel: String,
    note: Option<String>,
    options: Vec<String>,
    subcommands: Vec<String>,
}

fn inventory() -> (toml::Table, Vec<Mode>) {
    let table: toml::Table = INVENTORY.parse().expect("the inventory is valid TOML");
    let modes = table["mode"]
        .as_array()
        .expect("[[mode]] entries")
        .iter()
        .map(|m| {
            let get = |k: &str| m.get(k).and_then(toml::Value::as_str).map(str::to_owned);
            let list = |k: &str| {
                m.get(k)
                    .and_then(toml::Value::as_array)
                    .map(|a| {
                        a.iter()
                            .filter_map(|v| v.as_str().map(str::to_owned))
                            .collect()
                    })
                    .unwrap_or_default()
            };
            Mode {
                flag: get("flag").expect("every mode has a flag"),
                contract: get("contract").expect("every mode names its §28.4 row"),
                prod: get("prod").expect("every mode states its production default"),
                agent: get("agent").expect("every mode states its agent default"),
                cancel: get("cancel").expect("every mode states its cancellation behaviour"),
                note: get("note"),
                options: list("options"),
                subcommands: list("subcommands"),
            }
        })
        .collect();
    (table, modes)
}

/// Every long or short flag the baseline's help mentions at the start of a line.
fn flags_in_help() -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    for line in HELP.lines() {
        let trimmed = line.trim_start();
        if line.len() == trimmed.len() || !trimmed.starts_with('-') {
            continue;
        }
        let flag: String = trimmed
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '-')
            .collect();
        if flag.len() > 1 && flag.starts_with('-') {
            out.insert(flag);
        }
    }
    out
}

// ================================================ the inventory describes the baseline
#[test]
fn the_baseline_is_pinned_by_digest() {
    let (t, _) = inventory();
    let b = t["baseline"].as_table().unwrap();
    assert_eq!(b["tool"].as_str(), Some("redis-cli"));
    assert_eq!(b["version"].as_str(), Some("8.0.6"));
    assert!(
        b["image"].as_str().unwrap().contains("@sha256:"),
        "an inventory generated from a floating tag describes nothing in particular"
    );
    assert!(
        HELP.contains(b["image"].as_str().unwrap()),
        "the help fixture came from the same image"
    );
}

#[test]
fn every_mode_in_the_inventory_exists_in_the_baseline() {
    // A contract for a flag the tool does not have is a contract for nothing.
    let (_, modes) = inventory();
    let help = flags_in_help();
    for m in &modes {
        assert!(
            help.contains(&m.flag),
            "{} is in the inventory but not in redis-cli 8.0.6's help",
            m.flag
        );
        for o in &m.options {
            assert!(
                help.contains(o),
                "{}'s option {o} is not in the baseline's help",
                m.flag
            );
        }
    }
}

#[test]
fn every_flag_in_the_baseline_is_either_a_mode_or_explicitly_not_one() {
    // The direction that catches an omission. A new flag lands in neither set and fails here
    // rather than being silently classified by a rule that happens to match it.
    let (_, modes) = inventory();
    let known: BTreeSet<&str> = modes
        .iter()
        .map(|m| m.flag.as_str())
        .chain(
            modes
                .iter()
                .flat_map(|m| m.options.iter().map(String::as_str)),
        )
        .chain(NOT_A_MODE.iter().copied())
        .collect();

    let in_help = flags_in_help();
    let missing: Vec<&String> = in_help
        .iter()
        .filter(|f| !known.contains(f.as_str()))
        .collect();
    assert!(
        missing.is_empty(),
        "redis-cli 8.0.6 has flags the inventory neither records as a mode nor lists as \
         not-a-mode: {missing:?}"
    );
}

// ================================================ the inventory matches §28.4
#[test]
fn every_row_of_section_28_4_is_covered() {
    let (_, modes) = inventory();
    for row in CONTRACT_ROWS {
        assert!(
            modes.iter().any(|m| m.contract == *row),
            "§28.4's `{row}` row has no mode in the inventory"
        );
    }
}

#[test]
fn every_mode_names_a_real_contract_row_or_says_it_has_none() {
    let (_, modes) = inventory();
    for m in &modes {
        assert!(
            CONTRACT_ROWS.contains(&m.contract.as_str()) || m.contract == "uncontracted",
            "{} claims contract row {:?}, which is not in §28.4",
            m.flag,
            m.contract
        );
    }
}

#[test]
fn an_uncontracted_mode_must_explain_itself() {
    // "Uncontracted" is a finding, not a shrug. The note is what makes it actionable rather
    // than a label that means "we did not look".
    let (_, modes) = inventory();
    let uncontracted: Vec<&Mode> = modes
        .iter()
        .filter(|m| m.contract == "uncontracted")
        .collect();
    assert!(
        !uncontracted.is_empty(),
        "the baseline really does have modes §28.4 does not name; an empty list here means \
         the comparison was not done"
    );
    for m in uncontracted {
        let note = m
            .note
            .as_ref()
            .unwrap_or_else(|| panic!("{} is uncontracted with no explanation", m.flag));
        assert!(
            note.len() > 80,
            "{}'s note is too short to say what the gap is: {note:?}",
            m.flag
        );
    }
}

// ================================================ the safety columns
#[test]
fn every_mode_states_a_production_and_an_agent_default() {
    // §28.4's last paragraph: "deny 是策略默认值……agent origin 的 deny 只能由人工在 profile
    // 中开放". A mode with no stated default is a mode whose default is whatever the code did.
    let (_, modes) = inventory();
    for m in &modes {
        assert!(!m.prod.is_empty(), "{} has no production default", m.flag);
        assert!(!m.agent.is_empty(), "{} has no agent default", m.flag);
        assert!(
            !m.cancel.is_empty(),
            "{} says nothing about cancellation",
            m.flag
        );
    }
}

#[test]
fn the_modes_that_export_data_or_make_load_are_denied_in_production_and_for_agents() {
    // These are §28.4's explicit denials, and they are the ones worth a test of their own:
    // each moves the whole dataset off the server or generates load on it.
    let (_, modes) = inventory();
    for flag in ["--rdb", "--functions-rdb", "--replica", "--lru-test"] {
        let m = modes
            .iter()
            .find(|m| m.flag == flag)
            .unwrap_or_else(|| panic!("{flag} missing"));
        assert_eq!(m.prod, "deny", "{flag} must be denied in production");
        assert_eq!(m.agent, "deny", "{flag} must be denied for agents");
    }
}

#[test]
fn no_mode_that_writes_or_changes_topology_is_open_to_an_agent() {
    // §28.4: an agent's deny may only be opened by a human editing the profile, never by the
    // request itself. The inventory has to reflect that, or the policy layer will be built
    // from a table that already allows it.
    let (_, modes) = inventory();
    for flag in ["--pipe", "--eval", "--ldb", "--ldb-sync-mode", "--cluster"] {
        let m = modes.iter().find(|m| m.flag == flag).unwrap();
        assert_eq!(m.agent, "deny", "{flag} is open to an agent");
    }
}

#[test]
fn the_synchronous_debugger_needs_more_than_the_ordinary_script_policy() {
    // It blocks the server: the cost is paid by every other client, not by the person
    // debugging. §28.4 gives it its own confirmation for that reason.
    let (_, modes) = inventory();
    let sync = modes.iter().find(|m| m.flag == "--ldb-sync-mode").unwrap();
    let plain = modes.iter().find(|m| m.flag == "--ldb").unwrap();
    assert_ne!(
        sync.prod, plain.prod,
        "the blocking debugger has the same production policy as the non-blocking one"
    );
    assert!(sync.prod.contains("confirmation"), "{}", sync.prod);
    assert!(sync.note.is_some(), "and it says why");
}

#[test]
fn the_cluster_manager_subcommands_are_recorded_from_the_baseline() {
    // §28.5 needs a full action plan per subcommand, which is not possible without knowing
    // which subcommands there are.
    let (_, modes) = inventory();
    let cluster = modes.iter().find(|m| m.flag == "--cluster").unwrap();
    for sub in [
        "create",
        "check",
        "info",
        "fix",
        "reshard",
        "rebalance",
        "add-node",
        "del-node",
        "call",
        "set-timeout",
        "import",
        "backup",
    ] {
        assert!(
            cluster.subcommands.iter().any(|s| s == sub),
            "cluster subcommand {sub} is missing from the inventory"
        );
    }
    assert!(
        cluster.note.as_ref().unwrap().contains("backup"),
        "the note must flag that `backup` and `call` do more than change topology"
    );
}

#[test]
fn the_repeat_flag_multiplies_the_policy_rather_than_inheriting_it_once() {
    // A write approved once is not a write approved a thousand times.
    let (_, modes) = inventory();
    let r = modes.iter().find(|m| m.flag == "-r").unwrap();
    assert!(r.prod.contains("budget"), "{}", r.prod);
    assert!(r.agent.contains("budget"), "{}", r.agent);
}
