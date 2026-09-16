//! V-I03 — the project's licence is a real one (v2.1 §35.5, ADR-023).
//!
//! A repository with no licence is not "free to use": copyright's default is all rights
//! reserved, so without one nobody may legally distribute the binary, and a company cannot
//! legally install it. The workspace carried the placeholder `TBD-ADR-023` for a while, which
//! `cargo deny` tolerates only because these crates are `publish = false` — exactly the kind
//! of thing that survives to a release because nothing complained.
//!
//! These tests complain.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, missing_docs)]

const WORKSPACE_MANIFEST: &str = include_str!("../../../Cargo.toml");
const MIT: &str = include_str!("../../../LICENSE-MIT");
const APACHE: &str = include_str!("../../../LICENSE-APACHE");
const LICENSES_MD: &str = include_str!("../../../crates/pr-catalog/LICENSES.md");

fn declared_licence() -> String {
    WORKSPACE_MANIFEST
        .lines()
        .find_map(|l| l.trim().strip_prefix("license = "))
        .expect("the workspace declares a licence")
        .trim()
        .trim_matches('"')
        .to_owned()
}

#[test]
fn the_declared_licence_is_not_a_placeholder() {
    let l = declared_licence();
    assert!(
        !l.contains("TBD") && !l.contains("TODO") && !l.contains("UNLICENSED"),
        "the workspace licence is still a placeholder: {l:?}"
    );
    assert_eq!(l, "MIT OR Apache-2.0", "ADR-023's decision");
}

#[test]
fn the_declared_licence_is_a_well_formed_spdx_expression() {
    // Not a full SPDX parser — enough to catch a typo that would make every downstream
    // licence scanner report "unknown", which reads the same as "none".
    let l = declared_licence();
    let known = [
        "MIT",
        "Apache-2.0",
        "BSD-2-Clause",
        "BSD-3-Clause",
        "ISC",
        "AGPL-3.0-or-later",
        "GPL-3.0-or-later",
        "MPL-2.0",
    ];
    for token in l.split_whitespace() {
        if token == "OR" || token == "AND" || token == "WITH" {
            continue;
        }
        assert!(
            known.contains(&token),
            "{token:?} is not an SPDX identifier this project recognises; a typo here makes \
             every licence scanner report 'unknown', which reads the same as 'none'"
        );
    }
}

#[test]
fn both_licence_texts_are_present_and_are_the_real_thing() {
    // A dual licence with only one text is a dual licence in name only.
    assert!(
        MIT.contains("MIT License"),
        "LICENSE-MIT is not the MIT text"
    );
    assert!(MIT.contains("WITHOUT WARRANTY OF ANY KIND"));
    assert!(
        APACHE.contains("Apache License") && APACHE.contains("Version 2.0, January 2004"),
        "LICENSE-APACHE is not the Apache 2.0 text"
    );
    assert!(
        APACHE.contains("6. Trademarks."),
        "the Apache text is truncated"
    );
}

#[test]
fn neither_licence_text_still_has_its_fill_in_the_blank() {
    // The Apache appendix ships with `Copyright [yyyy] [name of copyright owner]`. Left in, it
    // is a licence that names nobody.
    assert!(
        !APACHE.contains("[yyyy]") && !APACHE.contains("[name of copyright owner]"),
        "LICENSE-APACHE still has its template placeholders"
    );
    assert!(APACHE.contains("Copyright 2026 Penguin Redis contributors"));
    assert!(MIT.contains("Copyright (c) 2026 Penguin Redis contributors"));
}

#[test]
fn the_catalog_licence_register_has_no_empty_conclusion() {
    // §35.5 / V-I03: "结论为空不得发布". Every source row must reach one of the three
    // conclusions, and the register is where that is recorded.
    for source in [
        "COMMAND DOCS",
        "Valkey 8.1.10",
        "src/commands/*.json",
        "CC-BY-SA-4.0",
    ] {
        assert!(
            LICENSES_MD.contains(source),
            "the register does not mention {source}"
        );
    }
    for conclusion in ["**可打包**", "**本仓独立撰写**", "不适用"] {
        assert!(
            LICENSES_MD.contains(conclusion),
            "the register never reaches the conclusion {conclusion}"
        );
    }
    // And the register says plainly that this is an engineering judgement rather than legal
    // advice, with a human review still outstanding. Losing that caveat would turn a recorded
    // decision into an unearned assurance.
    assert!(LICENSES_MD.contains("不是法律意见"));
    assert!(LICENSES_MD.contains("人工法务确认"));
}

#[test]
fn the_licence_is_declared_in_exactly_one_place() {
    // Changing the project's licence should be one edit, not a hunt through twenty crates —
    // which matters because this decision is explicitly reversible until Phase 7.
    let occurrences = WORKSPACE_MANIFEST
        .lines()
        .filter(|l| l.trim().starts_with("license = "))
        .count();
    assert_eq!(occurrences, 1, "the workspace declares its licence twice");

    // Every crate inherits it rather than restating it.
    for manifest in [
        include_str!("../../pr-core/Cargo.toml"),
        include_str!("../../pr-catalog/Cargo.toml"),
        include_str!("../../pr-terminal/Cargo.toml"),
        include_str!("../Cargo.toml"),
    ] {
        assert!(
            manifest.contains("license.workspace = true"),
            "a crate declares its own licence instead of inheriting: {}",
            manifest.lines().next().unwrap_or("")
        );
    }
}
