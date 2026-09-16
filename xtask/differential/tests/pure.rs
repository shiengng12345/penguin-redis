//! V-A02 — the parts of the harness that hold without a server.
//!
//! These run on every `cargo test`, so the case list and the committed report cannot drift
//! while the live job is red, queued, or quietly skipped.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, missing_docs)]

use differential::cases::{OutputExpectation, cases};
use std::path::{Path, PathBuf};

fn report_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("repo root")
        .join("tests/differential/report")
}

#[test]
fn the_five_cases_v_a02_names_are_all_present() {
    let ids: Vec<&str> = cases().iter().map(|c| c.id).collect();
    assert_eq!(ids, ["CMD-01", "CMD-02", "CMD-03", "CMD-04", "CMD-05"]);
}

#[test]
fn every_case_reads_state_back_afterwards() {
    // Layer 3 is the one §32.2 insists on, and a case with no probe silently has only three
    // layers while the report still says four.
    for c in cases() {
        assert!(
            !c.state_probes.is_empty(),
            "{} compares no final state, so layer 3 is vacuous for it",
            c.id
        );
        assert!(!c.seed.is_empty(), "{} has no initial state fixture", c.id);
    }
}

#[test]
fn a_permitted_output_difference_always_carries_a_reason() {
    // The escape hatch in layer 4 is only honest if it has to be argued for in advance.
    for c in cases() {
        match c.output {
            OutputExpectation::Identical => {}
            OutputExpectation::SameBytesDifferentExit { why }
            | OutputExpectation::Divergent { why } => {
                assert!(
                    why.len() > 30 && why.contains('§'),
                    "{}'s permitted difference does not cite what decides it: {why:?}",
                    c.id
                );
            }
        }
    }
}

#[test]
fn the_committed_report_covers_exactly_the_case_list() {
    // The report is an artifact, and an artifact that outlives the thing it describes is worse
    // than no artifact. `ci/check-differential.sh` re-runs and diffs; this catches the cheaper
    // failure of adding a case and forgetting to regenerate.
    let dir = report_dir();
    for c in cases() {
        let p = dir.join(format!("{}.json", c.id));
        assert!(p.exists(), "no committed report for {}", c.id);
        let v: serde_json::Value = serde_json::from_slice(&std::fs::read(&p).unwrap()).unwrap();
        assert_eq!(v["case_id"], c.id);
        assert_eq!(
            v["execution_status"], "PASS",
            "{} is committed as a failure",
            c.id
        );
        // Appendix B.2's marker must be gone: the template ships with
        // `NOT_RUN_IN_THIS_PLAN` precisely so that an unrun case cannot look like a passing
        // one, and replacing it is the whole deliverable.
        assert_ne!(v["execution_status"], "NOT_RUN_IN_THIS_PLAN");
        for field in [
            "baseline_cli",
            "server_image_digest",
            "protocol",
            "initial_state_fixture",
            "request",
            "expected_reply",
        ] {
            assert!(!v[field].is_null(), "{} is missing B.2's {field}", c.id);
        }
    }

    let extra: Vec<String> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| {
            let name = e.ok()?.file_name().to_string_lossy().into_owned();
            let stem = name.strip_suffix(".json")?.to_owned();
            cases().iter().all(|c| c.id != stem).then_some(stem)
        })
        .collect();
    assert!(extra.is_empty(), "reports with no case: {extra:?}");
}

#[test]
fn the_report_records_what_each_side_actually_did() {
    // A verdict with no evidence behind it is the same fiction B.2's marker exists to prevent.
    let dir = report_dir();
    for c in cases() {
        let v: serde_json::Value =
            serde_json::from_slice(&std::fs::read(dir.join(format!("{}.json", c.id))).unwrap())
                .unwrap();
        for side in ["baseline_observed", "penguin_observed"] {
            let o = &v[side];
            assert!(
                o["sent"].as_str().is_some_and(|s| s.contains(c.request[0])),
                "{} {side} does not show the command going out: {:?}",
                c.id,
                o["sent"]
            );
            assert!(
                o["received"].as_str().is_some_and(|s| !s.is_empty()),
                "{} {side} recorded no reply bytes",
                c.id
            );
            assert!(
                o["final_state"].as_array().is_some_and(|a| !a.is_empty()),
                "{} {side} read no state back",
                c.id
            );
        }
    }
}

#[test]
fn the_digest_the_harness_runs_matches_the_pinned_baseline() {
    // The baseline is only pinned if the harness uses the pin. Reading it from the report
    // rather than from the source keeps the check honest about what actually ran.
    let manifest = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("repo root")
        .join("compatibility/manifest.toml");
    let manifest = std::fs::read_to_string(manifest).unwrap();
    let pinned = manifest
        .split("[baseline_cli]")
        .nth(1)
        .and_then(|s| s.lines().find(|l| l.trim().starts_with("digest = ")))
        .and_then(|l| l.split('"').nth(1))
        .expect("baseline_cli digest");

    let v: serde_json::Value =
        serde_json::from_slice(&std::fs::read(report_dir().join("CMD-01.json")).unwrap()).unwrap();
    let used = v["server_image_digest"].as_str().unwrap();
    assert!(
        used.ends_with(pinned),
        "the harness ran {used} but the manifest pins {pinned}"
    );
}
