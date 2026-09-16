//! Grammar for local `:` commands (v2.1 §3.5, ADR-002, R18).
//!
//! The review found that `:find`, `:guide`, `:complete`, `:diff` and the recipe entry points
//! had no defined grammar at all — no quoting, no escaping, no option precedence — so two
//! implementers would have produced different local languages and, worse, different remote
//! scopes for the same `:complete keys --prefix '...'`.
//!
//! This is deliberately **not** the Redis tokenizer: it has options, and it never performs
//! shell expansion.

use bytes::Bytes;
use std::collections::BTreeMap;
use thiserror::Error;

/// Local-command parse failures.
#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum LocalError {
    /// Input did not begin with `:`.
    #[error("not a local command")]
    NotLocal,
    /// No name after the colon.
    #[error("missing command name")]
    MissingName,
    /// A quote was opened and never closed.
    #[error("unbalanced quotes")]
    UnbalancedQuotes,
    /// The same option was given twice. Never silently last-wins (§3.5).
    #[error("option --{0} given more than once")]
    DuplicateOption(String),
    /// An option that requires a value had none.
    #[error("option --{0} needs a value")]
    MissingOptionValue(String),
}

/// A parsed local command.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalCommand {
    /// Name after the colon, lowercased (`inspect`, `complete`, ...).
    pub name: String,
    /// Options, in sorted order. A flag with no value maps to an empty `Bytes`.
    pub options: BTreeMap<String, Bytes>,
    /// Positional arguments, in order.
    pub positionals: Vec<Bytes>,
    /// For `:find` / `:help`, the whole remaining input as one free-text string. Free text is
    /// never re-tokenised, because a task description is not argv.
    pub free_text: Option<String>,
}

/// Commands whose trailing arguments are free text rather than positionals.
const FREE_TEXT_COMMANDS: &[&str] = &["find", "help"];

fn unquote(seg: &[u8], dq: bool) -> Vec<u8> {
    // Escapes inside a local command mirror §3.2 so users learn one escape language.
    let mut out = Vec::with_capacity(seg.len());
    let mut i = 0;
    while i < seg.len() {
        if dq && seg[i] == b'\\' && i + 1 < seg.len() {
            let e = seg[i + 1];
            if e == b'x' && i + 3 < seg.len() {
                let h = |c: u8| -> Option<u8> {
                    match c {
                        b'0'..=b'9' => Some(c - b'0'),
                        b'a'..=b'f' => Some(c - b'a' + 10),
                        b'A'..=b'F' => Some(c - b'A' + 10),
                        _ => None,
                    }
                };
                if let (Some(a), Some(b)) = (h(seg[i + 2]), h(seg[i + 3])) {
                    out.push(a * 16 + b);
                    i += 4;
                    continue;
                }
            }
            out.push(match e {
                b'n' => b'\n',
                b't' => b'\t',
                b'r' => b'\r',
                other => other,
            });
            i += 2;
            continue;
        }
        out.push(seg[i]);
        i += 1;
    }
    out
}

/// Split respecting quotes; returns raw segments with a flag for whether they were
/// double-quoted (and therefore escape-processed).
fn segments(b: &[u8]) -> Result<Vec<Vec<u8>>, LocalError> {
    let mut out = Vec::new();
    let mut i = 0;
    loop {
        while i < b.len() && b[i].is_ascii_whitespace() {
            i += 1;
        }
        if i >= b.len() {
            return Ok(out);
        }
        let mut cur = Vec::new();
        loop {
            match b.get(i) {
                None => break,
                Some(c) if c.is_ascii_whitespace() => break,
                Some(&q @ (b'"' | b'\'')) => {
                    i += 1;
                    let start = i;
                    while i < b.len() && b[i] != q {
                        if q == b'"' && b[i] == b'\\' {
                            i += 1;
                        }
                        i += 1;
                    }
                    if i >= b.len() {
                        return Err(LocalError::UnbalancedQuotes);
                    }
                    cur.extend_from_slice(&unquote(&b[start..i], q == b'"'));
                    i += 1;
                }
                Some(&c) => {
                    cur.push(c);
                    i += 1;
                }
            }
        }
        out.push(cur);
    }
}

/// Parse a local command line beginning with `:`.
///
/// Grammar:
/// - `:name` then options and positionals in any order
/// - `--opt value` or `--opt=value`; a repeated option is an error, never last-wins
/// - `--` ends option parsing; everything after is positional
/// - single quotes are literal, double quotes process the §3.2 escapes
/// - **no** shell expansion: no `$VAR`, `~`, globbing, backticks or command substitution
///
/// # Errors
/// [`LocalError`] describing the first problem.
pub fn parse_local(line: &[u8]) -> Result<LocalCommand, LocalError> {
    let t = line.strip_prefix(b":").ok_or(LocalError::NotLocal)?;
    let segs = segments(t)?;
    let mut it = segs.into_iter();
    let name_raw = it.next().ok_or(LocalError::MissingName)?;
    if name_raw.is_empty() {
        return Err(LocalError::MissingName);
    }
    let name = String::from_utf8_lossy(&name_raw).to_lowercase();
    let free_text_cmd = FREE_TEXT_COMMANDS.contains(&name.as_str());

    let mut options: BTreeMap<String, Bytes> = BTreeMap::new();
    let mut positionals: Vec<Bytes> = Vec::new();
    let mut free: Vec<String> = Vec::new();
    let mut only_positional = false;
    let rest: Vec<Vec<u8>> = it.collect();
    let mut idx = 0usize;

    while idx < rest.len() {
        let seg = &rest[idx];
        if !only_positional && seg == b"--" {
            only_positional = true;
            idx += 1;
            continue;
        }
        if !only_positional && seg.starts_with(b"--") && seg.len() > 2 {
            let body = &seg[2..];
            let (key, inline) = match body.iter().position(|c| *c == b'=') {
                Some(p) => (
                    String::from_utf8_lossy(&body[..p]).into_owned(),
                    Some(Bytes::copy_from_slice(&body[p + 1..])),
                ),
                None => (String::from_utf8_lossy(body).into_owned(), None),
            };
            if options.contains_key(&key) {
                return Err(LocalError::DuplicateOption(key));
            }
            let value = match inline {
                Some(v) => v,
                None => {
                    // A following segment is the value unless it is itself an option.
                    match rest.get(idx + 1) {
                        Some(n) if !n.starts_with(b"--") => {
                            idx += 1;
                            Bytes::copy_from_slice(n)
                        }
                        // Bare flag.
                        _ => Bytes::new(),
                    }
                }
            };
            options.insert(key, value);
            idx += 1;
            continue;
        }
        if free_text_cmd {
            free.push(String::from_utf8_lossy(seg).into_owned());
        } else {
            positionals.push(Bytes::copy_from_slice(seg));
        }
        idx += 1;
    }

    Ok(LocalCommand {
        name,
        options,
        positionals,
        free_text: if free.is_empty() {
            None
        } else {
            Some(free.join(" "))
        },
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn p(s: &str) -> LocalCommand {
        parse_local(s.as_bytes()).unwrap()
    }

    #[test]
    fn parses_name_and_positionals() {
        let c = p(":use r3");
        assert_eq!(c.name, "use");
        assert_eq!(c.positionals, vec![Bytes::from_static(b"r3")]);
        assert!(c.options.is_empty());
        assert_eq!(p(":connections").name, "connections");
    }

    #[test]
    fn name_is_case_insensitive() {
        assert_eq!(p(":INSPECT").name, "inspect");
    }

    #[test]
    fn parses_both_option_forms() {
        for line in [
            ":complete keys --prefix player:",
            ":complete keys --prefix=player:",
        ] {
            let c = parse_local(line.as_bytes()).unwrap();
            assert_eq!(c.name, "complete");
            assert_eq!(
                c.options.get("prefix").map(Bytes::as_ref),
                Some(&b"player:"[..]),
                "{line}"
            );
            assert_eq!(c.positionals, vec![Bytes::from_static(b"keys")]);
        }
    }

    #[test]
    fn bare_flags_have_empty_values() {
        let c = p(":inspect --bytes");
        assert_eq!(c.options.get("bytes").map(Bytes::as_ref), Some(&b""[..]));
    }

    #[test]
    fn duplicate_options_are_an_error_not_last_wins() {
        // §3.5: silently taking the last value is how a scope gets widened by accident.
        assert_eq!(
            parse_local(b":complete keys --prefix a --prefix b"),
            Err(LocalError::DuplicateOption("prefix".into()))
        );
    }

    #[test]
    fn double_dash_ends_option_parsing() {
        let c = p(":send -- GET --raw");
        assert_eq!(c.name, "send");
        assert_eq!(
            c.positionals,
            vec![Bytes::from_static(b"GET"), Bytes::from_static(b"--raw")]
        );
        assert!(c.options.is_empty(), "nothing after -- is an option");
    }

    #[test]
    fn quotes_group_and_escape() {
        let c = p(r#":complete keys --prefix "a b""#);
        assert_eq!(
            c.options.get("prefix").map(Bytes::as_ref),
            Some(&b"a b"[..])
        );
        let c2 = p(r#":complete keys --prefix "x\x00y""#);
        assert_eq!(
            c2.options.get("prefix").map(Bytes::as_ref),
            Some(&b"x\0y"[..])
        );
        let c3 = p(r":complete keys --prefix 'a\nb'");
        assert_eq!(
            c3.options.get("prefix").map(Bytes::as_ref),
            Some(&b"a\\nb"[..]),
            "single quotes literal"
        );
    }

    #[test]
    fn glob_metacharacters_are_passed_through_verbatim() {
        // R32: the prefix/glob distinction is made by the *mode*, never by mangling here.
        let c = p(r":complete keys --prefix 'player:['");
        assert_eq!(
            c.options.get("prefix").map(Bytes::as_ref),
            Some(&b"player:["[..])
        );
    }

    #[test]
    fn no_shell_expansion_happens() {
        // §3.5: not a shell. These must arrive at the planner exactly as typed.
        for raw in ["$HOME", "~/x", "*", "`id`", "$(id)", "a;b", "a|b", "a&&b"] {
            let line = format!(":complete keys --prefix '{raw}'");
            let c = parse_local(line.as_bytes()).unwrap();
            assert_eq!(
                c.options.get("prefix").map(Bytes::as_ref),
                Some(raw.as_bytes()),
                "expanded {raw}"
            );
        }
    }

    #[test]
    fn free_text_commands_keep_the_rest_as_one_string() {
        // :find takes a task description, which is not argv and must not be re-tokenised.
        let c = p(":find 查看 hash 全部字段");
        assert_eq!(c.name, "find");
        assert_eq!(c.free_text.as_deref(), Some("查看 hash 全部字段"));
        assert!(c.positionals.is_empty());

        let c2 = p(":find read hash fields");
        assert_eq!(c2.free_text.as_deref(), Some("read hash fields"));
    }

    #[test]
    fn non_free_text_commands_keep_positionals_separate() {
        let c = p(":inspect --field config");
        assert_eq!(c.name, "inspect");
        assert_eq!(
            c.options.get("field").map(Bytes::as_ref),
            Some(&b"config"[..])
        );
        assert!(c.free_text.is_none());
    }

    #[test]
    fn the_documented_entry_points_all_parse() {
        // Every row of the §3.5 table.
        for line in [
            ":connections",
            ":use r3",
            ":browse",
            ":inspect",
            ":inspect --field config",
            ":inspect --key player:10001",
            ":view table",
            ":refresh",
            ":copy --field config",
            ":tasks",
            ":help",
            ":find",
            ":guide",
            ":assist",
            ":complete",
            ":history",
            ":complete keys --prefix player:",
            ":complete fields --key player:10001",
            ":complete clear",
            ":send -- GET k",
            ":diff --left @r2 --right @r3 --key promotion:70",
            ":bulk delete --match cache:old:*",
            ":recipe run check-platform --platform 70",
        ] {
            assert!(
                parse_local(line.as_bytes()).is_ok(),
                "failed to parse {line}"
            );
        }
    }

    #[test]
    fn rejects_malformed_input() {
        assert_eq!(parse_local(b"GET k"), Err(LocalError::NotLocal));
        assert_eq!(parse_local(b":"), Err(LocalError::MissingName));
        assert_eq!(
            parse_local(br#":find "unclosed"#),
            Err(LocalError::UnbalancedQuotes)
        );
    }

    #[test]
    fn a_redis_command_is_never_mistaken_for_a_local_one() {
        // ADR-002: only a leading colon selects the local grammar.
        assert!(parse_local(b"HGETALL player:10001").is_err());
        // ...and a key containing a colon is not a local command.
        assert!(parse_local(b"GET a:b:c").is_err());
    }
}
