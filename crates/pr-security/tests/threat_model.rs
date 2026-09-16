//! V-J03 — every trust boundary has an acceptance case (v2.1 §23, §33).
//!
//! The pass criterion is "无未覆盖边界" — no uncovered boundary. That is a claim, and a claim
//! in a document is worth nothing once the document and the code drift, so it is checked here
//! instead.
//!
//! Three checks, and the second and third are the ones that earn their keep:
//!
//! 1. every boundary names at least one SEC case
//! 2. **every SEC case is claimed by at least one boundary** — a case nobody claims is a test
//!    that passes without anyone knowing what it was protecting
//! 3. **every file a boundary cites actually exists** — a coverage claim pointing at a path
//!    that was renamed is a coverage claim that has quietly become false

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, missing_docs)]

use std::collections::BTreeSet;
use std::path::Path;

const REGISTER: &str = include_str!("../../../docs/threat-model/boundaries.toml");

/// Every SEC case v2.1 §33 defines.
///
/// Listed here rather than derived, so that a new case in the blueprint fails this file until
/// somebody says which boundary it protects.
const SEC_CASES: &[&str] = &[
    "SEC-01", "SEC-02", "SEC-03", "SEC-04", "SEC-05", "SEC-06", "SEC-07", "SEC-08", "SEC-09",
    "SEC-10", "SEC-11",
];

/// The boundaries V-J03 requires to be listed.
///
/// From the issue: 输入 / parser / 配置 / 连接 / 错误 / trace / history / clipboard / export /
/// crash / suggestion / metadata / plugin / MCP.
const REQUIRED_BOUNDARIES: &[&str] = &[
    "input",
    "parser",
    "config",
    "connection",
    "errors",
    "trace",
    "history",
    "clipboard",
    "export",
    "crash",
    "suggestion",
    "metadata",
    "plugin",
    "mcp",
];

struct Boundary {
    id: String,
    name: String,
    why: String,
    cases: Vec<String>,
    covered_by: Vec<String>,
}

fn boundaries() -> Vec<Boundary> {
    let t: toml::Table = REGISTER.parse().expect("the register is valid TOML");
    t["boundary"]
        .as_array()
        .expect("[[boundary]] entries")
        .iter()
        .map(|b| {
            let s = |k: &str| {
                b.get(k)
                    .and_then(toml::Value::as_str)
                    .unwrap_or_else(|| panic!("a boundary is missing {k}"))
                    .to_owned()
            };
            let list = |k: &str| {
                b.get(k)
                    .and_then(toml::Value::as_array)
                    .unwrap_or_else(|| panic!("{} is missing {k}", s("id")))
                    .iter()
                    .filter_map(|v| v.as_str().map(str::to_owned))
                    .collect()
            };
            Boundary {
                id: s("id"),
                name: s("name"),
                why: s("why"),
                cases: list("cases"),
                covered_by: list("covered_by"),
            }
        })
        .collect()
}

fn repo_root() -> &'static Path {
    // `CARGO_MANIFEST_DIR` is `crates/pr-security`.
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("the repository root")
}

#[test]
fn every_boundary_the_issue_names_is_in_the_register() {
    let ids: BTreeSet<String> = boundaries().into_iter().map(|b| b.id).collect();
    for required in REQUIRED_BOUNDARIES {
        assert!(
            ids.contains(*required),
            "V-J03 names the {required} boundary and the register does not have it"
        );
    }
}

#[test]
fn no_boundary_is_uncovered() {
    // The pass criterion, checked rather than asserted in prose.
    for b in boundaries() {
        assert!(
            !b.cases.is_empty(),
            "the {} boundary ({}) has no acceptance case",
            b.id,
            b.name
        );
        for c in &b.cases {
            assert!(
                SEC_CASES.contains(&c.as_str()),
                "{} cites {c}, which is not a SEC case in §33",
                b.id
            );
        }
    }
}

#[test]
fn no_sec_case_is_unclaimed() {
    // The direction that is easy to forget. A case nobody claims is a test that passes without
    // anyone knowing what it was protecting — and when it starts failing, nobody knows what
    // broke either.
    let claimed: BTreeSet<String> = boundaries()
        .into_iter()
        .flat_map(|b| b.cases.into_iter())
        .collect();
    let orphans: Vec<&&str> = SEC_CASES
        .iter()
        .filter(|c| !claimed.contains(**c))
        .collect();
    assert!(
        orphans.is_empty(),
        "these SEC cases are not claimed by any boundary, so nothing says what they protect: \
         {orphans:?}"
    );
}

#[test]
fn every_cited_file_exists() {
    // A coverage claim pointing at a path that was renamed is a coverage claim that has
    // quietly become false, and the register would keep saying the boundary is covered.
    let root = repo_root();
    for b in boundaries() {
        assert!(
            !b.covered_by.is_empty(),
            "{} cites no implementation or test",
            b.id
        );
        for path in &b.covered_by {
            let p = root.join(path);
            assert!(
                p.exists(),
                "{} cites {path}, which does not exist (looked at {})",
                b.id,
                p.display()
            );
        }
    }
}

#[test]
fn every_boundary_says_what_crossing_it_would_cost() {
    // "It is a boundary" is not a reason. The `why` is what tells a reader whether the
    // coverage is the right coverage, and a one-liner cannot do that.
    for b in boundaries() {
        assert!(
            b.why.len() > 80,
            "{}'s rationale is too short to be one: {:?}",
            b.id,
            b.why
        );
        assert!(
            !b.why.to_lowercase().contains("is a trust boundary"),
            "{} explains itself by restating its own name",
            b.id
        );
    }
}

#[test]
fn the_boundaries_are_distinct() {
    // Two entries for the same thing would make the coverage count look better than it is.
    let all = boundaries();
    let mut ids: Vec<&str> = all.iter().map(|b| b.id.as_str()).collect();
    let before = ids.len();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(before, ids.len(), "a boundary id is listed twice");

    let mut names: Vec<&str> = all.iter().map(|b| b.name.as_str()).collect();
    names.sort_unstable();
    names.dedup();
    assert_eq!(before, names.len(), "two boundaries share a name");
}

#[test]
fn the_display_boundary_is_covered_by_a_type_and_not_only_by_a_rule() {
    // §23.5 requires every server-originated string to become `SafeText` before it can reach a
    // renderer, and the only durable enforcement is one the type system applies. The register
    // must point at that, not at a convention.
    let errors = boundaries()
        .into_iter()
        .find(|b| b.id == "errors")
        .expect("the errors boundary");
    assert!(
        errors.covered_by.iter().any(|p| p.contains("safetext")),
        "the display boundary must cite the type that enforces it: {:?}",
        errors.covered_by
    );
}

#[test]
fn the_suggestion_boundary_is_present_because_it_is_the_easiest_to_miss() {
    // §23.7 calls it a *new* trust boundary. It is the one that does not look like one: a
    // completion menu built from key names the server returned is the server influencing what
    // the user sends next.
    let s = boundaries()
        .into_iter()
        .find(|b| b.id == "suggestion")
        .expect("§23.7's boundary");
    assert!(!s.cases.is_empty());
    assert!(
        s.covered_by.iter().any(|p| p.contains("zero_send")),
        "the zero-send invariant is what keeps this boundary one-way: {:?}",
        s.covered_by
    );
}
