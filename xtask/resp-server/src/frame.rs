//! RESP2/RESP3 frame model for the synthetic server (v2.1 §19.3).
//!
//! This is the *server-side* model used to **produce** bytes on the wire. It deliberately
//! keeps lexemes (doubles, big numbers) as raw bytes so that fixtures can exercise
//! lexical preservation in the client decoder (`pr-protocol`) and JSON layer (`pr-json`).

use bytes::Bytes;

/// One RESP frame. Variants map 1:1 to RESP3 type bytes; RESP2-only null forms are explicit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Frame {
    /// `+text\r\n`
    Simple(Bytes),
    /// `-ERR text\r\n`
    Error(Bytes),
    /// `:123\r\n`
    Integer(i64),
    /// `$len\r\n<bytes>\r\n` — binary-safe, may contain NUL / invalid UTF-8.
    Bulk(Bytes),
    /// Literal `$-1\r\n`, emitted verbatim in **both** protocols. This is deliberate: a
    /// fixture needs to be able to put a RESP2-style null on a RESP3 connection to check the
    /// client's reaction. Use [`Frame::Null`] for the protocol-adaptive form.
    NullBulk,
    /// Literal `*-1\r\n`, emitted verbatim in both protocols (see [`Frame::NullBulk`]).
    NullArray,
    /// Protocol-adaptive null: `_\r\n` in RESP3, `$-1\r\n` in RESP2.
    Null,
    /// `*n\r\n...`
    Array(Vec<Frame>),
    /// `#t\r\n` / `#f\r\n`
    Boolean(bool),
    /// `,<lexeme>\r\n` — lexeme kept verbatim (`1.2300`, `-0`, `inf`, `1e3`).
    Double(Bytes),
    /// `(<digits>\r\n`
    BigNumber(Bytes),
    /// `!len\r\n<bytes>\r\n`
    BulkError(Bytes),
    /// `=len\r\nxxx:<data>\r\n` — 3-byte format prefix (e.g. `txt`, `mkd`).
    Verbatim {
        /// 3-byte format prefix such as `txt` or `mkd`.
        format: [u8; 3],
        /// Payload after the `:` separator.
        data: Bytes,
    },
    /// `%n\r\n k v k v ...` — ordered entries, duplicates allowed on purpose.
    Map(Vec<(Frame, Frame)>),
    /// `~n\r\n...`
    Set(Vec<Frame>),
    /// `|n\r\n k v ... <value>` — attribute preceding a value.
    Attribute {
        /// Attribute key/value pairs, order preserved, duplicates allowed.
        attrs: Vec<(Frame, Frame)>,
        /// The value the attribute decorates.
        value: Box<Frame>,
    },
    /// `>n\r\n...` — out-of-band push.
    Push(Vec<Frame>),
    /// `$?\r\n;len\r\n<chunk>\r\n ... ;0\r\n` — RESP3 streamed string.
    StreamedBulk(Vec<Bytes>),
    /// `*?\r\n ... .\r\n`
    StreamedArray(Vec<Frame>),
    /// `%?\r\n k v ... .\r\n`
    StreamedMap(Vec<(Frame, Frame)>),
    /// `~?\r\n ... .\r\n`
    StreamedSet(Vec<Frame>),
}

impl Frame {
    /// Convenience: simple string from `&str`.
    #[must_use]
    pub fn simple(s: &str) -> Self {
        Self::Simple(Bytes::copy_from_slice(s.as_bytes()))
    }
    /// Convenience: error from `&str`.
    #[must_use]
    pub fn error(s: &str) -> Self {
        Self::Error(Bytes::copy_from_slice(s.as_bytes()))
    }
    /// Convenience: bulk string from `&[u8]`.
    #[must_use]
    pub fn bulk(b: &[u8]) -> Self {
        Self::Bulk(Bytes::copy_from_slice(b))
    }
    /// Convenience: array of bulk strings (what clients send as commands).
    #[must_use]
    pub fn command<I, S>(args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<[u8]>,
    {
        Self::Array(args.into_iter().map(|a| Self::bulk(a.as_ref())).collect())
    }
    /// Convenience: double from its lexeme.
    #[must_use]
    pub fn double(lexeme: &str) -> Self {
        Self::Double(Bytes::copy_from_slice(lexeme.as_bytes()))
    }
    /// Convenience: map from pairs of (`&str`, Frame).
    #[must_use]
    pub fn map<I>(pairs: I) -> Self
    where
        I: IntoIterator<Item = (Frame, Frame)>,
    {
        Self::Map(pairs.into_iter().collect())
    }

    /// True for types that exist only in RESP3.
    #[must_use]
    pub fn is_resp3_only(&self) -> bool {
        matches!(
            self,
            Self::Null
                | Self::Boolean(_)
                | Self::Double(_)
                | Self::BigNumber(_)
                | Self::BulkError(_)
                | Self::Verbatim { .. }
                | Self::Map(_)
                | Self::Set(_)
                | Self::Attribute { .. }
                | Self::Push(_)
                | Self::StreamedBulk(_)
                | Self::StreamedArray(_)
                | Self::StreamedMap(_)
                | Self::StreamedSet(_)
        )
    }

    /// Nesting depth (scalars = 1). Used by the hostile generator and by budget tests.
    #[must_use]
    pub fn depth(&self) -> usize {
        match self {
            Self::Array(v) | Self::Set(v) | Self::Push(v) | Self::StreamedArray(v) | Self::StreamedSet(v) => {
                1 + v.iter().map(Self::depth).max().unwrap_or(0)
            }
            Self::Map(kv) | Self::StreamedMap(kv) => {
                1 + kv.iter().map(|(k, v)| k.depth().max(v.depth())).max().unwrap_or(0)
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
}
