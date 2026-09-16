//! The support bundle, and the absence of telemetry (v2.1 §23.4, §19.5, R41, V-D06).
//!
//! Two of §23.4's ten paths are here because they are the same question asked twice: *what does
//! this program tell someone else about the session?*
//!
//! - A **diagnostics bundle** is what a person attaches to a bug report. It is the most
//!   dangerous of the ten, because it is assembled deliberately, from everything, by someone
//!   who wants it to be complete.
//! - **Telemetry** is the one that would do it without being asked. There is none, and
//!   [`TELEMETRY_ENDPOINTS`] exists so that claim has somewhere to live and a test to keep it
//!   true.
//!
//! §19.5's rule for anything trace-like is 「只记字节数不记内容」 — record how much, not what.
//! [`Bundle::record_payload`] is that rule with a type: it takes a length and has no way to be
//! given the bytes.

use crate::sink::{Destination, Emitted, emit};

/// Where this program sends data about its own use.
///
/// Empty, and asserted empty. A constant rather than a sentence in a document, because a
/// sentence cannot fail a build.
pub const TELEMETRY_ENDPOINTS: &[&str] = &[];

/// A support bundle under construction.
///
/// Facts go in through [`Bundle::fact`], which scrubs. Payloads do not go in at all — only
/// their sizes, through [`Bundle::record_payload`].
#[derive(Debug, Default)]
pub struct Bundle {
    lines: Vec<String>,
}

impl Bundle {
    /// An empty bundle.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a `name: value` fact. The value is scrubbed.
    pub fn fact(&mut self, name: &str, value: &str) -> &mut Self {
        let clean = emit(Destination::DiagnosticsBundle, value);
        self.lines.push(format!("{name}: {clean}"));
        self
    }

    /// Record that a payload existed, and how big it was.
    ///
    /// §19.5: 「只记字节数不记内容」. There is deliberately no variant of this that accepts the
    /// bytes — a bundle that can hold a payload will eventually hold one with a password in it,
    /// and the reviewer of that change will be looking at a call site, not at this rule.
    pub fn record_payload(&mut self, name: &str, bytes: usize) -> &mut Self {
        self.lines
            .push(format!("{name}: {bytes} bytes (contents not recorded)"));
        self
    }

    /// The finished bundle, ready to write.
    #[must_use]
    pub fn finish(&self) -> Emitted {
        // Scrubbed again on the way out. `fact` already scrubs, but a secret can be registered
        // *after* a fact was added — a credential read half-way through a session — and the
        // bundle is written at the end.
        emit(Destination::DiagnosticsBundle, &self.lines.join("\n"))
    }
}

/// A bundle describing this build and this machine, and nothing about the session.
///
/// What is deliberately absent: the connection URI, the profile name, any key or value seen,
/// the history, the environment block. A bundle is useful because it says which version and
/// which platform; everything else in it is somebody else's data.
#[must_use]
pub fn build_info() -> Emitted {
    let mut b = Bundle::new();
    b.fact("penguin-redis", env!("CARGO_PKG_VERSION"))
        .fact("os", std::env::consts::OS)
        .fact("arch", std::env::consts::ARCH)
        .fact("telemetry", "none")
        .fact(
            "secrets-registered",
            &crate::secrets::registered().to_string(),
        );
    b.finish()
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::secrets;

    #[test]
    fn there_is_no_telemetry() {
        assert!(
            TELEMETRY_ENDPOINTS.is_empty(),
            "an endpoint appeared: {TELEMETRY_ENDPOINTS:?}"
        );
    }

    #[test]
    fn a_bundle_scrubs_facts_even_when_the_secret_is_registered_afterwards() {
        let secret = "bundle-secret-71ac";
        let mut b = Bundle::new();
        b.fact("connection", &format!("redis://u:{secret}@h"));
        // Registered only now, which is the realistic order: the credential is read when the
        // connection is made, and the bundle is written when something goes wrong later.
        assert!(secrets::remember(secret.as_bytes()));
        let out = b.finish();
        assert!(!out.text().contains(secret), "{}", out.text());
    }

    #[test]
    fn a_payload_is_a_length_and_nothing_else() {
        let mut b = Bundle::new();
        b.record_payload("reply", 4096);
        let out = b.finish();
        assert!(out.text().contains("4096 bytes"));
        assert!(out.text().contains("contents not recorded"));
    }

    #[test]
    fn build_info_says_the_build_and_nothing_about_the_session() {
        let secret = "build-info-secret-2b90";
        assert!(secrets::remember(secret.as_bytes()));
        let out = build_info();
        assert!(out.text().contains("penguin-redis:"));
        assert!(out.text().contains("telemetry: none"));
        assert!(!out.text().contains(secret));
        // A count is fine; the values are not.
        assert!(out.text().contains("secrets-registered:"));
    }
}
