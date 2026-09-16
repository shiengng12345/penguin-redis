//! V-J02 — the frozen interface skeleton (v2.1 §11.11, §19.2, §21.3, §23.2, §3.5, §8.3,
//! §12.9, §31.4).
//!
//! Twelve types are frozen before Phase 1 starts, and the acceptance is specific:
//!
//! > Rust 类型 + doc test + schema 版本号 | 编译通过；**每个类型有 doc test**
//!
//! A doc test rather than prose because a type whose example does not compile is a type whose
//! documentation has already drifted from it — and the drift is invisible until someone tries
//! to use the type and finds the example was aspirational.
//!
//! This test lives in `prc` because `prc` is the one crate that links all of them. A test in
//! any single crate could only check its own.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, missing_docs)]

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

/// The twelve, with the crate that owns each and the file it is defined in.
///
/// Spelled out rather than derived: V-J02 names these twelve, and a list derived from what
/// happens to exist would silently shrink if one were deleted.
const CONTRACTS: &[(&str, &str, &str)] = &[
    ("CommandRequest", "pr-core", "src/request.rs"),
    ("ExecutionOutcome", "pr-core", "src/outcome.rs"),
    ("ResultRecord", "pr-core", "src/contract.rs"),
    ("SafeText", "pr-core", "src/safetext.rs"),
    ("TaskScope", "pr-core", "src/scope.rs"),
    ("CompletionRequest", "pr-intelligence", "src/contract.rs"),
    ("CompletionCandidate", "pr-intelligence", "src/contract.rs"),
    ("AssistanceSnapshot", "pr-intelligence", "src/contract.rs"),
    ("TrustIdentity", "pr-security", "src/trust.rs"),
    ("ApprovalToken", "pr-security", "src/approval.rs"),
    ("LocalCommandSpec", "pr-catalog", "src/contract.rs"),
    ("JsonNode", "pr-json", "src/dom.rs"),
];

/// Crates that own a frozen contract and therefore must carry a schema version.
const OWNERS: &[&str] = &[
    "pr-core",
    "pr-intelligence",
    "pr-security",
    "pr-catalog",
    "pr-json",
];

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("repo root")
        .to_path_buf()
}

fn source_of(krate: &str, file: &str) -> String {
    let p = root().join("crates").join(krate).join(file);
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))
}

/// The doc comment immediately above `pub struct/enum <name>`, with the `///` stripped.
fn doc_comment_for(src: &str, name: &str) -> Option<String> {
    let mut needle = None;
    'outer: for kind in [
        "pub struct ",
        "pub enum ",
        "pub type ",
        "pub fn ",
        "pub const ",
    ] {
        let pat = format!("{kind}{name}");
        // Every occurrence, not the first: `TaskScopeHandle` appears above `TaskScope`, and
        // stopping at the first match found the wrong item and then gave up.
        let mut from = 0;
        while let Some(rel) = src[from..].find(&pat) {
            let i = from + rel;
            let after = src[i + pat.len()..].chars().next().unwrap_or(' ');
            if !after.is_alphanumeric() && after != '_' {
                needle = Some(i);
                break 'outer;
            }
            from = i + pat.len();
        }
    }
    let at = needle?;
    let before = &src[..at];
    let mut doc: Vec<&str> = Vec::new();
    for line in before.lines().rev() {
        let t = line.trim_start();
        if t.starts_with("///") {
            doc.push(t.trim_start_matches("///"));
        } else if t.starts_with("#[") || t.is_empty() {
            // Attributes and blank lines between the doc and the item are fine.
            if t.is_empty() && !doc.is_empty() {
                break;
            }
        } else {
            break;
        }
    }
    doc.reverse();
    Some(doc.join("\n"))
}

#[test]
fn all_twelve_contracts_exist_where_they_are_declared_to() {
    for (name, krate, file) in CONTRACTS {
        let src = source_of(krate, file);
        assert!(
            doc_comment_for(&src, name).is_some(),
            "{name} is not declared in {krate}/{file}"
        );
    }
    assert_eq!(CONTRACTS.len(), 12, "V-J02 names twelve contracts");
}

#[test]
fn every_contract_has_a_doc_test_that_constructs_it() {
    // "Has a doc comment" is not the bar. The bar is a fenced example that the doc-test runner
    // compiles, and one that mentions the type rather than gesturing at it — an example that
    // only prints a constant would compile forever while the type changed underneath it.
    let mut without = Vec::new();
    for (name, krate, file) in CONTRACTS {
        let src = source_of(krate, file);
        let doc = doc_comment_for(&src, name).unwrap_or_default();
        let fenced = doc.matches("```").count() >= 2;
        // `ignore` and `no_run` doc tests are not run, so they cannot show drift.
        let runnable = fenced && !doc.contains("```ignore") && !doc.contains("```no_run");
        let mentions = doc
            .split("```")
            .nth(1)
            .is_some_and(|example| example.contains(name));
        if !(runnable && mentions) {
            without.push(format!(
                "{name} ({krate}/{file}): fenced={fenced} runnable={runnable} \
                 mentions-the-type={mentions}"
            ));
        }
    }
    assert!(
        without.is_empty(),
        "these contracts have no runnable doc test that uses them: {without:#?}"
    );
}

#[test]
fn every_owning_crate_declares_a_schema_version() {
    // §31.4: a frozen contract without a version is a contract that can only be changed by
    // breaking someone silently.
    for krate in OWNERS {
        let src = source_of(krate, "src/contract.rs");
        assert!(
            src.contains("pub const SCHEMA_VERSION: u32"),
            "{krate} has no SCHEMA_VERSION in its contract module"
        );
        assert!(
            doc_comment_for(&src, "SCHEMA_VERSION").is_some_and(|d| d.contains("```")),
            "{krate}'s SCHEMA_VERSION has no doc test pinning its value"
        );
    }
}

#[test]
fn the_owned_lists_add_up_to_exactly_the_twelve() {
    // Each crate publishes what it owns. Together they must be the twelve and nothing else --
    // a type that appears in no `OWNED` list is a type nobody has taken responsibility for.
    let mut owned: BTreeSet<&str> = BTreeSet::new();
    for n in pr_core::contract::OWNED {
        assert!(owned.insert(n), "{n} is claimed twice");
    }
    for n in pr_intelligence::contract::OWNED {
        assert!(owned.insert(n), "{n} is claimed twice");
    }
    for n in pr_security::contract::OWNED {
        assert!(owned.insert(n), "{n} is claimed twice");
    }
    for n in pr_catalog::contract::OWNED {
        assert!(owned.insert(n), "{n} is claimed twice");
    }
    for n in pr_json::contract::OWNED {
        assert!(owned.insert(n), "{n} is claimed twice");
    }
    let declared: BTreeSet<&str> = CONTRACTS.iter().map(|(n, _, _)| *n).collect();
    assert_eq!(
        owned, declared,
        "the crates' OWNED lists and V-J02's twelve do not agree"
    );
}

#[test]
fn the_contracts_compile_against_each_other() {
    // §31.4 points the dependency arrow into the contracts. The cheapest way to notice that a
    // change broke that is to actually build one of each here, in the crate that links all of
    // them -- which is what `prc` is.
    use bytes::Bytes;

    let req = pr_core::CommandRequest::new(
        vec![Bytes::from_static(b"GET"), Bytes::from_static(b"k")],
        pr_core::RequestOrigin::User,
        pr_core::Effects::read(),
    );
    let record = pr_core::contract::ResultRecord::new(1, req.clone());
    assert_eq!(record.id, 1);
    assert!(!record.retention.is_present());

    let scope = pr_intelligence::ObservationScope::new("p", "s", 0);
    let completion = pr_intelligence::contract::CompletionRequest {
        buffer: b"GET ".to_vec(),
        cursor_bytes: 4,
        buffer_revision: 1,
        scope,
        trigger: pr_intelligence::contract::Trigger::Explicit,
        max_candidates: 128,
    };
    assert!(!completion.trigger.debounces());

    let spec = pr_catalog::contract::LocalCommandSpec::unknown("X.Y");
    assert!(!spec.approval.policy_may_satisfy());

    let text = pr_core::SafeText::from_bytes(Bytes::from_static(b"\x1b[2J"));
    assert!(text.was_escaped());

    // And the versions are all readable from one place.
    for v in [
        pr_core::contract::SCHEMA_VERSION,
        pr_intelligence::contract::SCHEMA_VERSION,
        pr_security::contract::SCHEMA_VERSION,
        pr_catalog::contract::SCHEMA_VERSION,
        pr_json::contract::SCHEMA_VERSION,
    ] {
        assert!(v >= 1);
    }
}

#[test]
fn the_written_contract_document_lists_the_same_twelve() {
    // `docs/contracts/` is what a person reads. A document that has drifted from the code is
    // worse than none: it is read with the same confidence and is wrong.
    let doc = std::fs::read_to_string(root().join("docs/contracts/README.md"))
        .expect("docs/contracts/README.md");
    for (name, krate, _) in CONTRACTS {
        assert!(doc.contains(name), "docs/contracts does not mention {name}");
        assert!(
            doc.contains(krate),
            "docs/contracts does not mention {krate}"
        );
    }
    // And it must not describe something that no longer exists.
    for line in doc.lines().filter(|l| l.starts_with("| `")) {
        let Some(name) = line.split('`').nth(1) else {
            continue;
        };
        assert!(
            CONTRACTS.iter().any(|(n, _, _)| *n == name),
            "docs/contracts lists {name}, which is not one of V-J02's twelve"
        );
    }
}
