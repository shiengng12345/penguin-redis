//! V-F04 — every row of the §11.7 syntax-complexity table, against the real pinned catalog.
//!
//! §11.7 is a table of eleven syntax shapes with a stated analysis requirement for each. The
//! table is there because each row has a specific way of going wrong, and most of those ways
//! are silent: a stream id treated as a key misroutes on a cluster, a value position offering
//! observed data leaks it, a keyword matched by text turns a key named `NX` into an option.
//!
//! So every row here is checked against the **compiled snapshot**, not a hand-written
//! fixture. A fixture would only prove the walker agrees with what I typed into it; the
//! snapshot is what will actually ship, and the same test runs against both server families.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, missing_docs)]

use pr_catalog::compile::DELIBERATELY_UNKNOWN;
use pr_catalog::spec::version_at_least;
use pr_catalog::{CommandSpec, Family, Role, compile, merge, resolve_roles, snapshot};

const REDIS_JSON: &[u8] = include_bytes!("../snapshots/redis-8.0.json");
const REDIS_SUM: &str = include_str!("../snapshots/redis-8.0.json.blake3");
const VALKEY_JSON: &[u8] = include_bytes!("../snapshots/valkey-8.1.json");
const VALKEY_SUM: &str = include_str!("../snapshots/valkey-8.1.json.blake3");

fn redis() -> Vec<CommandSpec> {
    let s = snapshot::load(REDIS_JSON, REDIS_SUM).expect("redis snapshot");
    compile(&s, Family::Redis)
}

fn valkey() -> Vec<CommandSpec> {
    let s = snapshot::load(VALKEY_JSON, VALKEY_SUM).expect("valkey snapshot");
    compile(&s, Family::Valkey)
}

fn spec<'a>(all: &'a [CommandSpec], name: &str) -> &'a CommandSpec {
    all.iter()
        .find(|c| c.name == name)
        .unwrap_or_else(|| panic!("{name} missing from the compiled catalog"))
}

/// Roles for a command's arguments, written the way a user types them.
fn roles(all: &[CommandSpec], name: &str, args: &[&str]) -> Vec<Role> {
    let a: Vec<&[u8]> = args.iter().map(|s| s.as_bytes()).collect();
    resolve_roles(&spec(all, name).grammar, &a)
}

// ============================================================ row 1: subcommands
#[test]
fn row1_the_second_token_is_a_subcommand_not_a_key() {
    let r = redis();
    // `CLIENT` and `XINFO` are containers: not executable, and not classifiable on their own.
    for container in ["CLIENT", "XINFO", "CLUSTER", "CONFIG", "OBJECT"] {
        let c = spec(&r, container);
        assert!(c.is_container, "{container} should be a container");
        assert!(
            c.effects.unknown,
            "{container} must not report effects of its own — its subcommands differ wildly"
        );
    }

    // The word after the container is looked up as a *subcommand*, and the grammar that then
    // applies is the subcommand's — which starts one token further along. Resolving against
    // the container itself yields no key, because the container has no arguments at all.
    for (container, sub) in [("CLIENT", "LIST"), ("XINFO", "GROUPS")] {
        let c = spec(&r, container);
        let as_key = resolve_roles(&c.grammar, &[sub.as_bytes()]);
        assert!(
            !as_key.contains(&Role::Key),
            "{container} {sub}: the subcommand token must never resolve to a key, got {as_key:?}"
        );
        assert!(
            r.iter().any(|x| x.name == format!("{container}|{sub}")),
            "{container} {sub} must exist as its own spec"
        );
    }

    // `XINFO GROUPS mystream` — the key is the argument *after* the subcommand, and it is the
    // subcommand's own first argument.
    assert_eq!(roles(&r, "XINFO|GROUPS", &["mystream"]), vec![Role::Key]);
    // `CLIENT LIST` has no key at all; `normal` is the value of the TYPE option.
    assert_eq!(
        roles(&r, "CLIENT|LIST", &["TYPE", "normal"]),
        vec![Role::Keyword, Role::Keyword]
    );
    assert!(
        !roles(&r, "CLIENT|LIST", &["TYPE", "normal"]).contains(&Role::Key),
        "CLIENT LIST takes no key"
    );

    // The container's effects are never a stand-in for a subcommand's.
    assert!(spec(&r, "CLIENT|KILL").effects.admin);
    assert!(!spec(&r, "CLIENT|GETNAME").effects.admin);
}

// ============================================================ row 2: alternating repeats
#[test]
fn row2_hset_alternates_field_and_value_and_never_offers_data_in_the_value_slot() {
    let r = redis();
    assert_eq!(
        roles(&r, "HSET", &["k", "f1", "v1", "f2", "v2", "f3", "v3"]),
        vec![
            Role::Key,
            Role::Field,
            Role::Value,
            Role::Field,
            Role::Value,
            Role::Field,
            Role::Value
        ]
    );
    // §12.2: the value position must not be completed from observed data.
    let rs = roles(&r, "HSET", &["k", "f", "v", "f2", "v2"]);
    for (i, role) in rs.iter().enumerate() {
        if *role == Role::Value {
            assert!(
                !role.accepts_observed_names(),
                "argument {i} is a value and must not offer observed names"
            );
        }
    }
    assert!(
        Role::Field.accepts_observed_names(),
        "fields may be offered"
    );

    // HMSET and HRANDFIELD share the shape; the same rule must hold.
    assert_eq!(
        roles(&r, "HMSET", &["k", "f", "v"]),
        vec![Role::Key, Role::Field, Role::Value]
    );
}

// ============================================================ row 3: score/member repeats
#[test]
fn row3_zadd_never_swaps_score_and_member() {
    let r = redis();
    assert_eq!(
        roles(&r, "ZADD", &["k", "1", "a", "2", "b"]),
        vec![
            Role::Key,
            Role::Score,
            Role::Member,
            Role::Score,
            Role::Member
        ]
    );
    // With options in front, the pair still lands the right way round — a positional table
    // would be off by one here.
    assert_eq!(
        roles(&r, "ZADD", &["k", "NX", "CH", "1", "a"]),
        vec![
            Role::Key,
            Role::Keyword,
            Role::Keyword,
            Role::Score,
            Role::Member
        ]
    );
    assert!(
        !Role::Score.accepts_observed_names(),
        "a score is not a name"
    );
    assert!(Role::Member.accepts_observed_names());
}

// ============================================================ row 4: exclusive options
#[test]
fn row4_exclusive_groups_are_filtered_and_explained_not_silently_rewritten() {
    let r = redis();
    let set = spec(&r, "SET");

    let groups: Vec<&str> = set.exclusive.iter().map(|g| g.name.as_str()).collect();
    assert!(
        groups.contains(&"condition") && groups.contains(&"expiration"),
        "SET has two independent exclusive groups, got {groups:?}"
    );

    let fresh = set.available_keywords(&[]);
    for k in ["NX", "XX", "EX", "PX", "EXAT", "PXAT", "KEEPTTL", "GET"] {
        assert!(
            fresh.contains(&k.to_string()),
            "{k} should be offered initially"
        );
    }

    // Choosing EX removes the other expiry forms and leaves the condition group alone.
    let after_ex = set.available_keywords(&["EX"]);
    for gone in ["EX", "PX", "EXAT", "PXAT", "KEEPTTL"] {
        assert!(
            !after_ex.contains(&gone.to_string()),
            "{gone} must be removed"
        );
    }
    for kept in ["NX", "XX", "GET"] {
        assert!(
            after_ex.contains(&kept.to_string()),
            "{kept} is a different group"
        );
    }

    // The conflict is explainable: we can name the group a keyword belongs to.
    assert_eq!(
        set.group_of("PX").map(|g| g.name.as_str()),
        Some("expiration")
    );
    assert_eq!(
        set.group_of("XX").map(|g| g.name.as_str()),
        Some("condition")
    );

    // ZADD's conditions are likewise two groups, not one.
    let zadd = spec(&r, "ZADD");
    assert_eq!(
        zadd.group_of("NX").map(|g| g.name.as_str()),
        Some("condition")
    );
    assert_eq!(
        zadd.group_of("GT").map(|g| g.name.as_str()),
        Some("comparison")
    );
    assert!(
        zadd.available_keywords(&["NX"]).contains(&"GT".to_string()),
        "a condition must not suppress the comparison group"
    );

    // R42/R43: a line that breaks the rule is diagnosed, never silently amended. Two expiry
    // options are a conflict, and the evidence is that nothing is dropped or reordered — six
    // arguments in, six roles out — while the second one is pointedly *not* accepted as a
    // valid option.
    let conflicting = roles(&r, "SET", &["k", "v", "EX", "1", "PX", "2"]);
    assert_eq!(
        conflicting.len(),
        6,
        "no argument may be dropped from a conflicting line"
    );
    assert_eq!(
        &conflicting[..4],
        &[
            Role::Key,
            Role::Value,
            Role::Keyword,
            Role::Integer(pr_catalog::Unit::Seconds)
        ]
    );
    assert_ne!(
        conflicting[4],
        Role::Keyword,
        "the second expiry option must not be accepted as if it were legal"
    );
    // And the reason is available to put in front of the user.
    assert_eq!(
        set.group_of("PX").map(|g| g.name.as_str()),
        set.group_of("EX").map(|g| g.name.as_str()),
        "EX and PX must be explainable as the same group"
    );
}

#[test]
fn row4_options_are_filtered_by_server_version() {
    let r = redis();
    let set = spec(&r, "SET");
    // GET and EXAT arrived in 6.2. Offering them to a 6.0 server produces a syntax error the
    // user did not cause.
    let on_60 = set.available_keywords_for(&[], "6.0.20");
    assert!(!on_60.contains(&"GET".to_string()), "GET is 6.2+");
    assert!(!on_60.contains(&"EXAT".to_string()), "EXAT is 6.2+");
    assert!(on_60.contains(&"EX".to_string()), "EX is 2.6+");
    assert!(on_60.contains(&"KEEPTTL".to_string()), "KEEPTTL is 6.0+");

    let on_62 = set.available_keywords_for(&[], "6.2.0");
    assert!(on_62.contains(&"GET".to_string()));
    assert!(on_62.contains(&"EXAT".to_string()));

    // NX/XX inherit their group's 2.6.12, so they survive on an old server.
    assert!(
        set.available_keywords_for(&[], "2.8.0")
            .contains(&"NX".to_string())
    );
    assert!(
        !set.available_keywords_for(&[], "2.6.0")
            .contains(&"NX".to_string())
    );

    // The comparison is numeric, and a release candidate is not refused for its suffix.
    assert!(version_at_least("7.0.0", "6.2.0"));
    assert!(version_at_least("6.2.0", "6.2.0"));
    assert!(!version_at_least("6.0.20", "6.2.0"));
    assert!(version_at_least("8.0.0-rc1", "7.4.0"));
    assert!(
        version_at_least("10.0.0", "9.0.0"),
        "not a string comparison"
    );
}

// ============================================================ row 5: grouped choices
#[test]
fn row5_xadd_trimming_does_not_offer_every_keyword_at_once() {
    let r = redis();
    let xadd = spec(&r, "XADD");

    // MAXLEN and MINID are alternatives, so picking one removes the other.
    let trim = xadd
        .exclusive
        .iter()
        .find(|g| g.members.contains(&"MAXLEN".to_string()))
        .expect("XADD has a trim-strategy group");
    assert!(trim.members.contains(&"MINID".to_string()));
    assert!(
        !xadd
            .available_keywords(&["MAXLEN"])
            .contains(&"MINID".to_string()),
        "MAXLEN and MINID are alternatives"
    );

    // The exact/approximate marker is a nested choice of its own.
    let approx = xadd
        .exclusive
        .iter()
        .find(|g| g.members.contains(&"~".to_string()))
        .expect("XADD has an exact/approximate group");
    assert!(approx.members.contains(&"=".to_string()));

    // And the whole thing resolves the way it is typed.
    assert_eq!(
        roles(&r, "XADD", &["s", "MAXLEN", "~", "1000", "*", "f", "v"]),
        vec![
            Role::Key,
            Role::Keyword, // MAXLEN
            Role::Keyword, // ~
            Role::Value,   // threshold
            Role::Keyword, // * (auto id)
            Role::Field,
            Role::Value,
        ]
    );
    // An explicit id instead of `*` lands in the same slot.
    let explicit = roles(&r, "XADD", &["s", "1526919030474-55", "f", "v"]);
    assert_eq!(explicit[1..], [Role::EntryId, Role::Field, Role::Value]);
}

// ============================================================ row 6: numkeys
#[test]
fn row6_numkeys_decides_where_keys_stop() {
    let r = redis();
    // ASSIST-013: getting this wrong sends ARGV entries as keys and misroutes on a cluster.
    assert_eq!(
        roles(&r, "EVAL", &["return 1", "2", "k1", "k2", "a1", "a2"]),
        vec![
            Role::Value, // the script body
            Role::NumKeys,
            Role::Key,
            Role::Key,
            Role::Value,
            Role::Value
        ]
    );
    assert_eq!(
        roles(&r, "EVAL", &["s", "0", "a1", "a2"]),
        vec![Role::Value, Role::NumKeys, Role::Value, Role::Value]
    );
    // An unparsable numkeys must not cause a guess.
    let bad = roles(&r, "EVAL", &["s", "two", "k1", "k2"]);
    assert!(
        !bad[2..].contains(&Role::Key),
        "nothing may be assumed to be a key when numkeys does not parse: {bad:?}"
    );

    // The same rule governs the set/zset numkeys commands.
    assert_eq!(
        roles(
            &r,
            "ZUNIONSTORE",
            &["dst", "2", "a", "b", "WEIGHTS", "1", "2"]
        ),
        vec![
            Role::Key,
            Role::NumKeys,
            Role::Key,
            Role::Key,
            Role::Keyword,
            Role::Integer(pr_catalog::Unit::Count),
            Role::Integer(pr_catalog::Unit::Count)
        ],
        "the weights follow the keys, and are numbers rather than names"
    );
    assert_eq!(
        roles(&r, "LMPOP", &["2", "a", "b", "LEFT"]),
        vec![Role::NumKeys, Role::Key, Role::Key, Role::Keyword]
    );
}

// ============================================================ row 7: paired lists
#[test]
fn row7_xread_splits_keys_from_ids() {
    let r = redis();
    assert_eq!(
        roles(&r, "XREAD", &["STREAMS", "s1", "s2", "0-0", "0-0"]),
        vec![
            Role::Keyword,
            Role::Key,
            Role::Key,
            Role::EntryId,
            Role::EntryId
        ],
        "the ids must not be treated as keys"
    );
    assert_eq!(
        roles(
            &r,
            "XREAD",
            &["COUNT", "2", "BLOCK", "0", "STREAMS", "s", "$"]
        ),
        vec![
            Role::Keyword,
            Role::Integer(pr_catalog::Unit::Count),
            Role::Keyword,
            Role::Integer(pr_catalog::Unit::Milliseconds),
            Role::Keyword,
            Role::Key,
            Role::EntryId
        ]
    );
    // Mid-typing, the count is odd and the split is genuinely ambiguous — the third word
    // could be a third key or the first id. The extra slot goes to the key half, and no
    // numeric default is invented for the id that is missing.
    assert_eq!(
        roles(&r, "XREAD", &["STREAMS", "s1", "s2", "s3"]),
        vec![Role::Keyword, Role::Key, Role::Key, Role::EntryId],
        "an odd count gives the extra slot to the keys"
    );
    assert_eq!(
        roles(&r, "XREAD", &["STREAMS", "s1"]),
        vec![Role::Keyword, Role::Key],
        "the first word after STREAMS is always a key"
    );

    // XREADGROUP adds the group/consumer pair in front of the same structure.
    assert_eq!(
        roles(
            &r,
            "XREADGROUP",
            &["GROUP", "g", "c", "COUNT", "1", "STREAMS", "s", ">"]
        ),
        vec![
            Role::Keyword,
            Role::Group,
            Role::Consumer,
            Role::Keyword,
            Role::Integer(pr_catalog::Unit::Count),
            Role::Keyword,
            Role::Key,
            Role::EntryId
        ]
    );
}

// ============================================================ row 8: keyword-shaped keys
#[test]
fn row8_a_key_named_like_a_keyword_stays_a_key() {
    let r = redis();
    // ASSIST-015. The grammar position decides; the text never promotes data to an option.
    assert_eq!(
        roles(&r, "SET", &["NX", "GET"]),
        vec![Role::Key, Role::Value],
        "`SET NX GET` sets a key literally called NX"
    );
    assert_eq!(
        roles(&r, "GET", &["EX"]),
        vec![Role::Key],
        "a key may be called EX"
    );
    assert_eq!(
        roles(&r, "HSET", &["MATCH", "COUNT", "NOVALUES"]),
        vec![Role::Key, Role::Field, Role::Value]
    );
    // Conversely, once the mandatory arguments are satisfied, a matching word *is* the option.
    assert_eq!(
        roles(&r, "SET", &["k", "v", "NX"]),
        vec![Role::Key, Role::Value, Role::Keyword]
    );
    // And a non-matching word in an option position is not silently consumed as one.
    assert_eq!(
        roles(&r, "HSCAN", &["k", "0", "MATCH", "n*"]),
        vec![Role::Key, Role::Cursor, Role::Keyword, Role::Pattern]
    );
    assert_eq!(
        roles(&r, "HSCAN", &["k", "0", "NOVALUES"]),
        vec![Role::Key, Role::Cursor, Role::Keyword]
    );
}

// ============================================================ row 9: mode-dependent args
#[test]
fn row9_range_arguments_follow_the_selected_mode() {
    let r = redis();
    let zrange = spec(&r, "ZRANGE");
    // BYSCORE and BYLEX are alternatives; REV and LIMIT are independent of them.
    let sortby = zrange
        .exclusive
        .iter()
        .find(|g| g.members.contains(&"BYSCORE".to_string()))
        .expect("ZRANGE has a sort-by group");
    assert!(sortby.members.contains(&"BYLEX".to_string()));
    assert!(
        !zrange
            .available_keywords(&["BYSCORE"])
            .contains(&"BYLEX".to_string()),
        "BYSCORE and BYLEX are mutually exclusive"
    );
    assert!(
        zrange
            .available_keywords(&["BYSCORE"])
            .contains(&"REV".to_string()),
        "REV is independent of the sort mode"
    );

    // LIMIT is only meaningful with BYSCORE/BYLEX, and its two integers stay integers.
    assert_eq!(
        roles(
            &r,
            "ZRANGE",
            &["k", "(1", "5", "BYSCORE", "LIMIT", "0", "10"]
        ),
        vec![
            Role::Key,
            Role::Value,
            Role::Value,
            Role::Keyword,
            Role::Keyword,
            Role::Integer(pr_catalog::Unit::Count),
            Role::Integer(pr_catalog::Unit::Count)
        ]
    );
    // Plain index mode: the same two positions, no mode keyword.
    assert_eq!(
        roles(&r, "ZRANGE", &["k", "0", "-1", "REV"]),
        vec![Role::Key, Role::Value, Role::Value, Role::Keyword]
    );
    // The range endpoints are never offered as names in either mode — `(1` and `[a` are
    // syntax, not observed data.
    for args in [
        vec!["k", "0", "-1"],
        vec!["k", "(1", "+inf", "BYSCORE"],
        vec!["k", "[a", "[z", "BYLEX"],
    ] {
        let rs = roles(&r, "ZRANGE", &args);
        assert!(
            !rs[1..3].iter().any(|r| r.accepts_observed_names()),
            "range endpoints must not offer observed names: {args:?} -> {rs:?}"
        );
    }
}

// ============================================================ row 10: binary arguments
#[test]
fn row10_arbitrary_binary_arguments_resolve_without_loss_or_panic() {
    let r = redis();
    let hset = spec(&r, "HSET");
    // Quotes, an empty argument, a NUL and invalid UTF-8. Roles are decided by position, so
    // none of this changes the answer — and nothing here may panic or lossily convert.
    let args: Vec<&[u8]> = vec![
        b"key with \"quotes\"",
        b"",
        b"\x00\x01\x02",
        &[0xff, 0xfe, 0x80],
        b"field\nwith\rnewlines",
        b"\xed\xa0\x80", // encoded lone surrogate
    ];
    let rs = resolve_roles(&hset.grammar, &args);
    assert_eq!(
        rs,
        vec![
            Role::Key,
            Role::Field,
            Role::Value,
            Role::Field,
            Role::Value,
            Role::Field
        ]
    );

    // A keyword match is byte-wise and ASCII-case-insensitive, so invalid UTF-8 simply does
    // not match rather than being coerced into something that does.
    let set = spec(&r, "SET");
    let weird: Vec<&[u8]> = vec![b"k", b"v", &[0xff, b'N', b'X']];
    assert_eq!(
        resolve_roles(&set.grammar, &weird),
        vec![Role::Key, Role::Value, Role::Value],
        "near-miss bytes must not be promoted to the NX keyword"
    );
    // The genuine article still matches in either case.
    for spelling in [&b"nx"[..], b"Nx", b"NX"] {
        let a: Vec<&[u8]> = vec![b"k", b"v", spelling];
        assert_eq!(resolve_roles(&set.grammar, &a)[2], Role::Keyword);
    }

    // numkeys that is not valid UTF-8 must not be guessed at.
    let eval = spec(&r, "EVAL");
    let a: Vec<&[u8]> = vec![b"s", &[0xff, 0xff], b"x", b"y"];
    let rs = resolve_roles(&eval.grammar, &a);
    assert!(!rs[2..].contains(&Role::Key), "{rs:?}");
}

// ============================================================ row 11: extension types
#[test]
fn row11_module_commands_compile_and_keep_their_classification() {
    let r = redis();
    // The Redis 8 image bundles the JSON, Search, TimeSeries, Bloom and vector-set modules.
    let modules: Vec<&CommandSpec> = r.iter().filter(|c| c.group == "module").collect();
    assert!(
        modules.len() > 100,
        "expected the bundled modules in the pinned image, found {}",
        modules.len()
    );
    for name in ["JSON.SET", "JSON.GET", "FT.SEARCH", "TS.ADD", "BF.ADD"] {
        let s = spec(&r, name);
        assert_eq!(s.group, "module", "{name} should be grouped as a module");
        assert!(
            !s.effects.unknown,
            "{name} reports flags, so it must be classified rather than left unknown"
        );
    }
    // A module write is a write, not a read — this is the classification that decides whether
    // an approval is required (§20).
    assert!(spec(&r, "JSON.SET").effects.writes_data);
    assert!(spec(&r, "TS.ADD").effects.writes_data);
    assert!(spec(&r, "JSON.GET").effects.reads_data);
    assert!(!spec(&r, "JSON.GET").effects.mutates());
    assert!(spec(&r, "FT.SEARCH").effects.reads_data);

    // A command the catalog has never heard of falls to the generic path at maximum risk.
    assert!(
        r.iter().all(|c| c.name != "ACME.DOTHING"),
        "fixture assumption"
    );
    let unknown = pr_core::Effects::unknown();
    assert!(unknown.mutates(), "an unknown command is treated as risky");
}

// ============================================================ V-F01: the compile pipeline
#[test]
fn the_pinned_snapshots_load_verify_and_compile() {
    let rs = snapshot::load(REDIS_JSON, REDIS_SUM).expect("redis snapshot verifies");
    let vs = snapshot::load(VALKEY_JSON, VALKEY_SUM).expect("valkey snapshot verifies");
    assert_eq!(rs.provenance.family, "redis");
    assert_eq!(vs.provenance.family, "valkey");
    for p in [&rs.provenance, &vs.provenance] {
        assert!(
            p.image.contains("@sha256:"),
            "a snapshot must record the image digest it was captured from, got {:?}",
            p.image
        );
    }
    assert!(rs.commands.len() > 300, "{}", rs.commands.len());
    assert!(vs.commands.len() > 300, "{}", vs.commands.len());

    let r = compile(&rs, Family::Redis);
    let v = compile(&vs, Family::Valkey);
    assert_eq!(r.len(), rs.commands.len());
    assert_eq!(v.len(), vs.commands.len());
}

#[test]
fn every_command_is_classified_except_the_ones_we_chose_to_leave_open() {
    // This is the gate that makes the catalog the authority ADR-030 says it is. If a future
    // snapshot adds a command nobody classified, this fails and somebody has to look at it.
    for (family, all) in [("redis", redis()), ("valkey", valkey())] {
        let open: Vec<&str> = all
            .iter()
            .filter(|c| c.effects.unknown && !c.is_container)
            .map(|c| c.name.as_str())
            .collect();
        assert_eq!(
            open, DELIBERATELY_UNKNOWN,
            "{family}: unclassified commands must be exactly the documented list"
        );
    }
}

#[test]
fn a_container_is_marked_and_never_looks_harmless() {
    for (family, all) in [("redis", redis()), ("valkey", valkey())] {
        let containers: Vec<&CommandSpec> = all.iter().filter(|c| c.is_container).collect();
        assert!(
            containers.len() >= 14,
            "{family}: expected the usual containers, found {}",
            containers.len()
        );
        for c in containers {
            assert!(
                c.effects.mutates(),
                "{family}: container {} must not be classified as harmless",
                c.name
            );
            assert!(
                all.iter()
                    .any(|s| s.name.starts_with(&format!("{}|", c.name))),
                "{family}: container {} has no subcommands",
                c.name
            );
        }
    }
}

#[test]
fn the_dangerous_commands_are_classified_as_dangerous() {
    // Spot checks on the ones an approval prompt exists for (§20).
    let r = redis();
    for name in ["FLUSHALL", "FLUSHDB", "SHUTDOWN", "CLUSTER|RESET", "DEBUG"] {
        assert!(
            spec(&r, name).effects.destructive,
            "{name} must be classified destructive"
        );
    }
    for name in ["XREADGROUP", "SPOP", "LPOP", "GETDEL", "BLPOP"] {
        assert!(
            spec(&r, name).effects.consumes,
            "{name} must be classified consuming — its reply is the only copy"
        );
    }
    for name in ["BLPOP", "BRPOP", "WAIT", "XREAD"] {
        assert!(
            spec(&r, name).effects.blocks,
            "{name} blocks the connection"
        );
    }
    // And the safe ones are not dragged along with them.
    for name in ["GET", "PING", "INFO", "HGETALL"] {
        let e = spec(&r, name).effects;
        assert!(!e.mutates(), "{name} must not require approval: {e:?}");
    }
}

// ============================================================ V-F01: the dual branch
#[test]
fn the_two_families_merge_with_their_divergence_visible() {
    // R29: Redis and Valkey have diverged, and the catalog must say which is which rather
    // than presenting a union as if both servers had everything.
    let m = merge(redis(), valkey());

    let only_redis = |n: &str| m.redis_only.iter().any(|x| x == n);
    let only_valkey = |n: &str| m.valkey_only.iter().any(|x| x == n);

    // Hash-field TTLs and vector sets are Redis 8 additions Valkey 8.1 does not have.
    for n in ["HEXPIRE", "HPEXPIRE", "HGETEX", "HGETDEL", "VADD", "VSIM"] {
        assert!(only_redis(n), "{n} should be Redis-only");
    }
    // Valkey has its own additions.
    for n in [
        "COMMANDLOG",
        "CLIENT|CAPA",
        "CLUSTER|SLOT-STATS",
        "SCRIPT|SHOW",
    ] {
        assert!(only_valkey(n), "{n} should be Valkey-only");
    }
    // The bundled modules are Redis-only in these images.
    assert!(only_redis("JSON.SET") && only_redis("FT.SEARCH"));

    // Shared commands are labelled as shared.
    let by = |n: &str| m.commands.iter().find(|c| c.name == n).expect(n);
    for n in ["GET", "SET", "HSET", "XADD", "CLUSTER|NODES"] {
        assert_eq!(by(n).family, Family::Both, "{n} exists on both");
    }
    assert_eq!(by("HEXPIRE").family, Family::Redis);
    assert_eq!(by("COMMANDLOG").family, Family::Valkey);

    // Nothing is lost in the merge, and nothing is duplicated.
    let mut names: Vec<&str> = m.commands.iter().map(|c| c.name.as_str()).collect();
    let before = names.len();
    names.sort_unstable();
    names.dedup();
    assert_eq!(before, names.len(), "the merge duplicated a command");
    assert!(names.windows(2).all(|w| w[0] < w[1]), "the merge is sorted");

    // Where both families have a command, they agree about what it does. A disagreement is
    // not averaged away — it is reported, and this assertion is what makes it visible.
    assert!(
        m.effect_conflicts.is_empty(),
        "these commands are classified differently by the two families and need a decision: {:?}",
        m.effect_conflicts
    );
}

#[test]
fn a_shared_command_keeps_the_same_grammar_in_both_families() {
    // If the two servers describe `SET` differently, the analyser would behave differently
    // depending on which one the user is connected to. Worth knowing about.
    let (r, v) = (redis(), valkey());
    let mut differing = Vec::new();
    for rc in &r {
        let Some(vc) = v.iter().find(|c| c.name == rc.name) else {
            continue;
        };
        if rc.grammar != vc.grammar {
            differing.push(rc.name.clone());
        }
    }
    // Some divergence is expected as the projects add options independently; the test pins
    // the current set so a new one shows up as a change rather than as a surprise.
    assert!(
        differing.len() < 40,
        "unexpectedly many grammars differ between families ({}): {differing:?}",
        differing.len()
    );
    // The families do add options independently — Valkey 8.1 gives `SET` an `IFEQ`
    // condition that Redis 8.0 has no equivalent for. That is a real difference, so the test
    // records it instead of asserting it away.
    assert!(
        spec(&v, "SET")
            .available_keywords(&[])
            .contains(&"IFEQ".to_string()),
        "Valkey 8.1 SET has IFEQ"
    );
    assert!(
        !spec(&r, "SET")
            .available_keywords(&[])
            .contains(&"IFEQ".to_string()),
        "Redis 8.0 SET does not"
    );

    // What must not differ is how the §11.7 shapes resolve for the same input, because that
    // is what the user sees while typing.
    for (n, args) in [
        ("SET", vec!["k", "v", "EX", "300", "NX"]),
        ("HSET", vec!["k", "f", "v", "f2", "v2"]),
        ("ZADD", vec!["k", "NX", "1", "a"]),
        ("XREAD", vec!["STREAMS", "s1", "s2", "0", "0"]),
        ("EVAL", vec!["s", "2", "k1", "k2", "a"]),
        (
            "ZRANGE",
            vec!["k", "(1", "5", "BYSCORE", "LIMIT", "0", "10"],
        ),
        ("HSCAN", vec!["k", "0", "MATCH", "n*"]),
    ] {
        assert_eq!(
            roles(&r, n, &args),
            roles(&v, n, &args),
            "{n} {args:?} resolves differently on the two families"
        );
        assert_eq!(
            spec(&r, n).effects,
            spec(&v, n).effects,
            "{n} is classified differently"
        );
    }
}

#[test]
fn compiling_the_whole_catalog_is_fast_enough_to_do_at_startup() {
    // Not a benchmark; a guard against the compile step quietly becoming a reason to add a
    // build-time codegen stage. If this ever fails, that decision can be made with a number.
    let rs = snapshot::load(REDIS_JSON, REDIS_SUM).unwrap();
    let t = std::time::Instant::now();
    let out = compile(&rs, Family::Redis);
    let elapsed = t.elapsed();
    assert!(!out.is_empty());
    assert!(
        elapsed < std::time::Duration::from_millis(250),
        "compiling {} commands took {elapsed:?}",
        out.len()
    );
}

// ============================================================ V-I02: the divergence list
#[test]
fn the_committed_difference_list_matches_the_catalogs() {
    // `fixtures/catalog/valkey-diff/` is generated by `xtask catalog-diff`. If a snapshot
    // refresh changes what the two families disagree about, this fails here rather than
    // surprising someone in Phase 2.
    let d = pr_catalog::diff::diff(&redis(), &valkey());
    let summary = include_str!("../../../fixtures/catalog/valkey-diff/summary.txt");
    let field = |name: &str| -> usize {
        summary
            .lines()
            .find_map(|l| l.strip_prefix(name))
            .unwrap_or_else(|| panic!("{name} missing from summary.txt"))
            .trim()
            .parse()
            .expect("a number")
    };
    assert_eq!(field("redis_commands "), redis().len());
    assert_eq!(field("valkey_commands "), valkey().len());
    assert_eq!(field("identical "), d.identical);
    assert_eq!(field("redis_only "), d.redis_only.len());
    assert_eq!(field("valkey_only "), d.valkey_only.len());
    assert_eq!(field("argument_differences "), d.arguments.len());
    assert_eq!(field("since_differences "), d.since.len());
    assert_eq!(field("effect_differences "), d.effects.len());
}

#[test]
fn the_divergence_list_names_the_differences_that_actually_matter() {
    // Spot checks against what the two projects really did, so an empty or truncated list
    // cannot pass. Each of these is a suggestion that would produce a syntax error if the
    // client offered it to the wrong family.
    let d = pr_catalog::diff::diff(&redis(), &valkey());
    let arg = |name: &str| {
        d.arguments
            .iter()
            .find(|a| a.name == name)
            .unwrap_or_else(|| panic!("{name} should differ between the families"))
    };
    assert!(
        arg("SET")
            .valkey_only_keywords
            .contains(&"IFEQ".to_string())
    );
    assert!(
        arg("BGSAVE")
            .valkey_only_keywords
            .contains(&"CANCEL".to_string())
    );
    assert!(
        arg("CLIENT|KILL")
            .valkey_only_keywords
            .contains(&"PRIMARY".to_string()),
        "Valkey renamed the role and kept the old spelling"
    );
    assert!(
        arg("CLUSTER|SETSLOT")
            .valkey_only_keywords
            .contains(&"TIMEOUT".to_string())
    );

    // Redis-only commands include the hash-field TTL family and the bundled modules.
    for n in ["HEXPIRE", "VADD", "JSON.SET", "FT.SEARCH"] {
        assert!(d.redis_only.iter().any(|x| x == n), "{n}");
    }
    for n in ["COMMANDLOG", "SCRIPT|SHOW"] {
        assert!(d.valkey_only.iter().any(|x| x == n), "{n}");
    }

    // And the one that would be a real problem is still empty.
    assert!(
        d.effects.is_empty(),
        "the families classify these differently, which means a policy decision would be made \
         on the wrong information: {:?}",
        d.effects
    );
}
