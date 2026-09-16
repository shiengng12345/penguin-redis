//! What `:copy` is allowed to put on the clipboard (v2.1 §23.4, §23.2, SEC-05, V-D06).
//!
//! The clipboard is one of §23.4's ten secret paths and one of SEC-05's injection paths, and
//! they are different problems:
//!
//! - A value can be harmless to display and still be a password. That is `pr_security`.
//! - A value can be secret-free and still carry `ESC ] 52` — the escape that *writes the
//!   clipboard* — so displaying it in a terminal hands an attacker the very destination this
//!   module is about. That is [`SafeText`](pr_core::SafeText).
//!
//! Both, in that order, and the result is a [`pr_security::Emitted`] because nothing may reach
//! the clipboard that has not been through the secret boundary.

use pr_security::{Destination, Emitted};

/// Prepare `raw` for the clipboard.
///
/// `raw` is the authoritative bytes (ADR-005): what `:copy` copies is what the server sent,
/// not what the table showed. Escaping happens first so the escaping itself cannot be defeated
/// by a secret that looks like an escape, and scrubbing happens last so it sees the final text.
#[must_use]
pub fn prepare(raw: &[u8]) -> Emitted {
    let escaped = pr_core::SafeText::from_bytes(raw.to_vec());
    pr_security::emit(Destination::Clipboard, escaped.as_display())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn a_password_never_reaches_the_clipboard() {
        let secret = "clipboard-secret-a71f";
        assert!(pr_security::secrets::remember(secret.as_bytes()));
        let out = prepare(format!("value={secret}").as_bytes());
        assert!(!out.text().contains(secret), "{}", out.text());
        assert_eq!(out.destination(), Destination::Clipboard);
    }

    #[test]
    fn an_escape_that_writes_the_clipboard_is_neutralised_before_it_gets_there() {
        // SEC-05: the payload's whole purpose is to reach this destination.
        let out = prepare(b"\x1b]52;c;ZXZpbA==\x07");
        assert!(!out.text().contains('\x1b'), "{}", out.text());
    }

    #[test]
    fn ordinary_text_survives_unchanged() {
        let out = prepare(b"user:1001");
        assert_eq!(out.text(), "user:1001");
    }
}
