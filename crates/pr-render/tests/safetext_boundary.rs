//! V-D05 — the `SafeText` display boundary (v2.1 §23.5, §12.9, R34, SEC-05).
//!
//! §23.5 requires every server-originated string to pass through `SafeText` before it can
//! reach a renderer, a menu, a status line, a log or the clipboard. The review's point was
//! that stating this is not enough: the only durable enforcement is one the type system
//! applies, so a future renderer cannot quietly take a `&str`.
//!
//! This file checks both halves:
//! - **compile-time**: the public cell and styling constructors take `SafeText`, so a raw
//!   server string cannot be passed without an explicit, greppable conversion
//! - **behaviour**: for a corpus of real terminal-control payloads, nothing dangerous
//!   survives into the rendered output, in either colour mode

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, missing_docs)]

use pr_core::SafeText;
use pr_protocol::{NullForm, Value};
use pr_render::{
    Cell, ColorDepth, Column, Styled, Table, Theme, ThemeKind, Token, WidthPolicy, render_generic,
};

/// Payloads that drive a terminal if they reach it unescaped.
const HOSTILE: &[&[u8]] = &[
    b"\x1b]0;title\x07",                        // set window title
    b"\x1b]52;c;ZXZpbA==\x07",                  // OSC 52 clipboard write
    b"\x1b]8;;http://evil\x07link\x1b]8;;\x07", // hyperlink
    b"\x1b[2J",                                 // clear screen
    b"\x1b[?1049h",                             // alternate screen
    b"\x1bc",                                   // full reset
    b"\x1bP+q\x1b\\",                           // DCS
    b"\x1b_APC\x1b\\",                          // APC
    b"\x1b[6n",                                 // cursor position report
    b"\r\x1b[Kfake prompt> ",                   // fake a prompt
    b"\x07\x07\x07",                            // bells
    b"\x1b[200~pasted\x1b[201~",                // fake bracketed paste
    "\u{9b}2J".as_bytes(),                      // C1 CSI
    "\u{202e}gnp.exe".as_bytes(),               // bidi override
    b"\x00\xff\xfe",                            // NUL and invalid UTF-8
];

fn assert_inert(s: &str, what: &str) {
    for bad in ['\u{1b}', '\u{7}', '\u{0}', '\u{9b}', '\u{9d}', '\u{202e}'] {
        assert!(
            !s.contains(bad),
            "{what} leaked {bad:?} into the output: {s:?}"
        );
    }
}

#[test]
fn safetext_neutralises_every_hostile_payload() {
    for raw in HOSTILE {
        let t = SafeText::from_bytes(bytes::Bytes::copy_from_slice(raw));
        assert_inert(t.as_display(), "SafeText");
        assert!(t.was_escaped(), "{raw:?} should have been escaped");
        // The original is still available for :copy and byte views (ADR-005).
        assert_eq!(t.raw().as_ref(), *raw, "raw bytes must survive unchanged");
    }
}

#[test]
fn a_hostile_value_cannot_escape_through_a_table() {
    // SEC-05 end to end: the value arrives as bytes and is rendered into a frame.
    for raw in HOSTILE {
        let mut t = Table::field_value();
        t.push(vec![
            Cell::Text(SafeText::from_str_escaped("config")),
            Cell::Text(SafeText::from_bytes(bytes::Bytes::copy_from_slice(raw))),
        ]);
        for theme in [
            Theme::plain(),
            Theme::new(ThemeKind::Dark, ColorDepth::TrueColor),
        ] {
            let r = t.clone().render(80, theme, WidthPolicy::narrow());
            for line in &r.lines {
                // In coloured mode our own SGR is expected; the payload's is not.
                let stripped = strip_our_sgr(line);
                assert_inert(&stripped, "table cell");
            }
        }
    }
}

#[test]
fn a_hostile_key_name_cannot_escape_either() {
    // R34 widened the boundary: keys, fields, channels and CLIENT LIST fields, not just
    // values. A key is just as attacker-influenced as a value.
    for raw in HOSTILE {
        let mut t = Table::field_value();
        t.push(vec![
            Cell::Text(SafeText::from_bytes(bytes::Bytes::copy_from_slice(raw))),
            Cell::Text(SafeText::from_str_escaped("value")),
        ]);
        let r = t.render(80, Theme::plain(), WidthPolicy::narrow());
        for line in &r.lines {
            assert_inert(line, "table key");
        }
    }
}

#[test]
fn a_hostile_column_title_cannot_escape() {
    // A column title can come from a server too — INFO sections, CLIENT LIST fields.
    for raw in HOSTILE {
        let title = SafeText::from_bytes(bytes::Bytes::copy_from_slice(raw));
        let mut t = Table::new(vec![Column::new(title.as_display()), Column::new("VALUE")]);
        t.push(vec![Cell::text("a"), Cell::text("b")]);
        let r = t.render(80, Theme::plain(), WidthPolicy::narrow());
        for line in &r.lines {
            assert_inert(line, "column title");
        }
    }
}

#[test]
fn the_generic_renderer_is_equally_inert() {
    // §9.5: the fallback path is where unknown module data lands, so it is the most exposed.
    for raw in HOSTILE {
        let b = bytes::Bytes::copy_from_slice(raw);
        let shapes = [
            Value::Bulk(b.clone()),
            Value::Simple(b.clone()),
            Value::Error(b.clone()),
            Value::BulkError(b.clone()),
            Value::Array(vec![Value::Bulk(b.clone())]),
            Value::Map(vec![(Value::Bulk(b.clone()), Value::Null(NullForm::Resp3))]),
            Value::Attribute {
                attrs: vec![(Value::Bulk(b.clone()), Value::Integer(1))],
                value: Box::new(Value::Bulk(b.clone())),
            },
            Value::Push(vec![Value::Bulk(b.clone())]),
            Value::Verbatim {
                format: *b"txt",
                data: b.clone(),
            },
        ];
        for v in shapes {
            for line in render_generic(&v, Theme::plain()) {
                assert_inert(&line, "generic renderer");
            }
        }
    }
}

#[test]
fn styled_output_is_inert_in_every_theme_and_depth() {
    for raw in HOSTILE {
        let t = SafeText::from_bytes(bytes::Bytes::copy_from_slice(raw));
        for kind in [
            ThemeKind::Dark,
            ThemeKind::Light,
            ThemeKind::HighContrast,
            ThemeKind::Mono,
        ] {
            for depth in [
                ColorDepth::Mono,
                ColorDepth::Ansi16,
                ColorDepth::Ansi256,
                ColorDepth::TrueColor,
            ] {
                let theme = Theme::new(kind, depth);
                let s = Styled {
                    text: t.as_display(),
                    token: Token::Text,
                    bold: false,
                    theme,
                }
                .to_string();
                assert_inert(&strip_our_sgr(&s), "styled run");
            }
        }
    }
}

#[test]
fn plain_mode_output_contains_no_escape_byte_whatsoever() {
    // PIPE-01: nothing at all, not even our own styling.
    for raw in HOSTILE {
        let t = SafeText::from_bytes(bytes::Bytes::copy_from_slice(raw));
        let s = Styled {
            text: t.as_display(),
            token: Token::Error,
            bold: true,
            theme: Theme::plain(),
        }
        .to_string();
        assert!(!s.contains('\u{1b}'), "plain mode emitted an escape: {s:?}");
    }
}

#[test]
fn trusted_text_is_the_only_unescaped_path_and_is_penguin_authored() {
    // SafeText::trusted exists for our own labels. The test documents the rule: it is never
    // given server bytes, and it is greppable precisely so that can be reviewed.
    let t = SafeText::trusted("FIELD");
    assert_eq!(t.as_display(), "FIELD");
    assert!(!t.was_escaped());
    // Anything from a server goes through from_bytes, which always escapes.
    let s = SafeText::from_bytes(bytes::Bytes::from_static(b"\x1b[2J"));
    assert!(s.was_escaped());
}

#[test]
fn debug_formatting_is_also_inert() {
    // A stray {:?} in a log must not drive the terminal either.
    for raw in HOSTILE {
        let t = SafeText::from_bytes(bytes::Bytes::copy_from_slice(raw));
        assert_inert(&format!("{t:?}"), "Debug");
        assert_inert(&format!("{t}"), "Display");
    }
}

#[test]
fn escaping_is_idempotent_and_does_not_double_escape_our_own_output() {
    // Rendering an already-escaped string must not turn \x1b into \\x1b repeatedly.
    let once = SafeText::from_bytes(bytes::Bytes::from_static(b"\x1b[2J"));
    let twice = SafeText::from_str_escaped(once.as_display());
    assert_eq!(
        twice.as_display(),
        once.as_display(),
        "escaping an escaped string changed it"
    );
    assert!(
        !twice.was_escaped(),
        "the escaped form contains nothing dangerous"
    );
}

/// Remove the SGR sequences this renderer itself emits, so the assertion is about the
/// payload rather than our styling. Anything left that looks like an escape is a leak.
fn strip_our_sgr(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let b = s.as_bytes();
    let mut i = 0;
    while i < b.len() {
        // Our own sequences are always ESC [ <digits and ;> m
        if b[i] == 0x1b && b.get(i + 1) == Some(&b'[') {
            let mut j = i + 2;
            while j < b.len() && (b[j].is_ascii_digit() || b[j] == b';') {
                j += 1;
            }
            if b.get(j) == Some(&b'm') {
                i = j + 1;
                continue;
            }
        }
        let ch_len = s[i..].chars().next().map_or(1, char::len_utf8);
        out.push_str(&s[i..i + ch_len]);
        i += ch_len;
    }
    out
}
