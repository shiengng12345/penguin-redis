//! Generic RESP tree rendering (v2.1 §9.5, ADR-005, V-E05).
//!
//! When no specialised renderer recognises a reply — an unknown command, a module type, a
//! newer server's richer shape — the answer is a faithful generic tree, **not** a panic, a
//! blank screen, or a re-executed command (§9.5, CMD-10/12, SEC-07).
//!
//! What "faithful" means here:
//! - a double keeps its lexeme, so `1.2300` is shown as sent (ADR-005)
//! - a map keeps its wire order and its duplicate keys
//! - a bulk string that is not UTF-8 is shown escaped with its byte length, never lossily
//!   decoded into replacement characters
//! - every string goes through `SafeText`, so a module's metadata cannot drive the terminal

use crate::theme::{Styled, Theme, Token};
use pr_core::SafeText;
use pr_protocol::{NullForm, Value};

/// How deep to draw before summarising, so a hostile reply cannot produce endless output.
pub const MAX_RENDER_DEPTH: usize = 32;

/// Render a value as an indented tree.
#[must_use]
pub fn render(v: &Value, theme: Theme) -> Vec<String> {
    let mut out = Vec::new();
    walk(v, 0, &mut out, theme);
    out
}

fn indent(depth: usize) -> String {
    "  ".repeat(depth)
}

fn styled(text: &str, token: Token, theme: Theme) -> String {
    Styled {
        text,
        token,
        bold: false,
        theme,
    }
    .to_string()
}

/// A scalar's one-line form: type tag plus value.
fn scalar(v: &Value, theme: Theme) -> Option<String> {
    let s = match v {
        Value::Simple(b) => {
            let t = SafeText::from_bytes(b.clone());
            format!(
                "{} {}",
                styled("(status)", Token::Muted, theme),
                styled(t.as_display(), Token::Text, theme)
            )
        }
        Value::Error(b) | Value::BulkError(b) => {
            let t = SafeText::from_bytes(b.clone());
            format!(
                "{} {}",
                styled("(error)", Token::Muted, theme),
                styled(t.as_display(), Token::Error, theme)
            )
        }
        Value::Integer(i) => format!(
            "{} {}",
            styled("(integer)", Token::Muted, theme),
            styled(&i.to_string(), Token::JsonNumber, theme)
        ),
        // The lexeme is shown exactly as it arrived; parsing it to f64 would turn 1.2300
        // into 1.23 and -0 into 0 (ADR-005).
        Value::Double(lex) => {
            let t = SafeText::from_bytes(lex.clone());
            format!(
                "{} {}",
                styled("(double)", Token::Muted, theme),
                styled(t.as_display(), Token::JsonNumber, theme)
            )
        }
        Value::BigNumber(d) => {
            let t = SafeText::from_bytes(d.clone());
            format!(
                "{} {}",
                styled("(big number)", Token::Muted, theme),
                styled(t.as_display(), Token::JsonNumber, theme)
            )
        }
        Value::Boolean(b) => format!(
            "{} {}",
            styled("(boolean)", Token::Muted, theme),
            styled(if *b { "true" } else { "false" }, Token::JsonBool, theme)
        ),
        Value::Null(form) => {
            let tag = match form {
                NullForm::Bulk => "(nil)",
                NullForm::Array => "(nil array)",
                NullForm::Resp3 => "(null)",
            };
            styled(tag, Token::JsonNull, theme)
        }
        Value::Bulk(b) => {
            let t = SafeText::from_bytes(b.clone());
            if t.was_escaped() {
                // Say how many bytes there were, because the escaped form is longer than the
                // data and a reader needs the real size.
                format!(
                    "{} {}",
                    styled(&format!("({} bytes)", b.len()), Token::Muted, theme),
                    styled(t.as_display(), Token::JsonString, theme)
                )
            } else {
                styled(t.as_display(), Token::JsonString, theme)
            }
        }
        Value::Verbatim { format, data } => {
            let tag = String::from_utf8_lossy(format).into_owned();
            let t = SafeText::from_bytes(data.clone());
            format!(
                "{} {}",
                styled(&format!("(verbatim {tag})"), Token::Muted, theme),
                styled(t.as_display(), Token::JsonString, theme)
            )
        }
        _ => return None,
    };
    Some(s)
}

fn walk(v: &Value, depth: usize, out: &mut Vec<String>, theme: Theme) {
    if depth > MAX_RENDER_DEPTH {
        out.push(format!(
            "{}{}",
            indent(depth),
            styled(
                "(nesting beyond render depth; use --output resp)",
                Token::Warning,
                theme
            )
        ));
        return;
    }
    if let Some(line) = scalar(v, theme) {
        out.push(format!("{}{line}", indent(depth)));
        return;
    }
    match v {
        Value::Array(items) | Value::Set(items) | Value::Push(items) => {
            let tag = match v {
                Value::Set(_) => "set",
                Value::Push(_) => "push",
                _ => "array",
            };
            out.push(format!(
                "{}{}",
                indent(depth),
                styled(
                    &format!("({tag}, {} items)", items.len()),
                    Token::Muted,
                    theme
                )
            ));
            for (i, item) in items.iter().enumerate() {
                out.push(format!(
                    "{}{}",
                    indent(depth + 1),
                    styled(&format!("{}:", i + 1), Token::Muted, theme)
                ));
                walk(item, depth + 2, out, theme);
            }
        }
        Value::Map(kv) => {
            out.push(format!(
                "{}{}",
                indent(depth),
                styled(&format!("(map, {} entries)", kv.len()), Token::Muted, theme)
            ));
            for (k, val) in kv {
                // Keys are rendered as values, because a RESP map key need not be a string.
                let key_line = scalar(k, theme)
                    .unwrap_or_else(|| styled("(aggregate key)", Token::Muted, theme));
                out.push(format!("{}{key_line}", indent(depth + 1)));
                walk(val, depth + 2, out, theme);
            }
        }
        Value::Attribute { attrs, value } => {
            // Attributes are shown, never silently dropped (§19.3).
            out.push(format!(
                "{}{}",
                indent(depth),
                styled(
                    &format!("(attribute, {} entries)", attrs.len()),
                    Token::Muted,
                    theme
                )
            ));
            for (k, val) in attrs {
                let key_line = scalar(k, theme)
                    .unwrap_or_else(|| styled("(aggregate key)", Token::Muted, theme));
                out.push(format!("{}{key_line}", indent(depth + 1)));
                walk(val, depth + 2, out, theme);
            }
            walk(value, depth, out, theme);
        }
        _ => {
            // Unreachable: scalar() handled every remaining variant. Emitting a marker keeps
            // this total rather than panicking on a future variant (SEC-07).
            out.push(format!(
                "{}{}",
                indent(depth),
                styled("(unrenderable)", Token::Warning, theme)
            ));
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use bytes::Bytes;

    fn plain() -> Theme {
        Theme::plain()
    }
    fn lines(v: &Value) -> Vec<String> {
        render(v, plain())
    }
    fn joined(v: &Value) -> String {
        lines(v).join("\n")
    }

    #[test]
    fn scalars_carry_a_type_tag() {
        assert_eq!(
            joined(&Value::Simple(Bytes::from_static(b"OK"))),
            "(status) OK"
        );
        assert_eq!(joined(&Value::Integer(-5)), "(integer) -5");
        assert_eq!(joined(&Value::Boolean(true)), "(boolean) true");
        assert_eq!(joined(&Value::Null(NullForm::Resp3)), "(null)");
        assert_eq!(joined(&Value::Null(NullForm::Bulk)), "(nil)");
        assert_eq!(joined(&Value::Null(NullForm::Array)), "(nil array)");
        assert_eq!(joined(&Value::Bulk(Bytes::from_static(b"hello"))), "hello");
    }

    #[test]
    fn a_double_keeps_its_lexeme() {
        // ADR-005: rendering must not normalise what the server sent.
        for lex in ["1.2300", "-0", "1e3", "inf", "nan"] {
            let v = Value::Double(Bytes::copy_from_slice(lex.as_bytes()));
            assert_eq!(joined(&v), format!("(double) {lex}"), "{lex} was rewritten");
        }
    }

    #[test]
    fn a_big_number_keeps_every_digit() {
        let n = "9".repeat(100);
        let v = Value::BigNumber(Bytes::copy_from_slice(n.as_bytes()));
        assert_eq!(joined(&v), format!("(big number) {n}"));
    }

    #[test]
    fn non_utf8_is_escaped_and_its_byte_length_reported() {
        // Lossy decoding would replace these with U+FFFD and lose the size.
        let v = Value::Bulk(Bytes::from_static(b"\xff\xfe\x00"));
        let s = joined(&v);
        assert!(s.contains("(3 bytes)"), "{s}");
        assert!(s.contains("\\xff"), "{s}");
        assert!(!s.contains('\u{fffd}'), "must not lossily decode");
    }

    #[test]
    fn a_utf8_bulk_needs_no_byte_annotation() {
        assert_eq!(
            joined(&Value::Bulk(Bytes::from_static("中文".as_bytes()))),
            "中文"
        );
    }

    #[test]
    fn control_bytes_cannot_reach_the_terminal() {
        // SEC-07 / SEC-05: module metadata is untrusted.
        let v = Value::Bulk(Bytes::from_static(b"\x1b]0;evil\x07"));
        let s = joined(&v);
        assert!(!s.contains('\u{1b}'), "escape leaked: {s:?}");
        assert!(s.contains("\\x1b"));
    }

    #[test]
    fn arrays_report_their_length_and_index_their_items() {
        let v = Value::Array(vec![
            Value::Integer(1),
            Value::Bulk(Bytes::from_static(b"x")),
        ]);
        let s = joined(&v);
        assert!(s.starts_with("(array, 2 items)"));
        assert!(s.contains("1:"));
        assert!(s.contains("2:"));
        assert!(s.contains("(integer) 1"));
    }

    #[test]
    fn sets_and_pushes_are_labelled_distinctly() {
        assert!(joined(&Value::Set(vec![Value::Integer(1)])).starts_with("(set, 1 items)"));
        assert!(joined(&Value::Push(vec![Value::Integer(1)])).starts_with("(push, 1 items)"));
    }

    #[test]
    fn a_map_preserves_order_and_duplicate_keys() {
        // ADR-005: a HashMap would collapse these and change the answer.
        let v = Value::Map(vec![
            (Value::Bulk(Bytes::from_static(b"a")), Value::Integer(1)),
            (Value::Bulk(Bytes::from_static(b"a")), Value::Integer(2)),
        ]);
        let s = joined(&v);
        assert!(s.starts_with("(map, 2 entries)"));
        let one = s.find("(integer) 1").unwrap();
        let two = s.find("(integer) 2").unwrap();
        assert!(one < two, "wire order must be preserved");
        // Both entries are present, not one collapsed over the other. Counted as whole key
        // lines, since a bare substring search also matches the "a" in "(map, ...)".
        let key_lines = lines(&v).iter().filter(|l| l.trim() == "a").count();
        assert_eq!(key_lines, 2, "both duplicate keys must be rendered: {s}");
    }

    #[test]
    fn a_map_key_need_not_be_a_string() {
        let v = Value::Map(vec![(
            Value::Integer(7),
            Value::Bulk(Bytes::from_static(b"v")),
        )]);
        let s = joined(&v);
        assert!(s.contains("(integer) 7"), "{s}");
    }

    #[test]
    fn attributes_are_shown_not_dropped() {
        // §19.3: an attribute silently discarded is information lost.
        let v = Value::Attribute {
            attrs: vec![(Value::Simple(Bytes::from_static(b"ttl")), Value::Integer(3))],
            value: Box::new(Value::Simple(Bytes::from_static(b"OK"))),
        };
        let s = joined(&v);
        assert!(s.contains("(attribute, 1 entries)"), "{s}");
        assert!(s.contains("ttl"));
        assert!(
            s.contains("(status) OK"),
            "the decorated value is still rendered"
        );
    }

    #[test]
    fn deep_nesting_is_summarised_rather_than_rendered_forever() {
        // A hostile or module reply must not produce unbounded output.
        let mut v = Value::Integer(1);
        for _ in 0..MAX_RENDER_DEPTH + 10 {
            v = Value::Array(vec![v]);
        }
        let out = lines(&v);
        assert!(
            out.iter().any(|l| l.contains("beyond render depth")),
            "no depth guard fired"
        );
        assert!(
            out.len() < 200,
            "output should be bounded, got {}",
            out.len()
        );
    }

    #[test]
    fn an_empty_aggregate_renders_cleanly() {
        assert_eq!(joined(&Value::Array(vec![])), "(array, 0 items)");
        assert_eq!(joined(&Value::Map(vec![])), "(map, 0 entries)");
    }

    #[test]
    fn nothing_panics_on_any_shape() {
        // CMD-12 / SEC-07: an unrecognised result must fall back, never abort.
        let shapes = vec![
            Value::Array(vec![Value::Map(vec![(
                Value::Set(vec![Value::Integer(1)]),
                Value::Push(vec![Value::Null(NullForm::Resp3)]),
            )])]),
            Value::Attribute {
                attrs: vec![],
                value: Box::new(Value::Array(vec![])),
            },
            Value::Verbatim {
                format: *b"mkd",
                data: Bytes::from_static(b"# hi"),
            },
            Value::BulkError(Bytes::from_static(b"ERR \x1b[2J")),
        ];
        for v in shapes {
            let out = lines(&v);
            assert!(!out.is_empty());
            for l in &out {
                assert!(!l.contains('\u{1b}'), "escape leaked from {v:?}: {l:?}");
            }
        }
    }

    #[test]
    fn verbatim_shows_its_format_tag() {
        let v = Value::Verbatim {
            format: *b"txt",
            data: Bytes::from_static(b"hi"),
        };
        assert_eq!(joined(&v), "(verbatim txt) hi");
    }

    #[test]
    fn plain_mode_emits_no_escapes_anywhere() {
        let v = Value::Array(vec![
            Value::Double(Bytes::from_static(b"1.2300")),
            Value::Map(vec![(
                Value::Bulk(Bytes::from_static(b"k")),
                Value::Boolean(false),
            )]),
        ]);
        for l in lines(&v) {
            assert!(!l.contains('\u{1b}'));
        }
    }
}
