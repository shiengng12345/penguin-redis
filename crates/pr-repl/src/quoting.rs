//! The authoritative request tokenizer (v2.1 §3.2, §11.5, ADR-027, R04/R11).
//!
//! This is the *only* thing that turns an input line into argv bytes. It mirrors
//! `redis-cli`'s `sdssplitargs`, because a command typed into `prc` must produce the same
//! bytes it would produce in `redis-cli` — that is what §3.4 compatibility means.
//!
//! Escapes exist because the editor holds UTF-8 text (ADR-027): `"\xff"` is the only way to
//! put a non-UTF-8 byte into an argument from the keyboard.

use bytes::Bytes;
use thiserror::Error;

/// Tokenizer failures.
#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum TokenizeError {
    /// A quote was opened and never closed. While editing this is normal and the lenient
    /// analyser reports it as incomplete; on submit it is an error.
    #[error("unbalanced quotes")]
    UnbalancedQuotes,
    /// A closing quote must be followed by whitespace or end of input.
    #[error("closing quote must be followed by a space")]
    QuoteNotFollowedBySpace,
}

/// One token with the byte span it came from, so a completion can replace exactly this span
/// (this span/argv correspondence is the equivalence invariant of §11.5).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Token {
    /// Decoded bytes of the argument.
    pub bytes: Bytes,
    /// Byte range in the source line, including any quotes.
    pub span: std::ops::Range<usize>,
}

fn hex(c: u8) -> Option<u8> {
    match c {
        b'0'..=b'9' => Some(c - b'0'),
        b'a'..=b'f' => Some(c - b'a' + 10),
        b'A'..=b'F' => Some(c - b'A' + 10),
        _ => None,
    }
}

/// Split a line into argv, exactly as `redis-cli` does.
///
/// Rules, matching `sdssplitargs`:
/// - outside quotes, whitespace separates tokens
/// - `"..."` supports `\xHH`, `\n`, `\r`, `\t`, `\b`, `\a`, and `\<any>` for the literal char
/// - `'...'` supports only `\'`; everything else is literal
/// - a closing quote must be followed by whitespace or end of input
///
/// # Errors
/// [`TokenizeError`] for unbalanced quotes or a quote not followed by a space.
// A single scanning loop with three lexer states; splitting it would scatter the span
// bookkeeping that §11.5's equivalence invariant depends on.
#[allow(clippy::too_many_lines)]
pub fn split_args(line: &[u8]) -> Result<Vec<Token>, TokenizeError> {
    let mut out = Vec::new();
    let mut i = 0usize;
    loop {
        while i < line.len() && line[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= line.len() {
            return Ok(out);
        }
        let start = i;
        let mut cur: Vec<u8> = Vec::new();
        let mut inside_double = false;
        let mut inside_single = false;
        loop {
            if inside_double {
                let Some(&c) = line.get(i) else {
                    return Err(TokenizeError::UnbalancedQuotes);
                };
                if c == b'\\'
                    && i + 3 < line.len()
                    && line[i + 1] == b'x'
                    && let (Some(h), Some(l)) = (hex(line[i + 2]), hex(line[i + 3]))
                {
                    cur.push(h * 16 + l);
                    i += 4;
                    continue;
                }
                if c == b'\\' {
                    let Some(&e) = line.get(i + 1) else {
                        return Err(TokenizeError::UnbalancedQuotes);
                    };
                    cur.push(match e {
                        b'n' => b'\n',
                        b'r' => b'\r',
                        b't' => b'\t',
                        b'b' => 0x08,
                        b'a' => 0x07,
                        other => other,
                    });
                    i += 2;
                    continue;
                }
                if c == b'"' {
                    // Closing quote must be followed by a space or end of line.
                    match line.get(i + 1) {
                        None => {
                            i += 1;
                            break;
                        }
                        Some(n) if n.is_ascii_whitespace() => {
                            i += 1;
                            break;
                        }
                        Some(_) => return Err(TokenizeError::QuoteNotFollowedBySpace),
                    }
                }
                cur.push(c);
                i += 1;
            } else if inside_single {
                let Some(&c) = line.get(i) else {
                    return Err(TokenizeError::UnbalancedQuotes);
                };
                if c == b'\\' && line.get(i + 1) == Some(&b'\'') {
                    cur.push(b'\'');
                    i += 2;
                    continue;
                }
                if c == b'\'' {
                    match line.get(i + 1) {
                        None => {
                            i += 1;
                            break;
                        }
                        Some(n) if n.is_ascii_whitespace() => {
                            i += 1;
                            break;
                        }
                        Some(_) => return Err(TokenizeError::QuoteNotFollowedBySpace),
                    }
                }
                cur.push(c);
                i += 1;
            } else {
                match line.get(i) {
                    None => break,
                    Some(c) if c.is_ascii_whitespace() => break,
                    Some(b'"') => {
                        inside_double = true;
                        i += 1;
                    }
                    Some(b'\'') => {
                        inside_single = true;
                        i += 1;
                    }
                    Some(&c) => {
                        cur.push(c);
                        i += 1;
                    }
                }
            }
        }
        out.push(Token {
            bytes: Bytes::from(cur),
            span: start..i,
        });
    }
}

/// Whether a line can still be completed by typing more (an unbalanced quote), which the
/// lenient editor analyser reports as "keep going" rather than an error (§11.5).
#[must_use]
pub fn is_incomplete(line: &[u8]) -> bool {
    matches!(split_args(line), Err(TokenizeError::UnbalancedQuotes))
}

/// Quote a raw byte string so that [`split_args`] reproduces it exactly.
///
/// Used by Guide and by completion insertion, so what the user sees and what is sent cannot
/// drift apart (ADR-016).
#[must_use]
pub fn quote(arg: &[u8]) -> String {
    let simple = !arg.is_empty()
        && arg
            .iter()
            .all(|c| c.is_ascii_graphic() && !matches!(c, b'"' | b'\'' | b'\\'));
    if simple {
        return String::from_utf8_lossy(arg).into_owned();
    }
    let mut s = String::with_capacity(arg.len() + 2);
    s.push('"');
    for &c in arg {
        match c {
            b'\\' => s.push_str("\\\\"),
            b'"' => s.push_str("\\\""),
            b'\n' => s.push_str("\\n"),
            b'\r' => s.push_str("\\r"),
            b'\t' => s.push_str("\\t"),
            0x07 => s.push_str("\\a"),
            0x08 => s.push_str("\\b"),
            0x20..=0x7e => s.push(c as char),
            _ => {
                use std::fmt::Write as _;
                let _ = write!(s, "\\x{c:02x}");
            }
        }
    }
    s.push('"');
    s
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn argv(line: &str) -> Vec<Vec<u8>> {
        split_args(line.as_bytes())
            .unwrap()
            .into_iter()
            .map(|t| t.bytes.to_vec())
            .collect()
    }

    #[test]
    fn splits_on_whitespace() {
        assert_eq!(argv("GET key"), vec![b"GET".to_vec(), b"key".to_vec()]);
        assert_eq!(
            argv("  GET   key  "),
            vec![b"GET".to_vec(), b"key".to_vec()]
        );
        assert_eq!(argv(""), Vec::<Vec<u8>>::new());
        assert_eq!(argv("   "), Vec::<Vec<u8>>::new());
        assert_eq!(
            argv("GET\tkey\nx"),
            vec![b"GET".to_vec(), b"key".to_vec(), b"x".to_vec()]
        );
    }

    #[test]
    fn double_quotes_group_and_support_escapes() {
        assert_eq!(
            argv(r#"SET k "a b""#),
            vec![b"SET".to_vec(), b"k".to_vec(), b"a b".to_vec()]
        );
        assert_eq!(
            argv(r#"SET k "a\nb""#),
            vec![b"SET".to_vec(), b"k".to_vec(), b"a\nb".to_vec()]
        );
        assert_eq!(argv(r#"SET k "a\tb""#)[2], b"a\tb".to_vec());
        assert_eq!(argv(r#"SET k "q\"q""#)[2], b"q\"q".to_vec());
        assert_eq!(argv(r#"SET k "back\\slash""#)[2], b"back\\slash".to_vec());
    }

    #[test]
    fn hex_escapes_are_the_route_for_binary_argv() {
        // ADR-027: the only way to type a non-UTF-8 byte or a NUL.
        assert_eq!(argv(r#"SET k "\xff""#)[2], vec![0xff]);
        assert_eq!(argv(r#"SET k "\x00""#)[2], vec![0x00]);
        assert_eq!(argv(r#"SET k "a\x00b""#)[2], vec![b'a', 0, b'b']);
        assert_eq!(argv(r#"SET k "\x1b]0;x\x07""#)[2], b"\x1b]0;x\x07".to_vec());
        // A malformed \x falls back to the literal char, as redis-cli does.
        assert_eq!(argv(r#"SET k "\xzz""#)[2], b"xzz".to_vec());
    }

    #[test]
    fn single_quotes_are_literal_except_escaped_quote() {
        assert_eq!(argv(r"SET k 'a b'")[2], b"a b".to_vec());
        assert_eq!(
            argv(r"SET k 'a\nb'")[2],
            b"a\\nb".to_vec(),
            "no escapes inside single quotes"
        );
        assert_eq!(argv(r"SET k 'it\'s'")[2], b"it's".to_vec());
    }

    #[test]
    fn client_flags_inside_a_command_are_values_not_flags() {
        // CMD-01: `SET example '--raw'` must keep --raw as the value.
        assert_eq!(
            argv("SET example '--raw'"),
            vec![b"SET".to_vec(), b"example".to_vec(), b"--raw".to_vec()]
        );
        assert_eq!(argv("SET example '@prod'")[2], b"@prod".to_vec());
    }

    #[test]
    fn empty_argument_is_preserved() {
        assert_eq!(
            argv(r#"SET k """#),
            vec![b"SET".to_vec(), b"k".to_vec(), Vec::new()]
        );
        assert_eq!(argv("SET k ''")[2], Vec::<u8>::new());
    }

    #[test]
    fn rejects_unbalanced_and_misplaced_quotes() {
        assert_eq!(
            split_args(br#"SET k "abc"#),
            Err(TokenizeError::UnbalancedQuotes)
        );
        assert_eq!(
            split_args(b"SET k 'abc"),
            Err(TokenizeError::UnbalancedQuotes)
        );
        assert_eq!(
            split_args(br#"SET k "abc"d"#),
            Err(TokenizeError::QuoteNotFollowedBySpace)
        );
        assert_eq!(
            split_args(br#"SET k "a\"#),
            Err(TokenizeError::UnbalancedQuotes)
        );
    }

    #[test]
    fn incomplete_is_distinguishable_from_invalid() {
        // §11.5: while editing, an open quote means "keep typing", not "error".
        assert!(is_incomplete(br#"SET k "abc"#));
        assert!(!is_incomplete(br#"SET k "abc""#));
        assert!(
            !is_incomplete(br#"SET k "abc"d"#),
            "that one is genuinely invalid"
        );
    }

    #[test]
    fn spans_cover_the_source_including_quotes() {
        // This is the property completion insertion relies on (§11.5 equivalence).
        let line = br#"HGET player:1 "a b""#;
        let toks = split_args(line).unwrap();
        assert_eq!(toks.len(), 3);
        assert_eq!(&line[toks[0].span.clone()], b"HGET");
        assert_eq!(&line[toks[1].span.clone()], b"player:1");
        assert_eq!(&line[toks[2].span.clone()], br#""a b""#);
        // Replacing a token's span with its re-quoted bytes is a no-op.
        for t in &toks {
            let requoted = quote(&t.bytes);
            let mut rebuilt = line[..t.span.start].to_vec();
            rebuilt.extend_from_slice(requoted.as_bytes());
            rebuilt.extend_from_slice(&line[t.span.end..]);
            let again = split_args(&rebuilt).unwrap();
            let a: Vec<_> = again.iter().map(|x| x.bytes.clone()).collect();
            let b: Vec<_> = toks.iter().map(|x| x.bytes.clone()).collect();
            assert_eq!(a, b, "re-quoting token {:?} changed the argv", t.bytes);
        }
    }

    #[test]
    fn quote_round_trips_every_byte() {
        // The property that makes Guide safe: whatever we render, split_args gives back.
        for b in 0u8..=255 {
            let arg = vec![b];
            let line = format!("CMD {}", quote(&arg));
            let got = split_args(line.as_bytes()).unwrap();
            assert_eq!(got.len(), 2, "byte {b:#04x} produced {} tokens", got.len());
            assert_eq!(
                got[1].bytes.as_ref(),
                &arg[..],
                "byte {b:#04x} did not round-trip"
            );
        }
    }

    #[test]
    fn quote_round_trips_awkward_strings() {
        for s in [
            b"".to_vec(),
            b" ".to_vec(),
            b"a b".to_vec(),
            b"\"".to_vec(),
            b"'".to_vec(),
            b"\\".to_vec(),
            b"a\nb".to_vec(),
            b"\0".to_vec(),
            b"\xff\xfe".to_vec(),
            "中文 🐧".as_bytes().to_vec(),
            b"--raw".to_vec(),
            b"@prod".to_vec(),
            b"\x1b]52;c;x\x07".to_vec(),
        ] {
            let line = format!("CMD {}", quote(&s));
            let got = split_args(line.as_bytes()).unwrap();
            assert_eq!(
                got[1].bytes.as_ref(),
                &s[..],
                "failed for {s:?} rendered as {line}"
            );
        }
    }

    #[test]
    fn multibyte_utf8_passes_through_unquoted() {
        assert_eq!(
            argv("SET 中文 🐧"),
            vec![
                b"SET".to_vec(),
                "中文".as_bytes().to_vec(),
                "🐧".as_bytes().to_vec()
            ]
        );
    }
}
