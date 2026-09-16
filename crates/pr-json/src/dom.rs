//! Token-preserving JSON DOM (v2.1 §8.3/§8.5, ADR-028, R06).
//!
//! `serde_json::Value` cannot satisfy §8.3: its `Map` drops duplicate members and its
//! `Number` normalises the lexeme, so `{"a":1,"a":2}` loses a member and `1.2300`,
//! `9007199254740993` and `-0` all change. `RawValue` keeps the source text but gives no
//! addressable tree to edit. So this crate keeps both: a structural tree whose scalars carry
//! their original byte span, and the original buffer they index into.

use bytes::Bytes;
use std::ops::Range;

/// A node in the document.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JsonNode {
    /// Byte range in the source buffer covering exactly this node.
    pub span: Range<usize>,
    /// Structure and preserved lexeme.
    pub kind: NodeKind,
}

/// Node structure.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NodeKind {
    /// `null`.
    Null,
    /// `true` / `false`.
    Bool(bool),
    /// A number, kept as its original lexeme. Never parsed to f64 for storage.
    Number,
    /// A string. `span` covers the quotes; decoding is on demand.
    String,
    /// An array.
    Array(Vec<JsonNode>),
    /// An object. Members keep source order, duplicates are retained, and each member
    /// records which occurrence of its name it is (1-based).
    Object(Vec<Member>),
}

/// One object member.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Member {
    /// Span of the key token, including quotes.
    pub key_span: Range<usize>,
    /// Decoded key.
    pub key: String,
    /// 1-based index among members sharing this key.
    pub occurrence: u32,
    /// Total members sharing this key (so a renderer can show `a (1/2)`).
    pub occurrence_total: u32,
    /// The value.
    pub value: JsonNode,
}

/// A parsed document: the tree plus the bytes it indexes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Document {
    source: Bytes,
    root: JsonNode,
}

/// Parse / addressing errors.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum JsonError {
    /// Syntax error at a byte offset.
    #[error("invalid JSON at byte {0}: {1}")]
    Syntax(usize, &'static str),
    /// Input ended early.
    #[error("unexpected end of input")]
    Eof,
    /// Nesting beyond the configured budget (v2.1 §24.3).
    #[error("nesting deeper than {0}")]
    DepthExceeded(usize),
    /// Trailing bytes after a complete value.
    #[error("trailing bytes at {0}")]
    Trailing(usize),
    /// A path names a member that appears more than once without an occurrence selector.
    #[error("ambiguous member {0:?}: appears {1} times, use {0}#n")]
    AmbiguousMember(String, u32),
    /// Path did not resolve.
    #[error("no such path")]
    NoSuchPath,
    /// Path syntax is invalid.
    #[error("bad path: {0}")]
    BadPath(&'static str),
}

/// Parse limits.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    /// Maximum nesting depth.
    pub max_depth: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self { max_depth: 128 }
    }
}

struct Parser<'a> {
    b: &'a [u8],
    i: usize,
    depth: usize,
    limits: Limits,
}

impl Parser<'_> {
    fn ws(&mut self) {
        while self.i < self.b.len() && matches!(self.b[self.i], b' ' | b'\t' | b'\n' | b'\r') {
            self.i += 1;
        }
    }
    fn peek(&self) -> Result<u8, JsonError> {
        self.b.get(self.i).copied().ok_or(JsonError::Eof)
    }
    fn eat(&mut self, c: u8, msg: &'static str) -> Result<(), JsonError> {
        if self.peek()? == c {
            self.i += 1;
            Ok(())
        } else {
            Err(JsonError::Syntax(self.i, msg))
        }
    }
    fn lit(&mut self, s: &[u8], msg: &'static str) -> Result<(), JsonError> {
        if self.b.len() >= self.i + s.len() && &self.b[self.i..self.i + s.len()] == s {
            self.i += s.len();
            Ok(())
        } else {
            Err(JsonError::Syntax(self.i, msg))
        }
    }

    fn string_span(&mut self) -> Result<Range<usize>, JsonError> {
        let start = self.i;
        self.eat(b'"', "expected string")?;
        loop {
            let c = self.peek()?;
            self.i += 1;
            match c {
                b'"' => return Ok(start..self.i),
                b'\\' => {
                    let e = self.peek()?;
                    self.i += 1;
                    if e == b'u' {
                        for _ in 0..4 {
                            let h = self.peek()?;
                            if !h.is_ascii_hexdigit() {
                                return Err(JsonError::Syntax(self.i, "bad \\u escape"));
                            }
                            self.i += 1;
                        }
                    } else if !matches!(e, b'"' | b'\\' | b'/' | b'b' | b'f' | b'n' | b'r' | b't') {
                        return Err(JsonError::Syntax(self.i - 1, "bad escape"));
                    }
                }
                // Unescaped control characters are invalid JSON.
                0x00..=0x1f => return Err(JsonError::Syntax(self.i - 1, "control char in string")),
                _ => {}
            }
        }
    }

    fn number_span(&mut self) -> Result<Range<usize>, JsonError> {
        let start = self.i;
        if self.peek()? == b'-' {
            self.i += 1;
        }
        // int
        match self.peek()? {
            b'0' => self.i += 1,
            b'1'..=b'9' => {
                while self.i < self.b.len() && self.b[self.i].is_ascii_digit() {
                    self.i += 1;
                }
            }
            _ => return Err(JsonError::Syntax(self.i, "expected digit")),
        }
        // frac
        if self.i < self.b.len() && self.b[self.i] == b'.' {
            self.i += 1;
            let d0 = self.i;
            while self.i < self.b.len() && self.b[self.i].is_ascii_digit() {
                self.i += 1;
            }
            if self.i == d0 {
                return Err(JsonError::Syntax(self.i, "expected digit after '.'"));
            }
        }
        // exp
        if self.i < self.b.len() && matches!(self.b[self.i], b'e' | b'E') {
            self.i += 1;
            if self.i < self.b.len() && matches!(self.b[self.i], b'+' | b'-') {
                self.i += 1;
            }
            let d0 = self.i;
            while self.i < self.b.len() && self.b[self.i].is_ascii_digit() {
                self.i += 1;
            }
            if self.i == d0 {
                return Err(JsonError::Syntax(self.i, "expected digit in exponent"));
            }
        }
        Ok(start..self.i)
    }

    // A single-pass value parser is naturally long; splitting it per-token would scatter the
    // span bookkeeping that makes this DOM lossless.
    #[allow(clippy::too_many_lines)]
    fn value(&mut self) -> Result<JsonNode, JsonError> {
        self.depth += 1;
        if self.depth > self.limits.max_depth {
            return Err(JsonError::DepthExceeded(self.limits.max_depth));
        }
        self.ws();
        let start = self.i;
        let node = match self.peek()? {
            b'n' => {
                self.lit(b"null", "expected null")?;
                JsonNode {
                    span: start..self.i,
                    kind: NodeKind::Null,
                }
            }
            b't' => {
                self.lit(b"true", "expected true")?;
                JsonNode {
                    span: start..self.i,
                    kind: NodeKind::Bool(true),
                }
            }
            b'f' => {
                self.lit(b"false", "expected false")?;
                JsonNode {
                    span: start..self.i,
                    kind: NodeKind::Bool(false),
                }
            }
            b'"' => {
                let s = self.string_span()?;
                JsonNode {
                    span: s,
                    kind: NodeKind::String,
                }
            }
            b'-' | b'0'..=b'9' => {
                let s = self.number_span()?;
                JsonNode {
                    span: s,
                    kind: NodeKind::Number,
                }
            }
            b'[' => {
                self.i += 1;
                let mut items = Vec::new();
                self.ws();
                if self.peek()? == b']' {
                    self.i += 1;
                } else {
                    loop {
                        items.push(self.value()?);
                        self.ws();
                        match self.peek()? {
                            b',' => self.i += 1,
                            b']' => {
                                self.i += 1;
                                break;
                            }
                            _ => return Err(JsonError::Syntax(self.i, "expected ',' or ']'")),
                        }
                    }
                }
                JsonNode {
                    span: start..self.i,
                    kind: NodeKind::Array(items),
                }
            }
            b'{' => {
                self.i += 1;
                let mut raw: Vec<(Range<usize>, String, JsonNode)> = Vec::new();
                self.ws();
                if self.peek()? == b'}' {
                    self.i += 1;
                } else {
                    loop {
                        self.ws();
                        let ks = self.string_span()?;
                        let key = decode_string(&self.b[ks.clone()])
                            .ok_or(JsonError::Syntax(ks.start, "bad key escape"))?;
                        self.ws();
                        self.eat(b':', "expected ':'")?;
                        let v = self.value()?;
                        raw.push((ks, key, v));
                        self.ws();
                        match self.peek()? {
                            b',' => self.i += 1,
                            b'}' => {
                                self.i += 1;
                                break;
                            }
                            _ => return Err(JsonError::Syntax(self.i, "expected ',' or '}'")),
                        }
                    }
                }
                // Assign occurrence indices. Duplicates are kept, in source order (§8.3).
                let mut totals: std::collections::HashMap<String, u32> =
                    std::collections::HashMap::new();
                for (_, k, _) in &raw {
                    *totals.entry(k.clone()).or_insert(0) += 1;
                }
                let mut seen: std::collections::HashMap<String, u32> =
                    std::collections::HashMap::new();
                let members = raw
                    .into_iter()
                    .map(|(ks, key, value)| {
                        let n = seen.entry(key.clone()).or_insert(0);
                        *n += 1;
                        let total = totals.get(&key).copied().unwrap_or(1);
                        Member {
                            key_span: ks,
                            occurrence: *n,
                            occurrence_total: total,
                            key,
                            value,
                        }
                    })
                    .collect();
                JsonNode {
                    span: start..self.i,
                    kind: NodeKind::Object(members),
                }
            }
            _ => return Err(JsonError::Syntax(self.i, "unexpected token")),
        };
        self.depth -= 1;
        Ok(node)
    }
}

/// Decode a JSON string token (including surrounding quotes) into a `String`.
///
/// Returns `None` on a malformed escape or an unpaired surrogate. A lone surrogate is
/// well-formed JSON that denotes no Unicode scalar, so the document is still kept and only
/// this one string declines to decode (ADR-005).
#[must_use]
pub fn decode_string(tok: &[u8]) -> Option<String> {
    let inner = tok.get(1..tok.len().saturating_sub(1))?;
    let mut out = String::with_capacity(inner.len());
    let mut i = 0;
    while i < inner.len() {
        let c = inner[i];
        if c != b'\\' {
            let s = std::str::from_utf8(&inner[i..]).ok()?;
            let ch = s.chars().next()?;
            out.push(ch);
            i += ch.len_utf8();
            continue;
        }
        i += 1;
        let e = *inner.get(i)?;
        i += 1;
        match e {
            b'"' => out.push('"'),
            b'\\' => out.push('\\'),
            b'/' => out.push('/'),
            b'b' => out.push('\u{8}'),
            b'f' => out.push('\u{c}'),
            b'n' => out.push('\n'),
            b'r' => out.push('\r'),
            b't' => out.push('\t'),
            b'u' => {
                let hex = inner.get(i..i + 4)?;
                i += 4;
                let hi = u16::from_str_radix(std::str::from_utf8(hex).ok()?, 16).ok()?;
                if (0xD800..0xDC00).contains(&hi) {
                    // high surrogate: require the low half
                    if inner.get(i) != Some(&b'\\') || inner.get(i + 1) != Some(&b'u') {
                        return None;
                    }
                    let hex2 = inner.get(i + 2..i + 6)?;
                    i += 6;
                    let lo = u16::from_str_radix(std::str::from_utf8(hex2).ok()?, 16).ok()?;
                    if !(0xDC00..0xE000).contains(&lo) {
                        return None;
                    }
                    let c =
                        0x1_0000u32 + ((u32::from(hi) - 0xD800) << 10) + (u32::from(lo) - 0xDC00);
                    out.push(char::from_u32(c)?);
                } else if (0xDC00..0xE000).contains(&hi) {
                    return None; // lone low surrogate
                } else {
                    out.push(char::from_u32(u32::from(hi))?);
                }
            }
            _ => return None,
        }
    }
    Some(out)
}

impl Document {
    /// Parse a complete JSON document.
    ///
    /// # Errors
    /// [`JsonError`] describing the first syntax problem, depth overrun, or trailing bytes.
    pub fn parse(source: impl Into<Bytes>, limits: Limits) -> Result<Self, JsonError> {
        let source: Bytes = source.into();
        let mut p = Parser {
            b: &source,
            i: 0,
            depth: 0,
            limits,
        };
        let root = p.value()?;
        p.ws();
        if p.i != source.len() {
            return Err(JsonError::Trailing(p.i));
        }
        Ok(Self { source, root })
    }

    /// Only a complete object or array counts as auto-detected JSON (v2.1 §8.1).
    #[must_use]
    pub fn looks_like_json(b: &[u8]) -> bool {
        let t = b.trim_ascii();
        matches!(t.first(), Some(b'{' | b'['))
            && Self::parse(Bytes::copy_from_slice(t), Limits::default()).is_ok()
    }

    /// The root node.
    #[must_use]
    pub fn root(&self) -> &JsonNode {
        &self.root
    }

    /// The original bytes.
    #[must_use]
    pub fn source(&self) -> &Bytes {
        &self.source
    }

    /// Raw bytes of a node, exactly as they appeared.
    #[must_use]
    pub fn raw(&self, n: &JsonNode) -> &[u8] {
        &self.source[n.span.clone()]
    }

    /// The preserved lexeme of a number node (`1.2300`, `-0`, `9007199254740993`).
    #[must_use]
    pub fn number_lexeme(&self, n: &JsonNode) -> Option<&str> {
        matches!(n.kind, NodeKind::Number)
            .then(|| std::str::from_utf8(self.raw(n)).ok())
            .flatten()
    }

    /// Decoded value of a string node.
    #[must_use]
    pub fn string_value(&self, n: &JsonNode) -> Option<String> {
        matches!(n.kind, NodeKind::String)
            .then(|| decode_string(self.raw(n)))
            .flatten()
    }

    /// Resolve a path (see [`crate::path`]) to a node.
    ///
    /// # Errors
    /// [`JsonError::AmbiguousMember`] when a duplicated name is used without `#n`,
    /// [`JsonError::NoSuchPath`] when it does not resolve.
    pub fn get(&self, path: &str) -> Result<&JsonNode, JsonError> {
        let steps = crate::path::parse(path)?;
        let mut cur = &self.root;
        for st in &steps {
            cur = crate::path::step(cur, st)?;
        }
        Ok(cur)
    }

    /// Replace the bytes of the node at `path`, leaving every other byte untouched.
    ///
    /// # Errors
    /// As [`Document::get`].
    pub fn replace(&self, path: &str, new_raw: &[u8]) -> Result<Bytes, JsonError> {
        let node = self.get(path)?;
        let mut out = Vec::with_capacity(self.source.len() + new_raw.len());
        out.extend_from_slice(&self.source[..node.span.start]);
        out.extend_from_slice(new_raw);
        out.extend_from_slice(&self.source[node.span.end..]);
        Ok(Bytes::from(out))
    }

    /// Duplicate member names present anywhere in the document, for the `(1/2)` annotation
    /// and for the ambiguity warning in §8.3.
    #[must_use]
    pub fn duplicate_members(&self) -> Vec<(String, u32)> {
        let mut out = Vec::new();
        collect_dups(&self.root, &mut out);
        out
    }
}

fn collect_dups(n: &JsonNode, out: &mut Vec<(String, u32)>) {
    match &n.kind {
        NodeKind::Object(ms) => {
            for m in ms {
                if m.occurrence == 1 && m.occurrence_total > 1 {
                    out.push((m.key.clone(), m.occurrence_total));
                }
                collect_dups(&m.value, out);
            }
        }
        NodeKind::Array(items) => {
            for i in items {
                collect_dups(i, out);
            }
        }
        _ => {}
    }
}
