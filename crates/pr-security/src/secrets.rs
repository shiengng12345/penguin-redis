//! The one place a secret is allowed to be turned back into text: nowhere (v2.1 §23.4, §19.5,
//! R24, R41, SEC-01, V-D06).
//!
//! §23.4 names ten paths a password can escape through — a URI parse error, a TLS error, a
//! `Debug` derive, a trace line, history, the clipboard, a crash report, `--trace-wire`, a
//! diagnostics bundle, and telemetry. They have nothing in common except that each one turns
//! some internal value into text somebody can read, and that each one was written by somebody
//! who was not thinking about passwords at the time.
//!
//! So this module is not ten rules. It is one register and one function:
//!
//! - [`remember`] tells the process that a byte string is a secret. Every place that *learns*
//!   a secret — a profile's credential, an `AUTH` argument, a URI's userinfo — says so once.
//! - [`scrub`] removes every remembered secret from anything on its way out.
//!
//! The asymmetry is deliberate. A rule per path has to be right ten times and stays right only
//! until somebody adds an eleventh; a register has to be right once, at the point where the
//! secret enters, and every later path inherits it. `crates/prc/tests/secret_paths.rs` drives
//! all ten with the same injected token and greps the result, which is §23.4's own method:
//! 每路径注入已知 token 后全量 grep.
//!
//! ## What it deliberately does not do
//!
//! It does not guess. There is no "looks like a password" heuristic, because a heuristic that
//! is wrong in the safe direction redacts a key the user needed to see, and one that is wrong
//! in the other direction is a leak nobody notices. Only values somebody explicitly registered
//! are removed.
//!
//! Very short values are refused by [`remember`]: registering `"a"` would replace every `a` in
//! every message, which destroys the output and hides nothing — an attacker who knows the
//! password is one character does not need the log.

use std::sync::{OnceLock, RwLock};

/// What a redacted value is replaced with.
///
/// Fixed text rather than `*` repeated to the original length: the length of a password is
/// worth something to whoever is guessing it.
pub const REDACTED: &str = "[redacted]";

/// Shorter than this and a value is not registrable — see the module note.
pub const MIN_SECRET_LEN: usize = 4;

/// A set of values that must not appear in anything a person or a file can read.
#[derive(Debug, Default, Clone)]
pub struct SecretSet {
    values: Vec<Vec<u8>>,
}

impl SecretSet {
    /// An empty set.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Remember a secret. Returns whether it was taken.
    ///
    /// Values shorter than [`MIN_SECRET_LEN`] are refused; see the module note.
    pub fn remember(&mut self, secret: &[u8]) -> bool {
        if secret.len() < MIN_SECRET_LEN {
            return false;
        }
        if self.values.iter().any(|v| v == secret) {
            return true;
        }
        self.values.push(secret.to_vec());
        // Longest first, so a password that contains another registered value is replaced as a
        // whole rather than leaving the tail of it in the output.
        self.values.sort_by_key(|v| std::cmp::Reverse(v.len()));
        true
    }

    /// How many values are registered.
    #[must_use]
    pub fn len(&self) -> usize {
        self.values.len()
    }

    /// Whether nothing is registered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.values.is_empty()
    }

    /// Replace every remembered value in `haystack`.
    #[must_use]
    pub fn scrub(&self, haystack: &[u8]) -> Vec<u8> {
        let mut out = haystack.to_vec();
        for v in &self.values {
            out = replace_all(&out, v, REDACTED.as_bytes());
        }
        out
    }

    /// The first remembered value that occurs in `haystack`, if any.
    ///
    /// This is what the audit test asks. It returns the value rather than a bool so a failure
    /// can say *which* secret leaked when several are registered.
    #[must_use]
    pub fn appears_in<'a>(&'a self, haystack: &[u8]) -> Option<&'a [u8]> {
        self.values
            .iter()
            .find(|v| find(haystack, v).is_some())
            .map(Vec::as_slice)
    }
}

/// Find `needle` in `haystack`.
fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() || needle.len() > haystack.len() {
        return None;
    }
    haystack.windows(needle.len()).position(|w| w == needle)
}

/// Replace every occurrence of `needle`.
fn replace_all(haystack: &[u8], needle: &[u8], with: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(haystack.len());
    let mut rest = haystack;
    while let Some(i) = find(rest, needle) {
        out.extend_from_slice(&rest[..i]);
        out.extend_from_slice(with);
        rest = &rest[i + needle.len()..];
    }
    out.extend_from_slice(rest);
    out
}

/// The process-wide register.
///
/// Process-wide because the sinks are: a panic hook, a trace file, the clipboard. None of them
/// is handed a context object, and threading one through every one of them is how a path gets
/// missed. ADR-029 already scopes every budget to the process; this is the same boundary.
fn register() -> &'static RwLock<SecretSet> {
    static REGISTER: OnceLock<RwLock<SecretSet>> = OnceLock::new();
    REGISTER.get_or_init(|| RwLock::new(SecretSet::new()))
}

/// Tell the process that `secret` must never be written anywhere.
///
/// Called where a secret *enters*: a credential read from the OS store, the userinfo of a
/// connection URI, the argument of an `AUTH`.
#[must_use]
pub fn remember(secret: &[u8]) -> bool {
    register().write().is_ok_and(|mut r| r.remember(secret))
}

/// Remove every remembered secret from `haystack`.
#[must_use]
pub fn scrub(haystack: &[u8]) -> Vec<u8> {
    register()
        .read()
        .map_or_else(|_| haystack.to_vec(), |r| r.scrub(haystack))
}

/// [`scrub`], for text.
#[must_use]
pub fn scrub_str(s: &str) -> String {
    String::from_utf8_lossy(&scrub(s.as_bytes())).into_owned()
}

/// Whether any remembered secret occurs in `haystack`. For audits.
#[must_use]
pub fn appears(haystack: &[u8]) -> bool {
    register()
        .read()
        .is_ok_and(|r| r.appears_in(haystack).is_some())
}

/// How many secrets the process has been told about.
#[must_use]
pub fn registered() -> usize {
    register().read().map_or(0, |r| r.len())
}

/// Install a panic hook that scrubs the message before it is printed (§23.4, the crash-report
/// path).
///
/// A panic message is assembled from whatever was in scope, which is exactly when a password
/// held in a local ends up on somebody's terminal and then in a bug report. The hook keeps the
/// previous one and passes the scrubbed text through it, so this composes with a backtrace
/// printer rather than replacing it.
///
/// Returns whether it installed; calling it twice is harmless but only the first installs.
pub fn scrub_panics() -> bool {
    static INSTALLED: OnceLock<()> = OnceLock::new();
    let mut installed = false;
    INSTALLED.get_or_init(|| {
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            let rendered = format!("{info}");
            let clean = scrub_str(&rendered);
            if clean == rendered {
                previous(info);
            } else {
                // The message carried a secret, so the original must not reach the default
                // hook at all — printing a scrubbed copy *and* the original would be worse
                // than doing nothing.
                eprintln!("{clean}");
            }
        }));
        installed = true;
    });
    installed
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn a_registered_value_is_removed_wherever_it_appears() {
        let mut s = SecretSet::new();
        assert!(s.remember(b"hunter2-c0ffee"));
        let text = b"connecting as bob:hunter2-c0ffee@db (hunter2-c0ffee)".to_vec();
        let out = s.scrub(&text);
        assert!(s.appears_in(&text).is_some());
        assert!(
            s.appears_in(&out).is_none(),
            "{:?}",
            String::from_utf8_lossy(&out)
        );
        assert_eq!(
            String::from_utf8_lossy(&out),
            "connecting as bob:[redacted]@db ([redacted])"
        );
    }

    #[test]
    fn a_value_too_short_to_register_is_refused_rather_than_accepted_uselessly() {
        // Registering "a" would replace every `a` in every message and hide nothing.
        let mut s = SecretSet::new();
        assert!(!s.remember(b"abc"));
        assert!(s.is_empty());
        assert_eq!(String::from_utf8_lossy(&s.scrub(b"abc def")), "abc def");
    }

    #[test]
    fn the_replacement_does_not_leak_the_length() {
        let mut s = SecretSet::new();
        s.remember(b"short1");
        s.remember(b"a-very-much-longer-password");
        let a = s.scrub(b"x short1 y");
        let b = s.scrub(b"x a-very-much-longer-password y");
        assert_eq!(
            String::from_utf8_lossy(&a).replace("short1", ""),
            String::from_utf8_lossy(&b).replace("a-very-much-longer-password", "")
        );
    }

    #[test]
    fn a_secret_containing_another_secret_is_replaced_whole() {
        let mut s = SecretSet::new();
        s.remember(b"pass");
        s.remember(b"password-long");
        let out = s.scrub(b"password-long");
        assert_eq!(String::from_utf8_lossy(&out), REDACTED);
    }

    #[test]
    fn scrubbing_binary_is_byte_exact_outside_the_secret() {
        let mut s = SecretSet::new();
        s.remember(b"\x00\x01secret\xff");
        let text = b"\x1b[31m\x00\x01secret\xff\x1b[0m".to_vec();
        let out = s.scrub(&text);
        assert_eq!(&out[..5], b"\x1b[31m");
        assert!(out.ends_with(b"\x1b[0m"));
        assert!(s.appears_in(&out).is_none());
    }

    #[test]
    fn registering_the_same_value_twice_keeps_one_copy() {
        let mut s = SecretSet::new();
        assert!(s.remember(b"abcd"));
        assert!(s.remember(b"abcd"));
        assert_eq!(s.len(), 1);
    }
}
