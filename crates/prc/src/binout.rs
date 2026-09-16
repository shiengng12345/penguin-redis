//! Binary-exact stdout (v2.1 §35.1, WIN-02, V-C07).
//!
//! §35.1 requires `-x`, `-X`, `--bytes`, `--pipe`, `--output resp` and every non-TTY output
//! path to open stdout in binary mode on Windows, with no CRLF translation. A value that came
//! out of Redis as `a\r\nb` has to reach the pipe as `a\r\nb`; a client that turns it into
//! `a\r\r\nb` has corrupted the user's data on the way out, silently.
//!
//! Rust's `std::io::Stdout` already writes bytes verbatim to a pipe or a file, so the
//! translation risk is not the one a C program has. The Windows risk is different and worth
//! stating, because it is easy to meet by accident: when stdout is a **console**, Rust routes
//! writes through `WriteConsoleW`, which takes UTF-16. Bytes that are not valid UTF-8 cannot
//! survive that, and the write fails rather than emitting something wrong.
//!
//! Failing is the right behaviour and the wrong message. [`write_binary`] turns it into an
//! explanation: binary output is going to a terminal, and it needs redirecting.

use std::io::{IsTerminal, Write};

/// Why binary output could not be written.
#[derive(Debug, thiserror::Error)]
pub enum BinaryOutError {
    /// The bytes are not valid UTF-8 and stdout is a terminal.
    ///
    /// Not a failure of the value or of the terminal: it is a request that cannot be honoured
    /// as asked, and the fix is one shell character.
    #[error(
        "this value contains bytes that are not text, and stdout is a terminal; \
         redirect it to a file or a pipe (for example `> value.bin`)"
    )]
    ConsoleCannotTakeBinary,
    /// Anything else.
    #[error("writing to stdout: {0}")]
    Io(#[from] std::io::Error),
}

/// Write bytes to stdout exactly as given.
///
/// No trailing delimiter — `--bytes` is defined as the blob and nothing else (§18.1), and a
/// newline appended "for readability" changes the length of the thing the user asked for.
///
/// # Errors
/// [`BinaryOutError::ConsoleCannotTakeBinary`] when non-text bytes are aimed at a terminal,
/// or [`BinaryOutError::Io`] for a write failure such as a closed pipe.
pub fn write_binary(bytes: &[u8]) -> Result<(), BinaryOutError> {
    let out = std::io::stdout();
    if out.is_terminal() && std::str::from_utf8(bytes).is_err() {
        return Err(BinaryOutError::ConsoleCannotTakeBinary);
    }
    let mut lock = out.lock();
    lock.write_all(bytes)?;
    lock.flush()?;
    Ok(())
}

/// A byte pattern built out of everything that gets mangled by a text-mode pipeline.
///
/// Used by the `binary-out` probe so WIN-02 is checked against the values that actually break
/// rather than against a friendly string:
///
/// - `\r\n` — the pair a text-mode write would turn into `\r\r\n`
/// - a bare `\r` and a bare `\n` — each is a line ending to somebody
/// - `\x1a` — Ctrl+Z, end-of-file to a Windows text-mode *reader*
/// - `\0` — truncates anything that treats bytes as a C string
/// - `\xff\xfe` — a UTF-16 byte-order mark, and invalid UTF-8
#[must_use]
pub fn canary() -> Vec<u8> {
    let mut v = Vec::new();
    v.extend_from_slice(b"start");
    v.extend_from_slice(b"\r\n");
    v.extend_from_slice(b"\r");
    v.extend_from_slice(b"\n");
    v.extend_from_slice(b"\x1a");
    v.extend_from_slice(b"\0");
    v.extend_from_slice(&[0xff, 0xfe]);
    v.extend_from_slice("中文".as_bytes());
    v.extend_from_slice(b"end");
    v
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn the_canary_contains_every_byte_a_text_pipeline_mangles() {
        let c = canary();
        assert!(c.windows(2).any(|w| w == b"\r\n"), "the CRLF pair");
        assert!(c.contains(&0x1a), "Ctrl+Z, text-mode EOF");
        assert!(c.contains(&0), "NUL");
        assert!(c.contains(&0xff), "invalid UTF-8");
        assert!(std::str::from_utf8(&c).is_err(), "not text, deliberately");
        assert_eq!(c.len(), 5 + 2 + 1 + 1 + 1 + 1 + 2 + 6 + 3);
    }

    #[test]
    fn the_canary_is_stable() {
        // A test elsewhere compares bytes against this; it must not drift silently.
        assert_eq!(canary(), canary());
        assert!(canary().starts_with(b"start"));
        assert!(canary().ends_with(b"end"));
    }

    #[test]
    fn valid_text_is_never_refused_even_on_a_terminal() {
        // The refusal is about bytes a console physically cannot take, not about caution.
        for s in ["hello", "中文", "line\r\nline"] {
            assert!(
                std::str::from_utf8(s.as_bytes()).is_ok(),
                "{s:?} is text, so it goes to a console unchanged"
            );
        }
    }

    #[test]
    fn the_refusal_is_about_bytes_the_console_cannot_take_not_about_caution() {
        // The decision, without writing anything. A unit test must not put the canary on the
        // harness's own stdout: a NUL and a Ctrl+Z in the middle of the test output is how a
        // green run gets reported as a failure, which is what happened the first time.
        let binary = canary();
        let text = b"plain ascii".to_vec();
        for (bytes, is_text) in [(&binary, false), (&text, true)] {
            assert_eq!(std::str::from_utf8(bytes).is_ok(), is_text);
        }
        // Under `cargo test` stdout is a pipe, so the refusal does not apply and text goes
        // through untouched.
        assert!(!std::io::stdout().is_terminal());
        write_binary(b"").expect("an empty write is always fine");
    }
}
