//! Decoded RESP values (v2.1 §19.3, ADR-005, ADR-009).
//!
//! Scalars keep their **bytes**, never a converted Rust type: a double keeps its lexeme so
//! `1.2300` survives, a bulk string keeps arbitrary bytes so NUL and invalid UTF-8 survive.
//! Maps are ordered entry lists, not a `HashMap`, so duplicate keys and wire order survive.

use bytes::Bytes;

/// A fully decoded RESP value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Value {
    /// `+OK`
    Simple(Bytes),
    /// `-ERR ...`
    Error(Bytes),
    /// `:123`
    Integer(i64),
    /// `$n` — binary safe.
    Bulk(Bytes),
    /// `$-1`, `*-1` or `_` — which form arrived is recorded for faithful re-encoding.
    Null(NullForm),
    /// `*n`
    Array(Vec<Value>),
    /// `#t` / `#f`
    Boolean(bool),
    /// `,` — lexeme preserved verbatim (`1.2300`, `-0`, `inf`, `nan`).
    Double(Bytes),
    /// `(` — arbitrary precision, kept as digits.
    BigNumber(Bytes),
    /// `!n`
    BulkError(Bytes),
    /// `=n` — 3-byte format tag plus payload.
    Verbatim {
        /// Format tag such as `txt` or `mkd`.
        format: [u8; 3],
        /// Payload after the `:`.
        data: Bytes,
    },
    /// `%n` — ordered entries; duplicates preserved (ADR-005).
    Map(Vec<(Value, Value)>),
    /// `~n`
    Set(Vec<Value>),
    /// `|n` attached to the value that follows it.
    Attribute {
        /// Attribute entries.
        attrs: Vec<(Value, Value)>,
        /// The decorated value.
        value: Box<Value>,
    },
    /// `>n` — out-of-band; never occupies a command's reply slot (v2.1 §19.3).
    Push(Vec<Value>),
}

/// Which null spelling arrived.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NullForm {
    /// `$-1\r\n`
    Bulk,
    /// `*-1\r\n`
    Array,
    /// `_\r\n`
    Resp3,
}

impl Value {
    /// Nesting depth (scalars are 1).
    #[must_use]
    pub fn depth(&self) -> usize {
        match self {
            Self::Array(v) | Self::Set(v) | Self::Push(v) => {
                1 + v.iter().map(Self::depth).max().unwrap_or(0)
            }
            Self::Map(kv) => {
                1 + kv
                    .iter()
                    .map(|(k, v)| k.depth().max(v.depth()))
                    .max()
                    .unwrap_or(0)
            }
            Self::Attribute { attrs, value } => {
                1 + attrs
                    .iter()
                    .map(|(k, v)| k.depth().max(v.depth()))
                    .max()
                    .unwrap_or(0)
                    .max(value.depth())
            }
            _ => 1,
        }
    }

    /// Total number of scalar leaves, for budget accounting.
    #[must_use]
    pub fn leaf_count(&self) -> usize {
        match self {
            Self::Array(v) | Self::Set(v) | Self::Push(v) => v.iter().map(Self::leaf_count).sum(),
            Self::Map(kv) => kv
                .iter()
                .map(|(k, v)| k.leaf_count() + v.leaf_count())
                .sum(),
            Self::Attribute { attrs, value } => {
                attrs
                    .iter()
                    .map(|(k, v)| k.leaf_count() + v.leaf_count())
                    .sum::<usize>()
                    + value.leaf_count()
            }
            _ => 1,
        }
    }

    /// True for a push frame, which must be routed out-of-band rather than answering a command.
    #[must_use]
    pub fn is_push(&self) -> bool {
        matches!(self, Self::Push(_))
    }
}
