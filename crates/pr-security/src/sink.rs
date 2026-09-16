//! The narrow neck every piece of text passes through on its way out of the process
//! (v2.1 §23.4, §19.5, R24, R41, SEC-01, V-D06).
//!
//! [`secrets`](crate::secrets) can remove a password from a string. That is only half the
//! problem: something still has to *call* it, at every one of §23.4's ten destinations, and
//! keep calling it when an eleventh is added in Phase 3 by somebody who has never read §23.4.
//!
//! So a destination does not take a `String`. It takes an [`Emitted`], and the only way to get
//! one is [`emit`], which scrubs. The check moves from "did everybody remember" — which is a
//! promise — to "does it compile", which is not.
//!
//! ## This is not [`SafeText`](pr_core::SafeText)
//!
//! `SafeText` answers a different question: *can this text make the terminal do something*
//! (§23.2, SEC-05). A value can be perfectly safe to print and still be a password, and a value
//! can be secret-free and still carry `ESC ] 52` to write the clipboard. Both boundaries exist,
//! they compose, and neither implies the other. Text bound for a terminal goes through both.
//!
//! ## Why the destination is in the type
//!
//! It is not used for the scrubbing — every destination scrubs identically. It is there so the
//! audit can say *which* path it is testing, so a future destination has to be added to
//! [`Destination`] before it can be written to, and so the ten §23.4 paths have names in the
//! code rather than only in a document.

use crate::secrets;

/// Somewhere text can end up that a person, a file or another program can read.
///
/// These are §23.4's ten paths. Adding an eleventh means adding a variant, which is the point:
/// a new way out of the process should not be a one-line change nobody reviews.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Destination {
    /// A connection URI rendered back to a person — usually in a parse error.
    Uri,
    /// A TLS failure.
    TlsError,
    /// Anything produced by a `Debug` implementation.
    Debug,
    /// A diagnostic or trace line.
    Trace,
    /// The persisted command history.
    History,
    /// The system clipboard (`:copy`).
    Clipboard,
    /// A panic message or crash report.
    CrashReport,
    /// `--trace-wire`'s raw capture.
    TraceWire,
    /// A support bundle.
    DiagnosticsBundle,
    /// Anything that would leave the machine. There is nothing here; see
    /// [`crate::diagnostics::TELEMETRY_ENDPOINTS`].
    Telemetry,
}

impl Destination {
    /// Every destination, so the audit can iterate them rather than list them again.
    pub const ALL: &'static [Self] = &[
        Self::Uri,
        Self::TlsError,
        Self::Debug,
        Self::Trace,
        Self::History,
        Self::Clipboard,
        Self::CrashReport,
        Self::TraceWire,
        Self::DiagnosticsBundle,
        Self::Telemetry,
    ];

    /// The §23.4 name, for reports and test output.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Uri => "uri-parse-error",
            Self::TlsError => "tls-error",
            Self::Debug => "debug-derive",
            Self::Trace => "trace",
            Self::History => "history",
            Self::Clipboard => "clipboard",
            Self::CrashReport => "crash-report",
            Self::TraceWire => "trace-wire",
            Self::DiagnosticsBundle => "diagnostics-bundle",
            Self::Telemetry => "telemetry",
        }
    }
}

/// Text that has been through [`emit`] and may therefore be written out.
///
/// The field is private and there is no other constructor. A function that writes to one of
/// [`Destination`]'s places takes this type, so "did you scrub it" is answered by the compiler.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Emitted {
    destination: Destination,
    text: String,
}

impl Emitted {
    /// The scrubbed text.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Where it is going.
    #[must_use]
    pub fn destination(&self) -> Destination {
        self.destination
    }
}

impl std::fmt::Display for Emitted {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.text)
    }
}

/// Bytes that have been through [`emit_bytes`]. The binary counterpart of [`Emitted`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EmittedBytes {
    destination: Destination,
    bytes: Vec<u8>,
}

impl EmittedBytes {
    /// The scrubbed bytes.
    #[must_use]
    pub fn bytes(&self) -> &[u8] {
        &self.bytes
    }

    /// Where they are going.
    #[must_use]
    pub fn destination(&self) -> Destination {
        self.destination
    }
}

/// Scrub `text` for `destination`.
#[must_use]
pub fn emit(destination: Destination, text: &str) -> Emitted {
    Emitted {
        destination,
        text: secrets::scrub_str(text),
    }
}

/// Scrub `bytes` for `destination`.
///
/// Separate from [`emit`] because `--trace-wire` and the history file carry bytes that are not
/// text, and forcing them through UTF-8 to scrub them would corrupt exactly the capture
/// somebody turned tracing on to look at.
#[must_use]
pub fn emit_bytes(destination: Destination, bytes: &[u8]) -> EmittedBytes {
    EmittedBytes {
        destination,
        bytes: secrets::scrub(bytes),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn every_destination_scrubs() {
        let secret = "sink-test-secret-4f21";
        assert!(secrets::remember(secret.as_bytes()));
        for d in Destination::ALL {
            let out = emit(*d, &format!("before {secret} after"));
            assert!(
                !out.text().contains(secret),
                "{} did not scrub: {}",
                d.name(),
                out.text()
            );
            assert!(out.text().starts_with("before "));
            let raw = emit_bytes(*d, format!("x{secret}y").as_bytes());
            assert!(!secrets::appears(raw.bytes()), "{} (bytes)", d.name());
        }
    }

    #[test]
    fn the_ten_paths_are_all_named() {
        // §23.4 names ten. If a variant is added, this says so — the point being that the
        // count is a claim made in the report and the report should not be able to drift.
        assert_eq!(Destination::ALL.len(), 10);
        let mut names: Vec<&str> = Destination::ALL.iter().map(|d| d.name()).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), 10, "two destinations share a name");
    }

    #[test]
    fn emitted_bytes_are_otherwise_untouched() {
        let raw: Vec<u8> = (0u8..=255).collect();
        let out = emit_bytes(Destination::TraceWire, &raw);
        assert_eq!(
            out.bytes(),
            &raw[..],
            "scrubbing changed bytes it should not"
        );
    }
}
