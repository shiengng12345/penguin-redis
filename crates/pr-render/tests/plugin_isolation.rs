//! V-D09 — an L2 plugin cannot take the host down with it (v2.1 §30.4, R45).
//!
//! §30.4 settles a question v2.0 got wrong:
//!
//! > 不存在「进程内加载原生动态库」的级别：Rust 的 panic/abort/OOM 无法在同进程内可靠隔离。
//!
//! The promise that replaces it — 「崩溃/OOM/死循环只影响该插件」、「崩溃时退回 generic view，
//! **不影响已完成请求**」 — cannot be tested with a plugin that behaves. Every case here runs
//! `xtask/plugin-fixtures`, which fails on purpose in each of the ways a real plugin
//! eventually will and in the ways a hostile one would choose.
//!
//! Two properties are asserted after *every* failure, because they are the actual promise:
//!
//! 1. the host is still running and still able to render (the process boundary held), and
//! 2. a result the user already has is unchanged (a rendering preference must never be able
//!    to lose data).

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, missing_docs)]

use pr_render::plugin::{Grant, Limits, PluginFailure, PluginHost, fixture_plugin};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("repo root")
        .to_path_buf()
}

/// A host with the fixture plugin granted in one mode.
fn host(mode: &str) -> PluginHost {
    let program = fixture_plugin(&root()).expect("build plugin-fixtures first");
    PluginHost::new().grant(Grant {
        name: format!("fixture-{mode}"),
        program,
        args: vec![mode.to_owned()],
    })
}

fn limits(ms: u64, max_output: usize) -> Limits {
    Limits {
        timeout: Duration::from_millis(ms),
        max_output_bytes: max_output,
    }
}

/// Render something with the built-in renderer, to prove the host is still usable afterwards.
fn the_host_still_works() {
    use pr_protocol::value::Value;
    let out = pr_render::render_generic(
        &Value::Simple(bytes::Bytes::from_static(b"still here")),
        pr_render::Theme::default(),
    );
    assert!(
        out.iter().any(|l| l.contains("still here")),
        "the generic view stopped working: {out:?}"
    );
}

// ---------------------------------------------------------------------------------------------
// The control
// ---------------------------------------------------------------------------------------------

#[test]
fn a_well_behaved_plugin_renders() {
    // Without this, a host that failed everything would pass every other test in the file.
    let h = host("ok");
    let input = "{\"value\":\"hello\"}";
    let r = h
        .call("fixture-ok", input, limits(5_000, 64 * 1024))
        .expect("the control plugin should succeed");
    assert_eq!(r.lines.len(), 2);
    assert_eq!(r.lines[0].as_display(), "rendered by the plugin");
    // And the input really reached it, rather than the plugin printing a constant.
    assert_eq!(
        r.lines[1].as_display(),
        format!("input was {} bytes", input.len())
    );
}

// ---------------------------------------------------------------------------------------------
// 崩溃 / OOM / 死循环只影响该插件
// ---------------------------------------------------------------------------------------------

#[test]
fn a_panicking_plugin_leaves_the_host_running() {
    let h = host("panic");
    let err = h
        .call("fixture-panic", "{}", limits(5_000, 64 * 1024))
        .expect_err("a panicking plugin must not succeed");
    assert!(
        matches!(err, PluginFailure::Crashed { .. }),
        "expected a crash, got {err:?}"
    );
    assert!(
        err.message("fixture-panic")
            .contains("Your results are unaffected"),
        "the message must say what did *not* happen: {}",
        err.message("fixture-panic")
    );
    the_host_still_works();
}

#[test]
fn an_aborting_plugin_leaves_the_host_running() {
    // `abort` skips unwinding entirely — SIGABRT on Unix. A host that only coped with panics
    // would miss this, and native in-process plugins would have taken the host with them,
    // which is the whole reason §30.4 removed that level.
    let h = host("abort");
    let err = h
        .call("fixture-abort", "{}", limits(5_000, 64 * 1024))
        .expect_err("an aborting plugin must not succeed");
    let PluginFailure::Crashed { status } = &err else {
        panic!("expected a crash, got {err:?}");
    };
    #[cfg(unix)]
    assert!(
        status.contains("signal"),
        "a signal death should be named as one: {status}"
    );
    #[cfg(not(unix))]
    assert!(!status.is_empty());
    the_host_still_works();
}

#[test]
fn a_hanging_plugin_is_killed_at_the_timeout_rather_than_waited_for() {
    let h = host("hang");
    let started = Instant::now();
    let err = h
        .call("fixture-hang", "{}", limits(300, 64 * 1024))
        .expect_err("a hang must not succeed");
    let elapsed = started.elapsed();
    assert_eq!(
        err,
        PluginFailure::TimedOut {
            after: Duration::from_millis(300)
        }
    );
    // The timeout is only a timeout if it bounds the wait. A generous ceiling, because a
    // loaded CI runner is slow, not broken.
    assert!(
        elapsed < Duration::from_secs(5),
        "waited {elapsed:?} for a 300 ms timeout"
    );
    the_host_still_works();
}

#[test]
fn a_flooding_plugin_is_truncated_and_the_host_does_not_grow_to_match() {
    // The failure mode that kills a naive host: read to end of stream, and the *host* is what
    // dies, not the plugin. The plugin writes 64 KiB at a time, forever.
    let h = host("flood");
    let before = pr_core::mem::rss_bytes();
    let err = h
        .call("fixture-flood", "{}", limits(5_000, 256 * 1024))
        .expect_err("an unbounded flood must not succeed");
    assert_eq!(
        err,
        PluginFailure::OutputTooLarge { limit: 256 * 1024 },
        "expected the output budget to stop it, got {err:?}"
    );
    if let (Some(a), Some(b)) = (pr_core::mem::rss_bytes(), before) {
        let grew = a as i64 - b as i64;
        assert!(
            grew < 64 * 1024 * 1024,
            "the host grew by {grew} bytes while refusing an unbounded plugin"
        );
    }
    the_host_still_works();
}

#[test]
#[cfg_attr(
    windows,
    ignore = "Windows commit-charge behaviour makes a deliberate OOM unreliable to trigger"
)]
fn an_out_of_memory_plugin_is_the_one_that_dies() {
    // §30.4's strongest claim. The plugin allocates and touches memory until the OS stops it;
    // the host must be unaffected, and must report something an operator can act on.
    //
    // Given a generous timeout: on a machine with a lot of free memory and swap this can take
    // a while to reach the limit, and being killed for the timeout instead is an equally
    // acceptable outcome — both mean the host bounded it. The assertion is on the host, not
    // on which limit fired.
    let h = host("oom");
    let before = pr_core::mem::rss_bytes();
    let err = h
        .call("fixture-oom", "{}", limits(10_000, 64 * 1024))
        .expect_err("a plugin that exhausts memory must not succeed");
    assert!(
        matches!(
            err,
            PluginFailure::Crashed { .. } | PluginFailure::TimedOut { .. }
        ),
        "expected the plugin to be stopped, got {err:?}"
    );
    if let (Some(a), Some(b)) = (pr_core::mem::rss_bytes(), before) {
        let grew = a as i64 - b as i64;
        assert!(
            grew < 64 * 1024 * 1024,
            "the host grew by {grew} bytes while a plugin exhausted memory"
        );
    }
    the_host_still_works();
}

#[test]
fn a_crash_does_not_disturb_a_result_the_user_already_has() {
    // 「不影响已完成请求」, made concrete. A rendering preference must never be able to lose
    // data the user already has, so the successful rendering is taken *first* and checked
    // again afterwards.
    let good = host("ok");
    let before = good
        .call("fixture-ok", "{\"a\":1}", limits(5_000, 64 * 1024))
        .expect("the control");

    for mode in ["panic", "abort", "garbage", "wrong-shape"] {
        let h = host(mode);
        let _ = h.call(&format!("fixture-{mode}"), "{}", limits(2_000, 64 * 1024));
        the_host_still_works();
    }

    let after = good
        .call("fixture-ok", "{\"a\":1}", limits(5_000, 64 * 1024))
        .expect("the control still works after four failures");
    assert_eq!(
        before, after,
        "a result changed across a series of plugin failures"
    );
}

#[test]
fn repeated_crashes_do_not_leak_processes() {
    // A renderer is called often. A host that forgets to reap leaves one zombie per call, and
    // the symptom appears much later as "cannot fork". Counting children portably is awkward,
    // so this asserts the thing that would break: after fifty crashes, the host can still
    // spawn and still succeed.
    for _ in 0..50 {
        let h = host("panic");
        let err = h
            .call("fixture-panic", "{}", limits(2_000, 4096))
            .unwrap_err();
        assert!(matches!(err, PluginFailure::Crashed { .. }));
    }
    let h = host("ok");
    h.call("fixture-ok", "{}", limits(5_000, 64 * 1024))
        .expect("the host can still spawn after fifty crashes");
    the_host_still_works();
}

// ---------------------------------------------------------------------------------------------
// 默认 deny，逐项授权
// ---------------------------------------------------------------------------------------------

#[test]
fn a_plugin_that_is_not_granted_is_never_spawned() {
    // Default deny has to mean "does not run", not "runs and is ignored". Checked by asking
    // for a name that is not granted while a *different* one is: a host that spawned whatever
    // it was given would have run the crashing fixture here.
    let h = host("panic");
    assert!(h.is_granted("fixture-panic"));
    assert!(!h.is_granted("fixture-ok"));

    let err = h
        .call("fixture-ok", "{}", limits(5_000, 64 * 1024))
        .expect_err("an ungranted plugin must not run");
    assert_eq!(err, PluginFailure::NotGranted("fixture-ok".to_owned()));
    assert!(
        err.message("fixture-ok").contains("denied by default"),
        "{}",
        err.message("fixture-ok")
    );

    // And an empty host grants nothing at all, which is the state a user starts in.
    let empty = PluginHost::new();
    assert!(!empty.is_granted("anything"));
    assert!(matches!(
        empty.call("anything", "{}", Limits::default()),
        Err(PluginFailure::NotGranted(_))
    ));
}

// ---------------------------------------------------------------------------------------------
// 不获得 credential store、网络或文件系统能力
// ---------------------------------------------------------------------------------------------

#[test]
fn a_plugin_gets_a_built_environment_rather_than_an_inherited_one() {
    // Checked from outside the host: the fixture reports the variable names it was given, so
    // this tests what the child actually received rather than what the host's code intends.
    //
    // The canaries are variables the test process demonstrably has — `CARGO_MANIFEST_DIR` is
    // set by cargo, `PATH` by the shell. Asserting they are present in the parent first is
    // what stops this being a test that passes because it checked nothing.
    //
    // Inheriting and then removing known-bad names is the wrong default: it fails open for
    // every variable nobody thought of, and a credential path is exactly that kind of variable.
    // So the assertion is that the child's environment is *built*, not that one name is gone.
    let canaries: Vec<String> = ["CARGO_MANIFEST_DIR", "PATH"]
        .iter()
        .filter(|k| std::env::var_os(k).is_some())
        .map(|k| (*k).to_owned())
        .collect();
    assert!(
        canaries.len() == 2,
        "the test process does not have the variables this test relies on: {canaries:?}"
    );

    let h = host("echo-env");
    let r = h
        .call("fixture-echo-env", "{}", limits(5_000, 64 * 1024))
        .expect("the env reporter");
    let names = r.lines[0].as_display().to_owned();

    for leaked in &canaries {
        assert!(
            !names.contains(leaked.as_str()),
            "{leaked} reached the plugin, so the environment was inherited: {names}"
        );
    }
    for leaked in ["HOME", "USER", "SSH_AUTH_SOCK", "AWS_", "REDIS_", "CARGO_"] {
        assert!(
            !names.contains(leaked),
            "{leaked} reached the plugin: {names}"
        );
    }
    // What it *does* get, so the test would notice an environment that became empty by
    // accident rather than by design.
    assert!(
        names.contains("PENGUIN_PLUGIN_API"),
        "the plugin API marker is missing: {names}"
    );
}

#[test]
fn rendered_data_never_becomes_an_argument() {
    // A value that becomes an argv element is an injection waiting for the right value. The
    // grant fixes the arguments; the data goes in on stdin, where it is bytes.
    let program = fixture_plugin(&root()).unwrap();
    let g = Grant {
        name: "fixed".into(),
        program,
        args: vec!["ok".into()],
    };
    assert_eq!(g.args, vec!["ok".to_owned()]);

    let h = PluginHost::new().grant(g);
    // A payload that would be catastrophic as an argument is just input.
    let hostile = "{\"value\":\"--mode=abort; rm -rf /\"}";
    let r = h
        .call("fixted-nope", hostile, limits(5_000, 64 * 1024))
        .unwrap_err();
    assert!(matches!(r, PluginFailure::NotGranted(_)));

    let r = h
        .call("fixed", hostile, limits(5_000, 64 * 1024))
        .expect("the plugin still runs in its granted mode");
    assert_eq!(
        r.lines[1].as_display(),
        format!("input was {} bytes", hostile.len()),
        "the payload arrived as input, not as a mode"
    );
}

// ---------------------------------------------------------------------------------------------
// 参数与返回按 SafeText 处理
// ---------------------------------------------------------------------------------------------

#[test]
fn terminal_escapes_from_a_plugin_are_neutralised_before_they_are_lines() {
    // §23.6: a plugin is no more trusted than a server. The fixture emits an OSC title-set, a
    // BEL and a screen-clear — the three a hostile renderer reaches for first — correctly
    // encoded, so the output is *valid* and only its content is hostile. That is the case
    // that matters: malformed output is caught by the parser, well-formed output is not.
    let h = host("escapes-json");
    let r = h
        .call("fixture-escapes-json", "{}", limits(5_000, 64 * 1024))
        .expect("well-formed output is not a failure, whatever it contains");
    let line = &r.lines[0];
    assert!(
        line.was_escaped(),
        "the escapes passed through unmarked: {:?}",
        line.as_display()
    );
    for c in line.as_display().chars() {
        assert!(
            !pr_core::safetext::is_dangerous_char(c),
            "a dangerous character survived: {c:?} in {:?}",
            line.as_display()
        );
    }
    // The readable content is still there — neutralising is not deleting.
    assert!(line.as_display().contains("before"));
    assert!(line.as_display().contains("after"));
}

#[test]
fn raw_control_bytes_cannot_be_smuggled_through_the_output_parser() {
    // Found while writing the test above. A plugin that writes a bare ESC inside a JSON string
    // has produced invalid JSON, and the host must refuse it rather than repair it: repairing
    // would mean a path to the terminal that the JSON layer never inspected.
    let h = host("escapes-raw");
    let err = h
        .call("fixture-escapes-raw", "{}", limits(5_000, 64 * 1024))
        .expect_err("raw control bytes are not valid JSON");
    let PluginFailure::Malformed(why) = &err else {
        panic!("expected malformed output, got {err:?}");
    };
    assert!(
        why.contains("control character"),
        "the reason should name what was wrong: {why}"
    );
    the_host_still_works();
}

// ---------------------------------------------------------------------------------------------
// Output that is not a rendering
// ---------------------------------------------------------------------------------------------

#[test]
fn output_that_is_not_a_rendering_is_refused_by_name() {
    for (mode, expect) in [("garbage", "not JSON"), ("wrong-shape", "no `lines`")] {
        let h = host(mode);
        let err = h
            .call(&format!("fixture-{mode}"), "{}", limits(5_000, 64 * 1024))
            .expect_err("unusable output must not be a rendering");
        let PluginFailure::Malformed(why) = &err else {
            panic!("{mode} produced {err:?}");
        };
        assert!(why.contains(expect), "{mode}: {why}");
        the_host_still_works();
    }
}

#[test]
fn a_plugin_that_prints_a_rendering_and_then_fails_is_not_believed() {
    // It produced perfectly good output and then said it failed. Trusting the bytes over the
    // exit status would mean showing a rendering the plugin itself disowned.
    let h = host("output-then-fail");
    let err = h
        .call("fixture-output-then-fail", "{}", limits(5_000, 64 * 1024))
        .expect_err("a non-zero exit must not produce a rendering");
    let PluginFailure::Crashed { status } = &err else {
        panic!("expected a crash, got {err:?}");
    };
    assert!(
        status.contains('3'),
        "the exit code should be named: {status}"
    );
    the_host_still_works();
}

#[test]
fn a_slow_but_finite_plugin_is_not_mistaken_for_a_hang() {
    // The other half of the timeout test. Without it, a host that returned `TimedOut` for
    // everything would pass `a_hanging_plugin_is_killed_at_the_timeout`.
    let h = host("slow");
    let r = h
        .call("fixture-slow", "{}", limits(5_000, 64 * 1024))
        .expect("300 ms is not a hang");
    assert_eq!(r.lines[0].as_display(), "eventually");

    // And with a timeout below its runtime, the same plugin does time out — so the limit is
    // what decides, not the plugin.
    let err = h
        .call("fixture-slow", "{}", limits(50, 64 * 1024))
        .expect_err("a 50 ms budget must stop a 300 ms plugin");
    assert!(matches!(err, PluginFailure::TimedOut { .. }), "{err:?}");
}

#[test]
fn every_failure_tells_the_user_the_built_in_view_is_being_used() {
    // The user-facing half of "退回 generic view". A failure the user cannot interpret gets
    // blamed on Penguin, so every message says what is being shown instead.
    let program = fixture_plugin(&root()).unwrap();
    let failures = [
        PluginFailure::SpawnFailed("no such file".into()),
        PluginFailure::TimedOut {
            after: Duration::from_secs(2),
        },
        PluginFailure::Crashed {
            status: "signal 6".into(),
        },
        PluginFailure::OutputTooLarge { limit: 1024 },
        PluginFailure::Malformed("not JSON".into()),
    ];
    for f in &failures {
        let m = f.message("some-plugin");
        assert!(
            m.contains("built-in view") || m.contains("could not be started"),
            "{f:?} does not say what happens next: {m}"
        );
    }
    // And the deny message says why rather than only that.
    let m = PluginFailure::NotGranted("p".into()).message("p");
    assert!(
        m.contains("denied by default") && m.contains("granted one at a time"),
        "{m}"
    );
    assert!(program.exists());
}
