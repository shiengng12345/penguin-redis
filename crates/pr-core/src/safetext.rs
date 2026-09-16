//! `SafeText` — the single display trust boundary (v2.1 §23.5, §12.9, ADR-034 pending).
//!
//! **Every** string that originates outside Penguin — values, keys, fields, members, channels,
//! patterns, consumer/group names, `CLIENT LIST` fields, `INFO` values, server error text,
//! `COMMAND DOCS` text, module metadata, imported profile names — must be converted to
//! [`SafeText`] before it reaches any renderer, menu, status line, log or clipboard.
//!
//! Only `--raw`, `--bytes` and `--output resp` may bypass this (v2.1 §18.2), and they warn
//! when writing to a TTY.
//!
//! Renderer and TUI widget APIs take `SafeText`, never `&str`/`String`, so bypassing is a
//! compile error rather than a review miss.

use bytes::Bytes;
use std::fmt;

/// Display-safe text plus a reference to the original bytes.
///
/// Construction escapes anything that could drive a terminal: C0/C1 control characters,
/// `ESC` sequence introducers, OSC/DCS/APC/SOS/PM strings, and lone surrogates arriving as
/// WTF-8. The escaped form is what gets printed; [`SafeText::raw`] returns the untouched
/// bytes for `:copy`, byte views and exact re-insertion.
#[derive(Clone, PartialEq, Eq)]
pub struct SafeText {
    escaped: String,
    raw: Bytes,
    escaped_any: bool,
}

/// C1 range that terminals interpret as control introducers (0x80..=0x9F).
const C1_START: u32 = 0x80;
const C1_END: u32 = 0x9F;

fn push_escaped_byte(out: &mut String, b: u8) {
    use std::fmt::Write as _;
    // `\xHH` — deliberately not `\u{..}` so the form round-trips through §3.2 quoting.
    let _ = write!(out, "\\x{b:02x}");
}

/// True if `c` must never be emitted verbatim to a terminal.
#[must_use]
pub fn is_dangerous_char(c: char) -> bool {
    let u = c as u32;
    // C0 except the three whitespace characters a renderer lays out itself
    if u < 0x20 && c != '\t' && c != '\n' && c != '\r' {
        return true;
    }
    if u == 0x7f {
        return true; // DEL
    }
    if (C1_START..=C1_END).contains(&u) {
        return true; // C1: includes CSI 0x9b, OSC 0x9d, ST 0x9c
    }
    // Bidi overrides/isolates can reorder a line and disguise a key name.
    matches!(u, 0x200E | 0x200F | 0x202A..=0x202E | 0x2066..=0x2069)
}

impl SafeText {
    /// Build from raw bytes, escaping anything terminal-dangerous and any invalid UTF-8.
    #[must_use]
    pub fn from_bytes(raw: impl Into<Bytes>) -> Self {
        let raw: Bytes = raw.into();
        let mut escaped = String::with_capacity(raw.len());
        let mut escaped_any = false;
        let mut rest: &[u8] = &raw;
        loop {
            match std::str::from_utf8(rest) {
                Ok(s) => {
                    escaped_any |= Self::push_str_escaped(&mut escaped, s);
                    break;
                }
                Err(e) => {
                    let (good, bad) = rest.split_at(e.valid_up_to());
                    if let Ok(s) = std::str::from_utf8(good) {
                        escaped_any |= Self::push_str_escaped(&mut escaped, s);
                    }
                    let skip = e.error_len().unwrap_or(bad.len()).max(1);
                    for b in &bad[..skip.min(bad.len())] {
                        push_escaped_byte(&mut escaped, *b);
                        escaped_any = true;
                    }
                    if skip >= bad.len() {
                        break;
                    }
                    rest = &bad[skip..];
                }
            }
        }
        Self {
            escaped,
            raw,
            escaped_any,
        }
    }

    /// Build from a `&str` (still escaped — a Rust string can hold `\u{1b}`).
    #[must_use]
    pub fn from_str_escaped(s: &str) -> Self {
        Self::from_bytes(Bytes::copy_from_slice(s.as_bytes()))
    }

    /// Text Penguin itself authored (headers, labels). Not escaped, because it never
    /// contains untrusted input; use [`SafeText::from_bytes`] for anything from a server.
    #[must_use]
    pub fn trusted(s: impl Into<String>) -> Self {
        let s = s.into();
        let raw = Bytes::copy_from_slice(s.as_bytes());
        Self {
            escaped: s,
            raw,
            escaped_any: false,
        }
    }

    fn push_str_escaped(out: &mut String, s: &str) -> bool {
        let mut any = false;
        for c in s.chars() {
            if is_dangerous_char(c) {
                let mut buf = [0u8; 4];
                for b in c.encode_utf8(&mut buf).as_bytes() {
                    push_escaped_byte(out, *b);
                }
                any = true;
            } else {
                out.push(c);
            }
        }
        any
    }

    /// The display form: safe to print to any terminal.
    #[must_use]
    pub fn as_display(&self) -> &str {
        &self.escaped
    }

    /// The original bytes, unmodified.
    #[must_use]
    pub fn raw(&self) -> &Bytes {
        &self.raw
    }

    /// Whether escaping changed anything (renderers may annotate such cells).
    #[must_use]
    pub fn was_escaped(&self) -> bool {
        self.escaped_any
    }

    /// Display width in terminal columns, per the configured policy (v2.1 §14.6).
    /// Phase 0 uses the conservative narrow-ambiguous default; the full policy lands with V-C04.
    #[must_use]
    pub fn display_len(&self) -> usize {
        self.escaped.chars().count()
    }

    /// True when nothing was captured.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.raw.is_empty()
    }
}

impl fmt::Display for SafeText {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.escaped)
    }
}

/// `Debug` shows the escaped form too, so a stray `{:?}` in a log cannot leak an escape
/// sequence into the terminal.
impl fmt::Debug for SafeText {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SafeText({:?})", self.escaped)
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn d(b: &[u8]) -> String {
        SafeText::from_bytes(Bytes::copy_from_slice(b))
            .as_display()
            .to_owned()
    }

    #[test]
    fn escapes_osc_title_and_clipboard() {
        // SEC-05: a value must not be able to set the title or write the clipboard.
        assert_eq!(d(b"\x1b]0;evil\x07"), "\\x1b]0;evil\\x07");
        assert_eq!(d(b"\x1b]52;c;ZXZpbA==\x07"), "\\x1b]52;c;ZXZpbA==\\x07");
    }

    #[test]
    fn escapes_csi_and_c1_forms() {
        assert_eq!(d(b"\x1b[2J"), "\\x1b[2J");
        // C1 CSI (0x9b) and C1 ST (0x9c) are UTF-8 encoded in a Rust str
        assert_eq!(d("\u{9b}2J".as_bytes()), "\\xc2\\x9b2J");
        assert_eq!(d(b"\x07\x08\x0c"), "\\x07\\x08\\x0c");
        assert_eq!(d(b"\x7f"), "\\x7f");
    }

    #[test]
    fn keeps_layout_whitespace_and_normal_text() {
        assert_eq!(d(b"a\tb\nc\r\n"), "a\tb\nc\r\n");
        assert_eq!(d("player:10001".as_bytes()), "player:10001");
        assert_eq!(d("中文 🐧".as_bytes()), "中文 🐧");
        assert!(!SafeText::from_bytes(Bytes::from_static(b"plain")).was_escaped());
    }

    #[test]
    fn escapes_bidi_overrides() {
        // A key name that visually reverses the rest of the line.
        assert_eq!(d("\u{202e}gnp.exe".as_bytes()), "\\xe2\\x80\\xaegnp.exe");
        assert!(SafeText::from_bytes(Bytes::from_static("\u{202e}".as_bytes())).was_escaped());
    }

    #[test]
    fn invalid_utf8_is_escaped_bytewise_and_raw_is_preserved() {
        let raw = Bytes::from_static(b"ab\xffcd\xc3");
        let t = SafeText::from_bytes(raw.clone());
        assert_eq!(t.as_display(), "ab\\xffcd\\xc3");
        assert_eq!(t.raw(), &raw); // DATA-01: bytes round-trip
        assert!(t.was_escaped());
    }

    #[test]
    fn nul_is_escaped_but_retained_in_raw() {
        let t = SafeText::from_bytes(Bytes::from_static(b"a\0b"));
        assert_eq!(t.as_display(), "a\\x00b");
        assert_eq!(t.raw().as_ref(), b"a\0b");
    }

    #[test]
    fn debug_never_emits_escape_bytes() {
        let t = SafeText::from_bytes(Bytes::from_static(b"\x1b]0;x\x07"));
        let s = format!("{t:?}");
        assert!(!s.contains('\x1b'), "Debug leaked ESC: {s:?}");
        assert!(!s.contains('\x07'));
    }

    #[test]
    fn display_never_emits_escape_bytes_for_whole_corpus() {
        // Every byte value, alone and in pairs with ESC.
        for b in 0u8..=255 {
            let t = SafeText::from_bytes(Bytes::copy_from_slice(&[0x1b, b]));
            let s = t.as_display();
            assert!(!s.contains('\x1b'), "byte {b:#04x} leaked ESC");
            assert!(
                !s.chars().any(is_dangerous_char),
                "byte {b:#04x} leaked a dangerous char"
            );
        }
    }

    #[test]
    fn trusted_text_is_not_escaped() {
        let t = SafeText::trusted("FIELD");
        assert_eq!(t.as_display(), "FIELD");
        assert!(!t.was_escaped());
    }
}
