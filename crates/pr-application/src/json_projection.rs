//! `--json` projection v1 (v2.1 §18.3, R17, PIPE-02, V-E04).
//!
//! This is **not** `typed-json`. §18.1 gives them different jobs and §18.3 gives this one a
//! table, row by row, because the two answer different questions:
//!
//! - `typed-json` is versioned and lossless — a tool reads it and can reconstruct what arrived
//! - `--json` is the official CLI's shape — `jq` reads it and a human wrote the filter
//!
//! Losslessness and familiarity pull in opposite directions, and §18.3 chooses familiarity
//! *with the losses named*. Every place this projection throws information away, it says so on
//! stderr. That is the part worth being careful about: a projection that silently drops an
//! attribute, or silently turns `nan` into `null`, is one where a script quietly computes the
//! wrong answer.
//!
//! Where this differs from the pinned `redis-cli --json` baseline, the difference is recorded
//! in `compatibility/manifest.toml` rather than argued about.

use pr_protocol::{NullForm, Value};

/// Something the user must be told on stderr because the projection lost information.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Notice {
    /// A bulk string was not valid UTF-8 and became `{"$bytes": …}`.
    NonUtf8 {
        /// How many bytes it was.
        len: usize,
    },
    /// A double was `inf`, `-inf` or `nan` and became a string.
    NonFiniteDouble {
        /// The lexeme as it arrived.
        lexeme: String,
    },
    /// A RESP3 map had duplicate keys and became an array of pairs.
    DuplicateMapKeys,
    /// A RESP3 map had a key that was not a UTF-8 string and became an array of pairs.
    NonStringMapKey,
    /// An attribute was discarded.
    AttributeDiscarded,
    /// A verbatim string's format tag was discarded.
    VerbatimFormatDiscarded {
        /// The tag that was dropped, e.g. `txt`.
        format: String,
    },
}

impl Notice {
    /// The line written to stderr.
    #[must_use]
    pub fn message(&self) -> String {
        match self {
            Self::NonUtf8 { len } => format!(
                "a {len}-byte value is not valid UTF-8 and was encoded as {{\"$bytes\": base64}}"
            ),
            Self::NonFiniteDouble { lexeme } => {
                format!("the double {lexeme} has no JSON number form and was encoded as a string")
            }
            Self::DuplicateMapKeys => {
                "the reply has duplicate keys, so it was encoded as an array of pairs rather \
                 than an object"
                    .to_owned()
            }
            Self::NonStringMapKey => {
                "the reply has a key that is not a string, so it was encoded as an array of \
                 pairs rather than an object"
                    .to_owned()
            }
            Self::AttributeDiscarded => {
                "an attribute was discarded; use --output typed-json to see it".to_owned()
            }
            Self::VerbatimFormatDiscarded { format } => format!(
                "the verbatim format tag {format:?} was discarded; use --output typed-json to \
                 see it"
            ),
        }
    }
}

/// How to treat a value that has no faithful JSON form.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Strictness {
    /// §18.3's default: project it, and say what was lost.
    #[default]
    Lenient,
    /// `--json-strict`: refuse instead, with a non-zero exit.
    ///
    /// For a pipeline that would rather stop than receive a value it cannot round-trip.
    Strict,
}

/// The RESP version in play. `-2` and `-3` change the projection, which is PIPE-02's point.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Protocol {
    /// RESP2.
    Two,
    /// RESP3.
    Three,
}

/// The result of projecting one reply.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Projection {
    /// The JSON document.
    pub json: String,
    /// What to tell the user on stderr.
    pub notices: Vec<Notice>,
    /// Set when `--json-strict` refused the value.
    pub refused: Option<Notice>,
}

impl Projection {
    /// Whether the projection succeeded.
    #[must_use]
    pub fn ok(&self) -> bool {
        self.refused.is_none()
    }
}

/// Project one reply to §18.3's `--json` shape.
#[must_use]
pub fn project(v: &Value, protocol: Protocol, strictness: Strictness) -> Projection {
    let mut notices = Vec::new();
    let mut refused = None;
    let json = render(v, protocol, strictness, &mut notices, &mut refused);
    Projection {
        json,
        notices,
        refused,
    }
}

/// Project several replies — a `--pipe` run or a transaction (§18.3's last row).
///
/// One JSON document per line. A single reply is a single document with no trailing newline,
/// because `jq` reading one document and `jq` reading a stream are different invocations and
/// the shape has to match what the user asked for.
#[must_use]
pub fn project_many(vs: &[Value], protocol: Protocol, strictness: Strictness) -> Projection {
    if let [one] = vs {
        return project(one, protocol, strictness);
    }
    let mut notices = Vec::new();
    let mut refused = None;
    let mut lines = Vec::new();
    for v in vs {
        lines.push(render(v, protocol, strictness, &mut notices, &mut refused));
    }
    Projection {
        json: lines.join("\n"),
        notices,
        refused,
    }
}

/// Whether a reply is an error, which §18.3 maps to exit code 4.
#[must_use]
pub fn is_error(v: &Value) -> bool {
    match v {
        Value::Error(_) | Value::BulkError(_) => true,
        Value::Attribute { value, .. } => is_error(value),
        _ => false,
    }
}

fn render(
    v: &Value,
    protocol: Protocol,
    strictness: Strictness,
    notices: &mut Vec<Notice>,
    refused: &mut Option<Notice>,
) -> String {
    match v {
        // A string that is valid UTF-8 is a JSON string.
        Value::Simple(b) | Value::Bulk(b) => string_or_bytes(b, strictness, notices, refused),
        Value::Integer(n) => n.to_string(),
        // §18.3: a finite double keeps its original lexeme as a JSON number, so `1.2300` does
        // not silently become `1.23` — the trailing zeros were the server's choice.
        Value::Double(lex) => double(lex, notices),
        // A big number stays a string. Making it a JSON number would lose precision in every
        // parser that uses a float, which is most of them.
        Value::BigNumber(d) => json_string(&String::from_utf8_lossy(d)),
        Value::Boolean(t) => t.to_string(),
        Value::Null(NullForm::Bulk | NullForm::Array | NullForm::Resp3) => "null".to_owned(),
        Value::Array(items) | Value::Set(items) => {
            // A set keeps arrival order: the server sent it in some order and inventing a
            // different one would make two runs of the same command differ.
            let parts: Vec<String> = items
                .iter()
                .map(|i| render(i, protocol, strictness, notices, refused))
                .collect();
            format!("[{}]", parts.join(","))
        }
        Value::Map(entries) => map(entries, protocol, strictness, notices, refused),
        // §18.3: the content, with the format prefix dropped — and the drop is announced.
        Value::Verbatim { format, data } => {
            notices.push(Notice::VerbatimFormatDiscarded {
                format: String::from_utf8_lossy(format).into_owned(),
            });
            string_or_bytes(data, strictness, notices, refused)
        }
        Value::Error(b) | Value::BulkError(b) => {
            format!(
                r#"{{"error":{}}}"#,
                json_string(&String::from_utf8_lossy(b))
            )
        }
        // §18.3: discarded, and counted. The value it decorated is projected on its own.
        Value::Attribute { value, .. } => {
            notices.push(Notice::AttributeDiscarded);
            render(value, protocol, strictness, notices, refused)
        }
        // A push has no place in a `--json` document at all (§18.1); the emitter routes it
        // away before this is reached. Projecting it as an array is the least surprising thing
        // to do if one ever arrives here.
        Value::Push(items) => {
            let parts: Vec<String> = items
                .iter()
                .map(|i| render(i, protocol, strictness, notices, refused))
                .collect();
            format!("[{}]", parts.join(","))
        }
    }
}

fn string_or_bytes(
    b: &[u8],
    strictness: Strictness,
    notices: &mut Vec<Notice>,
    refused: &mut Option<Notice>,
) -> String {
    if let Ok(s) = std::str::from_utf8(b) {
        return json_string(s);
    }
    let n = Notice::NonUtf8 { len: b.len() };
    if strictness == Strictness::Strict {
        if refused.is_none() {
            *refused = Some(n);
        }
        // The document still has to be well-formed JSON: a caller that ignores `refused`
        // should get something parseable rather than a hole, and `null` is the honest stand-in
        // for "there is no JSON form of this".
        return "null".to_owned();
    }
    notices.push(n);
    format!(r#"{{"$bytes":"{}"}}"#, crate::output::base64(b))
}

fn double(lex: &[u8], notices: &mut Vec<Notice>) -> String {
    let text = String::from_utf8_lossy(lex).into_owned();
    let finite = text.parse::<f64>().is_ok_and(f64::is_finite);
    if finite {
        // The lexeme verbatim: `1.2300` is a valid JSON number and reparsing it would throw
        // away what the server chose to send.
        text
    } else {
        notices.push(Notice::NonFiniteDouble {
            lexeme: text.clone(),
        });
        json_string(&text)
    }
}

fn map(
    entries: &[(Value, Value)],
    protocol: Protocol,
    strictness: Strictness,
    notices: &mut Vec<Notice>,
    refused: &mut Option<Notice>,
) -> String {
    let keys: Option<Vec<String>> = entries
        .iter()
        .map(|(k, _)| match k {
            Value::Simple(b) | Value::Bulk(b) => std::str::from_utf8(b).ok().map(str::to_owned),
            _ => None,
        })
        .collect();

    let as_pairs = |notices: &mut Vec<Notice>, refused: &mut Option<Notice>| {
        let parts: Vec<String> = entries
            .iter()
            .map(|(k, val)| {
                format!(
                    "[{},{}]",
                    render(k, protocol, strictness, notices, refused),
                    render(val, protocol, strictness, notices, refused)
                )
            })
            .collect();
        format!("[{}]", parts.join(","))
    };

    let Some(keys) = keys else {
        notices.push(Notice::NonStringMapKey);
        return as_pairs(notices, refused);
    };

    let mut seen: Vec<&String> = keys.iter().collect();
    let before = seen.len();
    seen.sort();
    seen.dedup();
    if seen.len() != before {
        notices.push(Notice::DuplicateMapKeys);
        return as_pairs(notices, refused);
    }

    let parts: Vec<String> = entries
        .iter()
        .zip(keys.iter())
        .map(|((_, val), k)| {
            format!(
                "{}:{}",
                json_string(k),
                render(val, protocol, strictness, notices, refused)
            )
        })
        .collect();
    format!("{{{}}}", parts.join(","))
}

fn json_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                use std::fmt::Write as _;
                let _ = write!(out, "\\u{:04x}", c as u32);
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}
