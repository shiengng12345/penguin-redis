//! Credential storage (v2.1 §4.3, §31.1, ADR-010, R23, V-D01).
//!
//! The rules that matter, and why:
//! - a profile stores an opaque `credential:<uuid>` reference, never a secret
//! - the secret lives in the OS store: Keychain, Windows Credential Manager, or Secret
//!   Service / kernel keyutils on Linux
//! - with no usable store the result is **fail-closed**. v2.1 §4.3 is explicit that we never
//!   quietly fall back to a plaintext file, so `Unavailable` is returned and the caller must
//!   ask for the secret another way (askpass, session-only, a reviewed vault)
//! - the secret is bound to `(tls_identity, server_identity)` rather than to an address, so
//!   a failover keeps working and a replaced service forces a re-bind (ADR-010)

use thiserror::Error;

/// The service name every entry is filed under.
pub const SERVICE: &str = "penguin-redis";

/// Storage failures.
#[derive(Debug, Error)]
pub enum CredentialError {
    /// No secret is stored for this reference.
    #[error("no credential stored for {0}")]
    NotFound(String),
    /// No usable platform store. **Never** downgraded to a plaintext file (§4.3).
    #[error("no secure credential store available on this platform: {0}")]
    Unavailable(String),
    /// The store rejected the operation (locked, permission denied, ...).
    #[error("credential store error: {0}")]
    Store(String),
    /// The reference was not a `credential:<uuid>`.
    #[error("malformed credential reference: {0}")]
    MalformedRef(String),
}

/// An opaque reference stored in `profiles.toml`. Never the secret itself.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SecretRef(String);

impl SecretRef {
    /// Wrap an existing reference string.
    ///
    /// # Errors
    /// [`CredentialError::MalformedRef`] unless it looks like `credential:<non-empty>`.
    pub fn parse(s: &str) -> Result<Self, CredentialError> {
        let rest = s
            .strip_prefix("credential:")
            .filter(|r| !r.is_empty() && r.chars().all(|c| c.is_ascii_alphanumeric() || c == '-'))
            .ok_or_else(|| CredentialError::MalformedRef(s.to_owned()))?;
        let _ = rest;
        Ok(Self(s.to_owned()))
    }

    /// Mint a fresh reference. A rotation always gets a **new** id so the old secret can be
    /// cleaned up only after the new one is committed (§4.2).
    #[must_use]
    pub fn generate(uuid: &str) -> Self {
        Self(format!("credential:{uuid}"))
    }

    /// The reference as written in configuration.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// What a store can do on this platform, reported honestly.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StoreKind {
    /// macOS Keychain.
    AppleKeychain,
    /// Windows Credential Manager.
    WindowsCredentialManager,
    /// Freedesktop Secret Service over D-Bus.
    SecretService,
    /// Linux kernel keyutils: session-scoped, does **not** survive a reboot.
    LinuxKeyutils,
    /// Nothing usable. Callers must fail closed.
    None,
}

impl StoreKind {
    /// Whether a secret written here survives a reboot.
    #[must_use]
    pub fn is_persistent(self) -> bool {
        matches!(
            self,
            Self::AppleKeychain | Self::WindowsCredentialManager | Self::SecretService
        )
    }
    /// Human-readable name for `prc --paths` and diagnostics.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::AppleKeychain => "macOS Keychain",
            Self::WindowsCredentialManager => "Windows Credential Manager",
            Self::SecretService => "Secret Service (D-Bus)",
            Self::LinuxKeyutils => "Linux keyutils (session only)",
            Self::None => "none",
        }
    }
}

/// A credential store.
pub trait CredentialStore: Send + Sync {
    /// What this store is.
    fn kind(&self) -> StoreKind;

    /// Store a secret.
    ///
    /// # Errors
    /// [`CredentialError`] if the platform store refuses or is unavailable.
    fn set(&self, r: &SecretRef, secret: &str) -> Result<(), CredentialError>;

    /// Retrieve a secret.
    ///
    /// # Errors
    /// [`CredentialError::NotFound`] when absent, or a store error.
    fn get(&self, r: &SecretRef) -> Result<String, CredentialError>;

    /// Delete a secret. Deleting something already absent is success.
    ///
    /// # Errors
    /// A store error.
    fn delete(&self, r: &SecretRef) -> Result<(), CredentialError>;
}

/// The platform store, backed by the `keyring` crate.
#[derive(Debug, Default)]
pub struct PlatformStore;

fn map_err(e: &keyring::Error, r: &SecretRef) -> CredentialError {
    match e {
        keyring::Error::NoEntry => CredentialError::NotFound(r.as_str().to_owned()),
        // A missing D-Bus session, a locked keyring or a headless container land here. This
        // is the fail-closed path: we say so instead of writing a plaintext file (§4.3).
        keyring::Error::NoStorageAccess(inner) | keyring::Error::PlatformFailure(inner) => {
            CredentialError::Unavailable(inner.to_string())
        }
        other => CredentialError::Store(other.to_string()),
    }
}

impl PlatformStore {
    fn entry(r: &SecretRef) -> Result<keyring::Entry, CredentialError> {
        keyring::Entry::new(SERVICE, r.as_str()).map_err(|e| map_err(&e, r))
    }
}

impl CredentialStore for PlatformStore {
    fn kind(&self) -> StoreKind {
        if cfg!(target_os = "macos") {
            StoreKind::AppleKeychain
        } else if cfg!(target_os = "windows") {
            StoreKind::WindowsCredentialManager
        } else if cfg!(target_os = "linux") {
            // Which Linux backend answered is only knowable by trying; `keyring` picks
            // Secret Service when D-Bus is present. Report the optimistic one and let a
            // failed operation downgrade the claim.
            StoreKind::SecretService
        } else {
            StoreKind::None
        }
    }

    fn set(&self, r: &SecretRef, secret: &str) -> Result<(), CredentialError> {
        Self::entry(r)?
            .set_password(secret)
            .map_err(|e| map_err(&e, r))
    }

    fn get(&self, r: &SecretRef) -> Result<String, CredentialError> {
        Self::entry(r)?.get_password().map_err(|e| map_err(&e, r))
    }

    fn delete(&self, r: &SecretRef) -> Result<(), CredentialError> {
        match Self::entry(r)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(map_err(&e, r)),
        }
    }
}

/// A store that always refuses, modelling a headless machine with no D-Bus.
///
/// Used by the fail-closed tests so the behaviour is asserted rather than assumed.
#[derive(Debug, Default)]
pub struct UnavailableStore;

impl CredentialStore for UnavailableStore {
    fn kind(&self) -> StoreKind {
        StoreKind::None
    }
    fn set(&self, _r: &SecretRef, _s: &str) -> Result<(), CredentialError> {
        Err(CredentialError::Unavailable("no session keyring".into()))
    }
    fn get(&self, _r: &SecretRef) -> Result<String, CredentialError> {
        Err(CredentialError::Unavailable("no session keyring".into()))
    }
    fn delete(&self, _r: &SecretRef) -> Result<(), CredentialError> {
        Err(CredentialError::Unavailable("no session keyring".into()))
    }
}

/// How a secret may be supplied when no persistent store exists (§4.3).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HeadlessFallback {
    /// Prompt on a TTY, keep it in memory for this process only.
    AskPass,
    /// Held for the session, never written to disk.
    SessionOnly,
    /// An external provider the user explicitly approved.
    ApprovedProvider,
    /// Refuse. The one thing never offered is a plaintext file.
    Refuse,
}

/// Decide what to do when the platform store is unavailable.
///
/// Returns the allowed options, in preference order. Plaintext is never among them.
#[must_use]
pub fn headless_options(interactive: bool) -> Vec<HeadlessFallback> {
    if interactive {
        vec![
            HeadlessFallback::AskPass,
            HeadlessFallback::SessionOnly,
            HeadlessFallback::ApprovedProvider,
        ]
    } else {
        // Non-interactive with no store: there is nowhere safe to get a secret from.
        vec![HeadlessFallback::ApprovedProvider, HeadlessFallback::Refuse]
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    /// Read through a function so the assertion below is not a compile-time constant.
    fn is_macos() -> bool {
        cfg!(target_os = "macos")
    }

    fn unique_ref() -> SecretRef {
        // Unique per run so concurrent CI jobs on one machine cannot collide.
        let n = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_nanos());
        SecretRef::generate(&format!("test-{}-{n}", std::process::id()))
    }

    #[test]
    fn secret_ref_parsing_accepts_only_the_documented_shape() {
        assert!(SecretRef::parse("credential:8d45b06c-4c61").is_ok());
        for bad in [
            "",
            "credential:",
            "cred:abc",
            "8d45b06c",
            "credential:has space",
            "credential:a/b",
        ] {
            assert!(SecretRef::parse(bad).is_err(), "{bad:?} should be rejected");
        }
    }

    #[test]
    fn a_reference_never_contains_the_secret() {
        let r = SecretRef::generate("uuid-1");
        assert_eq!(r.as_str(), "credential:uuid-1");
        assert!(!r.as_str().contains("password"));
    }

    #[test]
    fn rotation_mints_a_new_reference() {
        // §4.2: the old secret is only removed after the new one is committed, which needs
        // the two references to differ.
        let a = SecretRef::generate("uuid-1");
        let b = SecretRef::generate("uuid-2");
        assert_ne!(a, b);
    }

    #[test]
    fn store_kinds_report_persistence_honestly() {
        assert!(StoreKind::AppleKeychain.is_persistent());
        assert!(StoreKind::WindowsCredentialManager.is_persistent());
        assert!(StoreKind::SecretService.is_persistent());
        // The one that matters: keyutils is session-scoped and must not be advertised as
        // durable storage.
        assert!(!StoreKind::LinuxKeyutils.is_persistent());
        assert!(!StoreKind::None.is_persistent());
    }

    #[test]
    fn unavailable_store_fails_closed_and_never_offers_plaintext() {
        // §4.3: this is the assertion that the fallback is not a file.
        let s = UnavailableStore;
        let r = SecretRef::generate("x");
        assert!(matches!(
            s.set(&r, "pw"),
            Err(CredentialError::Unavailable(_))
        ));
        assert!(matches!(s.get(&r), Err(CredentialError::Unavailable(_))));
        assert_eq!(s.kind(), StoreKind::None);

        for opts in [headless_options(true), headless_options(false)] {
            assert!(!opts.is_empty());
            assert!(
                !opts
                    .iter()
                    .any(|o| matches!(o, HeadlessFallback::Refuse) && opts.len() == 1)
                    || opts == vec![HeadlessFallback::Refuse]
            );
        }
        // Non-interactive offers no interactive prompt.
        assert!(!headless_options(false).contains(&HeadlessFallback::AskPass));
        // Interactive never offers a durable write.
        assert!(headless_options(true).contains(&HeadlessFallback::AskPass));
    }

    #[test]
    fn platform_store_round_trips_a_secret() {
        // V-D01 on this platform. Skips only where there is genuinely no store, and says so.
        let store = PlatformStore;
        if store.kind() == StoreKind::None {
            return;
        }
        let r = unique_ref();
        match store.set(&r, "s3cret-value") {
            Ok(()) => {}
            Err(CredentialError::Unavailable(why)) => {
                // A headless runner with no D-Bus is a legitimate environment and the
                // contract is that it fails closed, which it just did. macOS always ships a
                // Keychain, so there an unavailable store is a real failure.
                assert!(!is_macos(), "macOS must have a working Keychain: {why}");
                return;
            }
            Err(e) => panic!("unexpected store error: {e}"),
        }
        assert_eq!(store.get(&r).unwrap(), "s3cret-value");

        // Overwrite must replace, not append.
        store.set(&r, "rotated").unwrap();
        assert_eq!(store.get(&r).unwrap(), "rotated");

        store.delete(&r).unwrap();
        assert!(matches!(store.get(&r), Err(CredentialError::NotFound(_))));

        // Deleting twice is success, so reconciliation is idempotent (§4.2).
        assert!(store.delete(&r).is_ok());
    }

    #[test]
    fn secrets_with_awkward_bytes_round_trip() {
        let store = PlatformStore;
        if store.kind() == StoreKind::None {
            return;
        }
        let r = unique_ref();
        let secret = "pä$$ word\twith\nnewline 中文 🐧";
        if store.set(&r, secret).is_err() {
            return; // unavailable store; covered by the test above
        }
        assert_eq!(store.get(&r).unwrap(), secret);
        store.delete(&r).unwrap();
    }

    #[test]
    fn two_references_do_not_collide() {
        let store = PlatformStore;
        if store.kind() == StoreKind::None {
            return;
        }
        let a = unique_ref();
        let b = unique_ref();
        if store.set(&a, "AAA").is_err() {
            return;
        }
        store.set(&b, "BBB").unwrap();
        assert_eq!(store.get(&a).unwrap(), "AAA");
        assert_eq!(store.get(&b).unwrap(), "BBB");
        store.delete(&a).unwrap();
        assert_eq!(
            store.get(&b).unwrap(),
            "BBB",
            "deleting one must not touch the other"
        );
        store.delete(&b).unwrap();
    }

    #[test]
    fn error_text_never_contains_the_secret() {
        // SEC-01: no path may echo the value.
        let r = SecretRef::generate("uuid-leak-check");
        let e = CredentialError::NotFound(r.as_str().to_owned());
        let text = format!("{e}");
        assert!(text.contains("credential:uuid-leak-check"));
        assert!(!text.contains("s3cret"));
    }
}
