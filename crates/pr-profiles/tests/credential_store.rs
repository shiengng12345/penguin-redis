//! V-G05 / WIN-03 — the real platform credential store, round-tripped (v2.1 §4.3, §30.2).
//!
//! WIN-03 asks for three things and this file covers the first:
//!
//! > Windows Credential Manager + 配置锁 | **凭证存取**、`LockFileEx` 并发写、ACL 仅当前用户
//!
//! The other two live in `shared_files.rs`, which also runs everywhere.
//!
//! This test touches the **real** store — Keychain on macOS, Credential Manager on Windows,
//! Secret Service on Linux — because a mock proves only that the mock works, and the thing
//! being verified is that the platform accepts what we hand it. It is `#[ignore]`d for one
//! reason: on a headless Linux box with no D-Bus session there is no store to talk to, and a
//! test that fails there would be reporting the environment rather than the code. The CI job
//! runs it explicitly on all three platforms, with a D-Bus session on Linux.
//!
//! Everything it writes is namespaced by process id and deleted on the way out, including on
//! the failure paths — a test that leaves entries behind in a developer's Keychain is a test
//! they will disable.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, missing_docs)]

use pr_profiles::credentials::{
    CredentialError, CredentialStore, PlatformStore, SecretRef, StoreKind,
};

fn scratch_ref(tag: &str) -> SecretRef {
    SecretRef::generate(&format!("vg05-{tag}-{}", std::process::id()))
}

/// Delete on every path, so a failing assertion does not leave an entry behind.
struct Cleanup(SecretRef);
impl Drop for Cleanup {
    fn drop(&mut self) {
        let _ = PlatformStore.delete(&self.0);
    }
}

#[test]
#[ignore = "touches the real OS credential store; run by the shared-files CI job with --ignored"]
fn a_secret_round_trips_through_the_platform_store() {
    let store = PlatformStore;
    let r = scratch_ref("roundtrip");
    let _cleanup = Cleanup(r.clone());

    // A secret with bytes that a naive store would mangle: non-ASCII, a quote, a newline and
    // a NUL-adjacent control character. If the platform cannot hold these, better to find out
    // here than when a user's password contains one.
    let secret = "p@ssw0rd — \"quoted\"\n\ttab 密码";
    store
        .set(&r, secret)
        .unwrap_or_else(|e| panic!("{} refused a secret: {e}", store.kind().name()));

    let got = store.get(&r).expect("read back what was just written");
    assert_eq!(got, secret, "the store did not return what it was given");

    // Overwriting is an update, not a second entry.
    store.set(&r, "second").unwrap();
    assert_eq!(store.get(&r).unwrap(), "second");

    store.delete(&r).unwrap();
    let after = store.get(&r);
    assert!(
        matches!(after, Err(CredentialError::NotFound(_))),
        "a deleted secret is still readable: {after:?}"
    );

    // Deleting something absent is success, so a cleanup path never has to check first.
    store.delete(&r).unwrap();
}

#[test]
#[ignore = "touches the real OS credential store; run by the shared-files CI job with --ignored"]
fn the_store_reports_the_platform_it_actually_is() {
    // A store that claims to be the Windows Credential Manager on macOS would make every
    // diagnostic about credentials misleading.
    let kind = PlatformStore.kind();
    let expected = if cfg!(target_os = "macos") {
        StoreKind::AppleKeychain
    } else if cfg!(target_os = "windows") {
        StoreKind::WindowsCredentialManager
    } else {
        StoreKind::SecretService
    };
    assert_eq!(kind, expected);
    assert!(kind.is_persistent(), "{} is not persistent", kind.name());
}

#[test]
#[ignore = "touches the real OS credential store; run by the shared-files CI job with --ignored"]
fn two_references_do_not_collide() {
    // Secret references are per-profile. If two profiles shared an entry, changing one
    // profile's password would silently change the other's -- the 「凭证误绑定」 half of LIFE-03.
    let store = PlatformStore;
    let a = scratch_ref("collide-a");
    let b = scratch_ref("collide-b");
    let _ca = Cleanup(a.clone());
    let _cb = Cleanup(b.clone());

    store.set(&a, "secret-a").unwrap();
    store.set(&b, "secret-b").unwrap();
    assert_eq!(store.get(&a).unwrap(), "secret-a");
    assert_eq!(store.get(&b).unwrap(), "secret-b");

    store.delete(&a).unwrap();
    assert_eq!(
        store.get(&b).unwrap(),
        "secret-b",
        "deleting one reference removed another"
    );
}

#[test]
fn a_secret_reference_is_never_the_secret() {
    // Runs everywhere, no store needed. §4.3: what goes in a profile file is a *reference*.
    // This is the check that the reference cannot accidentally become a place to put a
    // password -- the profile file is the thing that gets copied into a bug report.
    let r = SecretRef::generate("profile-uuid");
    let s = r.as_str();
    assert!(s.starts_with("credential:"), "{s}");
    assert!(!s.contains("password"));
    assert!(
        SecretRef::parse("hunter2").is_err(),
        "a bare string must not parse as a reference"
    );
    assert!(SecretRef::parse(s).is_ok());
}
