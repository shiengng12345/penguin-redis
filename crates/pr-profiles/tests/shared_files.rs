//! V-G05 — shared-file concurrency on all three platforms (v2.1 §30.2, §12.10, R20, R39,
//! WIN-03, LIFE-03).
//!
//! LIFE-03 is 「多终端同时改 profile | 无丢更新、无凭证误绑定」, and the first half of that is
//! only testable with real **processes**. An advisory lock is a promise between processes;
//! threads inside one process share it on most platforms, so a threaded test would pass
//! against an implementation that does not work at all.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    missing_docs,
    clippy::items_after_statements,
    clippy::cast_possible_truncation,
    // The temp files are dotfiles whose names already contain dots; `extension()` would match
    // a different set, and on Windows it compares case-insensitively.
    clippy::case_sensitive_file_extension_comparisons
)]

use pr_profiles::perms;
use pr_profiles::shared::{SharedFile, SharedFileError};
use std::path::{Path, PathBuf};
use std::process::Command;

fn scratch(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!(
        "penguin-vg05-{name}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn worker_binary() -> PathBuf {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("repo root");
    let exe = if cfg!(windows) {
        "shared_writer.exe"
    } else {
        "shared_writer"
    };
    for profile in ["debug", "release"] {
        let p = root.join("target").join(profile).join("examples").join(exe);
        if p.exists() {
            return p;
        }
    }
    // Build it on demand: cargo builds a dependency's lib, not its examples, and a test that
    // fails on a clean checkout with "file not found" teaches people to ignore it.
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_owned());
    let out = Command::new(cargo)
        .args(["build", "-p", "pr-profiles", "--example", "shared_writer"])
        .current_dir(root)
        .output()
        .expect("build the worker");
    assert!(
        out.status.success(),
        "building shared_writer failed:\n{}",
        String::from_utf8_lossy(&out.stderr)
    );
    for profile in ["debug", "release"] {
        let p = root.join("target").join(profile).join("examples").join(exe);
        if p.exists() {
            return p;
        }
    }
    panic!("shared_writer built but is not where it should be");
}

// ---------------------------------------------------------------------------------------------
// LIFE-03 — no lost updates
// ---------------------------------------------------------------------------------------------

#[test]
fn concurrent_processes_lose_no_updates() {
    // Eight processes, twenty lines each. If the read-modify-write ever interleaves, the file
    // ends up with fewer than 160 lines and the test says exactly how many went missing.
    let dir = scratch("lost-updates");
    let path = dir.join("profiles.toml");
    let worker = worker_binary();

    const WRITERS: usize = 8;
    const ROUNDS: usize = 20;

    let mut children = Vec::new();
    for w in 0..WRITERS {
        children.push(
            Command::new(&worker)
                .arg(&path)
                .arg(format!("w{w}"))
                .arg(ROUNDS.to_string())
                .spawn()
                .expect("spawn writer"),
        );
    }
    for mut c in children {
        let status = c.wait().expect("wait");
        assert!(status.success(), "a writer failed: {status}");
    }

    let final_state = SharedFile::new(&path).read().unwrap();
    let lines: Vec<&str> = final_state.body.lines().filter(|l| !l.is_empty()).collect();
    assert_eq!(
        lines.len(),
        WRITERS * ROUNDS,
        "{} of {} updates were lost",
        WRITERS * ROUNDS - lines.len(),
        WRITERS * ROUNDS
    );
    // Every writer's lines are all there, and each appears once.
    for w in 0..WRITERS {
        for i in 0..ROUNDS {
            let tag = format!("w{w}-{i}");
            assert_eq!(
                lines.iter().filter(|l| **l == tag).count(),
                1,
                "{tag} appears the wrong number of times"
            );
        }
    }
    // The version advanced once per write, which is what makes a stale writer detectable.
    assert_eq!(final_state.version as usize, WRITERS * ROUNDS);
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_stale_writer_is_refused_and_its_edit_is_kept() {
    // The case the lock alone cannot catch: a process read, thought about it, and is now
    // writing something computed from a version that has moved on.
    let dir = scratch("stale");
    let path = dir.join("profiles.toml");
    let file = SharedFile::new(&path);

    let v1 = file.update(|_| "first\n".to_owned()).unwrap();
    assert_eq!(v1, 1);
    // Somebody else writes in between.
    file.update(|b| format!("{b}second\n")).unwrap();

    let err = file
        .write_if_unchanged(v1, "a careful edit based on v1\n")
        .unwrap_err();
    let SharedFileError::Conflict {
        expected,
        found,
        conflict,
        ..
    } = &err
    else {
        panic!("expected a conflict, got {err:?}");
    };
    assert_eq!((*expected, *found), (1, 2));

    // Refused -- and not discarded. §12.10 asks for `conflicts/` by name because refusing to
    // write and then throwing the edit away is still losing it.
    assert!(conflict.exists(), "the rejected edit was not preserved");
    let kept = std::fs::read_to_string(conflict).unwrap();
    assert!(kept.contains("a careful edit based on v1"));

    // And the file on disk still has the other writer's work.
    let now = file.read().unwrap();
    assert_eq!(now.version, 2);
    assert!(now.body.contains("second"));
    assert!(!now.body.contains("careful edit"));

    // The message tells the user where to look rather than only that something went wrong.
    let msg = err.to_string();
    assert!(msg.contains("was not discarded"), "{msg}");
    assert!(msg.contains("conflicts"), "{msg}");
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn a_file_written_by_something_else_is_not_overwritten_blind() {
    // A file with no version header did not come from us. Guessing a version for it is how
    // the protocol stops protecting anything.
    let dir = scratch("foreign");
    let path = dir.join("profiles.toml");
    std::fs::write(&path, "[profile]\nname = \"written by hand\"\n").unwrap();

    let file = SharedFile::new(&path);
    let err = file.read().unwrap_err();
    assert!(matches!(err, SharedFileError::NoVersion(_)), "{err:?}");
    assert!(file.update(str::to_owned).is_err());

    // The hand-written file is untouched.
    let after = std::fs::read_to_string(&path).unwrap();
    assert!(after.contains("written by hand"));
    let _ = std::fs::remove_dir_all(&dir);
}

// ---------------------------------------------------------------------------------------------
// Atomic replacement
// ---------------------------------------------------------------------------------------------

#[test]
fn the_destination_is_never_written_in_place() {
    // A reader that opens the file at any moment sees a whole version, old or new. That holds
    // because the destination is only ever replaced by rename -- if it were opened for writing
    // and truncated, there would be a window where it is empty.
    //
    // This is asserted **directly**, by holding a reader open across the update, rather than by
    // comparing an identity number before and after. Two earlier attempts at the indirect
    // version were both wrong on Windows and both looked right:
    //
    //   - creation time: NTFS file system tunnelling deliberately restores the original
    //     creation timestamp when a name is recreated within ~15 seconds of being removed,
    //     which is exactly what an atomic replace does. The test then reported "modified in
    //     place" about the operation it exists to prove.
    //   - `Metadata::file_index`: still unstable (`windows_by_handle`), so it does not compile.
    //
    // Holding the reader open is also the property the protocol actually promises, instead of
    // a proxy for it. A handle opened before the replace keeps reading the version it opened;
    // an in-place rewrite would show the new bytes, or an empty window, through that handle.
    let dir = scratch("atomic");
    let path = dir.join("profiles.toml");
    let file = SharedFile::new(&path);
    file.update(|_| "one\n".to_owned()).unwrap();

    let mut reader = std::fs::File::open(&path).unwrap();

    file.update(|_| "two\n".to_owned()).unwrap();

    let mut seen = String::new();
    std::io::Read::read_to_string(&mut reader, &mut seen).unwrap();
    // Content checks rather than an exact match: the file carries a `# penguin-version:`
    // header, and pinning the whole text here would make this test fail for the unrelated
    // reason that the header changed.
    assert!(
        seen.contains("one\n") && !seen.contains("two"),
        "a reader that opened the file before the update saw the update through its own \
         handle, so the destination was rewritten in place rather than replaced; it read {seen:?}"
    );
    let now = std::fs::read_to_string(&path).unwrap();
    assert!(
        now.contains("two\n") && !now.contains("one\n"),
        "and a reader opening it now must see the new version; it read {now:?}"
    );

    // And no temp file is left behind.
    let leftovers: Vec<String> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| {
            let n = e.ok()?.file_name().to_string_lossy().into_owned();
            // Ends-with rather than `extension()`: the temp files are dotfiles whose name
            // already contains dots, and `extension()` on Windows is case-insensitive in a
            // way that would quietly widen what this matches.
            n.ends_with(".tmp").then_some(n)
        })
        .collect();
    assert!(
        leftovers.is_empty(),
        "temp files left behind: {leftovers:?}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

// ---------------------------------------------------------------------------------------------
// WIN-03 — the file is readable only by the current user
// ---------------------------------------------------------------------------------------------

#[test]
fn every_file_the_protocol_creates_is_restricted_to_the_current_user() {
    let dir = scratch("perms");
    let path = dir.join("profiles.toml");
    let file = SharedFile::new(&path);
    file.update(|_| "secret-reference = \"credential:1\"\n".to_owned())
        .unwrap();

    for p in [&path, &file.lock_path()] {
        assert!(
            perms::is_user_only(p).unwrap(),
            "{} is not restricted to the current user: {}",
            p.display(),
            perms::describe(p).unwrap()
        );
    }

    // A conflict file holds the same kind of content and gets the same treatment -- it is the
    // easiest one to forget, because it is written on the failure path.
    let v = file.read().unwrap().version;
    file.update(|b| format!("{b}x\n")).unwrap();
    let err = file.write_if_unchanged(v, "rejected\n").unwrap_err();
    let SharedFileError::Conflict { conflict, .. } = &err else {
        panic!("expected a conflict");
    };
    assert!(
        perms::is_user_only(conflict).unwrap(),
        "a conflict file is world-readable: {}",
        perms::describe(conflict).unwrap()
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn restricting_is_idempotent_and_tolerates_a_missing_file() {
    let dir = scratch("idem");
    let path = dir.join("f");
    std::fs::write(&path, "x").unwrap();
    perms::restrict_to_current_user(&path).unwrap();
    let first = perms::describe(&path).unwrap();
    perms::restrict_to_current_user(&path).unwrap();
    assert_eq!(first, perms::describe(&path).unwrap());

    // A file that vanished between creation and restriction is a race, not an error: callers
    // restrict immediately after creating, and a concurrent delete should not fail the write.
    perms::restrict_to_current_user(&dir.join("gone")).unwrap();
    let _ = std::fs::remove_dir_all(&dir);
}

#[cfg(unix)]
#[test]
fn a_permissive_file_is_reported_as_permissive() {
    // The check has to be able to say no, or it says nothing.
    use std::os::unix::fs::PermissionsExt as _;
    let dir = scratch("permissive");
    let path = dir.join("f");
    std::fs::write(&path, "x").unwrap();
    let mut p = std::fs::metadata(&path).unwrap().permissions();
    p.set_mode(0o644);
    std::fs::set_permissions(&path, p).unwrap();
    assert!(!perms::is_user_only(&path).unwrap());
    perms::restrict_to_current_user(&path).unwrap();
    assert!(perms::is_user_only(&path).unwrap());
    let _ = std::fs::remove_dir_all(&dir);
}
