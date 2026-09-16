//! Output modes and where push frames go (v2.1 §18.1, §19.3, R17, R40, V-B06).
//!
//! A RESP3 push — an invalidation, a pub/sub delivery, server-side tracking — can arrive
//! before a reply, after it, or with no reply in flight at all. §18.1 is explicit about what
//! that must not do: **stdout in a one-shot run contains the command's reply and nothing
//! else.** A script doing `prc --json GET k | jq .` has to keep working while another client
//! writes to a tracked key.
//!
//! So a push never occupies the reply slot, in any mode. What varies is where it goes
//! instead:
//!
//! | mode | push destination |
//! |---|---|
//! | `pretty`, `raw`, `json`, `csv`, `bytes`, `resp` | discarded, counted on stderr |
//! | any of the above with `--show-pushes` | NDJSON events on stderr |
//! | `ndjson`, `typed-json` | events on stdout, in arrival order |
//!
//! The last row is why the routing is a function of the mode rather than a flag: those two
//! modes are *event* formats, so a push is legitimate content. The others are reply formats,
//! where a push would be an unannounced extra record.

use pr_protocol::{NullForm, Value};

/// How the reply is written.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutputMode {
    /// Table and colour, for a person.
    Pretty,
    /// The official CLI's raw behaviour.
    Raw,
    /// The official CLI's JSON.
    Json,
    /// CSV for one command.
    Csv,
    /// Versioned, type-safe JSON.
    TypedJson,
    /// One structured event per line.
    Ndjson,
    /// Normalised RESP3, re-encoded.
    Resp,
    /// One blob, no delimiter.
    Bytes,
}

impl OutputMode {
    /// The flag that selects it, for error messages.
    #[must_use]
    pub fn flag(self) -> &'static str {
        match self {
            Self::Pretty => "--output pretty",
            Self::Raw => "--raw",
            Self::Json => "--json",
            Self::Csv => "--csv",
            Self::TypedJson => "--output typed-json",
            Self::Ndjson => "--output ndjson",
            Self::Resp => "--output resp",
            Self::Bytes => "--bytes",
        }
    }

    /// Whether this mode's stdout is a stream of events rather than one reply.
    #[must_use]
    pub fn is_event_stream(self) -> bool {
        matches!(self, Self::Ndjson | Self::TypedJson)
    }
}

/// Why the flags could not be resolved.
#[derive(Clone, Debug, thiserror::Error, PartialEq, Eq)]
pub enum FlagError {
    /// Two or more output selectors were given.
    ///
    /// Reported rather than resolved by "last one wins", because the last one winning is how
    /// a stray `--json` in a wrapper script silently changes what a pipeline receives.
    #[error("output flags conflict: {0}; choose one")]
    Conflict(String),
    /// `--output` was given a name that does not exist.
    #[error("unknown output format {0:?}")]
    UnknownFormat(String),
}

/// Resolve the output mode from the flags as given, in order.
///
/// # Errors
/// [`FlagError::Conflict`] if more than one selector appears, [`FlagError::UnknownFormat`] for
/// an unrecognised `--output` name.
pub fn select(flags: &[&str]) -> Result<OutputMode, FlagError> {
    let mut chosen: Vec<OutputMode> = Vec::new();
    let mut i = 0;
    while i < flags.len() {
        let f = flags[i];
        let mode = match f {
            "--raw" => Some(OutputMode::Raw),
            "--json" => Some(OutputMode::Json),
            "--csv" => Some(OutputMode::Csv),
            "--bytes" => Some(OutputMode::Bytes),
            "--output" => {
                i += 1;
                let name = flags.get(i).copied().unwrap_or("");
                Some(named(name)?)
            }
            _ => match f.strip_prefix("--output=") {
                Some(name) => Some(named(name)?),
                None => None,
            },
        };
        if let Some(m) = mode {
            chosen.push(m);
        }
        i += 1;
    }
    match chosen.as_slice() {
        [] => Ok(OutputMode::Pretty),
        [one] => Ok(*one),
        many => {
            let mut names: Vec<&str> = many.iter().map(|m| m.flag()).collect();
            names.dedup();
            Err(FlagError::Conflict(names.join(", ")))
        }
    }
}

fn named(name: &str) -> Result<OutputMode, FlagError> {
    match name {
        "pretty" => Ok(OutputMode::Pretty),
        "raw" => Ok(OutputMode::Raw),
        "json" => Ok(OutputMode::Json),
        "csv" => Ok(OutputMode::Csv),
        "typed-json" => Ok(OutputMode::TypedJson),
        "ndjson" => Ok(OutputMode::Ndjson),
        "resp" => Ok(OutputMode::Resp),
        "bytes" => Ok(OutputMode::Bytes),
        other => Err(FlagError::UnknownFormat(other.to_owned())),
    }
}

/// Which RESP version to negotiate.
///
/// `--json` prefers RESP3 so that a map is a map rather than a flattened array. That is a
/// *default*, not an override: a user who wrote `-2` is asking for the RESP2 shape, and
/// §18.1 forbids silently giving them something else.
#[must_use]
pub fn resolve_protocol(mode: OutputMode, explicit: Option<u8>) -> u8 {
    match explicit {
        Some(v) => v,
        None if matches!(
            mode,
            OutputMode::Json | OutputMode::TypedJson | OutputMode::Resp
        ) =>
        {
            3
        }
        None => 2,
    }
}

/// Where push frames go.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PushRoute {
    /// Dropped from stdout, counted, and the count reported on stderr at the end.
    DiscardCounted,
    /// Written to stderr as NDJSON events.
    StderrNdjson,
    /// Written to stdout as events, in arrival order.
    StdoutEvent,
}

/// Decide the route.
///
/// `--show-pushes` is deliberately a no-op for the event formats: they already carry pushes
/// on stdout, and also writing them to stderr would make a consumer that reads both see every
/// push twice.
#[must_use]
pub fn push_route(mode: OutputMode, show_pushes: bool) -> PushRoute {
    if mode.is_event_stream() {
        PushRoute::StdoutEvent
    } else if show_pushes {
        PushRoute::StderrNdjson
    } else {
        PushRoute::DiscardCounted
    }
}

/// A frame as it arrived, with the exact bytes that carried it.
#[derive(Clone, Debug)]
pub struct Arrival<'a> {
    /// The decoded value.
    pub value: &'a Value,
    /// The bytes it was decoded from, for `--trace-wire`.
    pub wire: &'a [u8],
}

/// Routes arriving frames to stdout, stderr and an optional wire trace.
#[derive(Debug, Default)]
pub struct Emitter {
    mode: Option<OutputMode>,
    route: Option<PushRoute>,
    /// Everything destined for stdout.
    pub stdout: Vec<u8>,
    /// Everything destined for stderr.
    pub stderr: Vec<u8>,
    /// Raw wire bytes, if `--trace-wire` was given.
    trace: Option<Vec<u8>>,
    pushes: usize,
    replies: usize,
    seq: usize,
}

impl Emitter {
    /// An emitter for `mode`, with the push route `show_pushes` implies.
    #[must_use]
    pub fn new(mode: OutputMode, show_pushes: bool) -> Self {
        Self {
            mode: Some(mode),
            route: Some(push_route(mode, show_pushes)),
            ..Self::default()
        }
    }

    /// Also capture the raw wire bytes.
    ///
    /// Separate from `--output resp` on purpose: `resp` is a *normalised* re-encoding, and
    /// somebody debugging a server needs what actually came down the socket (§18.1).
    #[must_use]
    pub fn with_wire_trace(mut self) -> Self {
        self.trace = Some(Vec::new());
        self
    }

    /// The wire trace, if one was requested.
    #[must_use]
    pub fn wire_trace(&self) -> Option<&[u8]> {
        self.trace.as_deref()
    }

    /// Pushes seen so far.
    #[must_use]
    pub fn pushes(&self) -> usize {
        self.pushes
    }

    /// Replies written so far.
    #[must_use]
    pub fn replies(&self) -> usize {
        self.replies
    }

    /// Route one arriving frame.
    pub fn accept(&mut self, arrival: &Arrival<'_>) {
        if let Some(t) = self.trace.as_mut() {
            t.extend_from_slice(arrival.wire);
        }
        let mode = self.mode.unwrap_or(OutputMode::Pretty);
        let route = self.route.unwrap_or(PushRoute::DiscardCounted);
        self.seq += 1;
        let seq = self.seq;

        if is_push(arrival.value) {
            self.pushes += 1;
            match route {
                PushRoute::DiscardCounted => {}
                PushRoute::StderrNdjson => {
                    let line = event_line("push", seq, arrival.value);
                    self.stderr.extend_from_slice(line.as_bytes());
                    self.stderr.push(b'\n');
                }
                PushRoute::StdoutEvent => {
                    let line = event_line("push", seq, arrival.value);
                    self.stdout.extend_from_slice(line.as_bytes());
                    self.stdout.push(b'\n');
                }
            }
            return;
        }

        self.replies += 1;
        match mode {
            OutputMode::Ndjson | OutputMode::TypedJson => {
                let line = event_line("reply", seq, arrival.value);
                self.stdout.extend_from_slice(line.as_bytes());
                self.stdout.push(b'\n');
            }
            OutputMode::Resp => encode_resp3(arrival.value, &mut self.stdout),
            OutputMode::Bytes => {
                if let Value::Bulk(b) = arrival.value {
                    self.stdout.extend_from_slice(b);
                } else {
                    self.stderr
                        .extend_from_slice(b"--bytes requires a single blob reply\n");
                }
            }
            OutputMode::Raw | OutputMode::Pretty | OutputMode::Json | OutputMode::Csv => {
                let line = render_simple(mode, arrival.value);
                self.stdout.extend_from_slice(line.as_bytes());
            }
        }
    }

    /// Write the trailing diagnostics.
    ///
    /// The suppression count is the contract's only mention of the pushes in a one-shot run,
    /// so it is not optional: silently dropping them would leave a user with no way to know
    /// their tracking invalidations are being thrown away.
    pub fn finish(&mut self) {
        if self.route == Some(PushRoute::DiscardCounted) && self.pushes > 0 {
            let plural = if self.pushes == 1 { "" } else { "s" };
            self.stderr.extend_from_slice(
                format!("{} push frame{plural} suppressed\n", self.pushes).as_bytes(),
            );
        }
    }

    /// stdout as text.
    #[must_use]
    pub fn stdout_text(&self) -> String {
        String::from_utf8_lossy(&self.stdout).into_owned()
    }

    /// stderr as text.
    #[must_use]
    pub fn stderr_text(&self) -> String {
        String::from_utf8_lossy(&self.stderr).into_owned()
    }
}

/// Whether this frame is a push, looking through an attribute wrapper.
///
/// A push can carry an attribute, and unwrapping is what stops an attributed invalidation
/// from being mistaken for a reply.
#[must_use]
pub fn is_push(v: &Value) -> bool {
    match v {
        Value::Push(_) => true,
        Value::Attribute { value, .. } => is_push(value),
        _ => false,
    }
}

// ---------------------------------------------------------------- RESP3 re-encoding

/// Re-encode as normalised RESP3 (§18.1 `--output resp`).
///
/// Normalised means:
/// - RESP3 spellings throughout, whatever arrived: a RESP2 `$-1` becomes `_`, so a consumer
///   does not have to handle three nulls
/// - an attribute is written immediately before the value it decorates, which is a defined
///   order rather than the incidental one a particular server used
/// - bytes are untouched: double lexemes, big-number digits, verbatim tags and binary bulks
///   round-trip exactly
///
/// It is explicitly **not** a wire capture. `--trace-wire` is.
pub fn encode_resp3(v: &Value, out: &mut Vec<u8>) {
    match v {
        Value::Simple(b) => {
            out.push(b'+');
            out.extend_from_slice(b);
            out.extend_from_slice(b"\r\n");
        }
        Value::Error(b) => {
            out.push(b'-');
            out.extend_from_slice(b);
            out.extend_from_slice(b"\r\n");
        }
        Value::Integer(n) => out.extend_from_slice(format!(":{n}\r\n").as_bytes()),
        Value::Bulk(b) => {
            out.extend_from_slice(format!("${}\r\n", b.len()).as_bytes());
            out.extend_from_slice(b);
            out.extend_from_slice(b"\r\n");
        }
        // Every null spelling normalises to the RESP3 one. Which arrived is still available
        // to a caller that kept the decoded value; this output exists to be uniform.
        Value::Null(_) => out.extend_from_slice(b"_\r\n"),
        Value::Boolean(t) => out.extend_from_slice(if *t { b"#t\r\n" } else { b"#f\r\n" }),
        Value::Double(lex) => {
            out.push(b',');
            out.extend_from_slice(lex);
            out.extend_from_slice(b"\r\n");
        }
        Value::BigNumber(d) => {
            out.push(b'(');
            out.extend_from_slice(d);
            out.extend_from_slice(b"\r\n");
        }
        Value::BulkError(b) => {
            out.extend_from_slice(format!("!{}\r\n", b.len()).as_bytes());
            out.extend_from_slice(b);
            out.extend_from_slice(b"\r\n");
        }
        Value::Verbatim { format, data } => {
            out.extend_from_slice(format!("={}\r\n", data.len() + 4).as_bytes());
            out.extend_from_slice(format);
            out.push(b':');
            out.extend_from_slice(data);
            out.extend_from_slice(b"\r\n");
        }
        Value::Array(items) => {
            out.extend_from_slice(format!("*{}\r\n", items.len()).as_bytes());
            for i in items {
                encode_resp3(i, out);
            }
        }
        Value::Set(items) => {
            out.extend_from_slice(format!("~{}\r\n", items.len()).as_bytes());
            for i in items {
                encode_resp3(i, out);
            }
        }
        Value::Map(entries) => {
            out.extend_from_slice(format!("%{}\r\n", entries.len()).as_bytes());
            for (k, val) in entries {
                encode_resp3(k, out);
                encode_resp3(val, out);
            }
        }
        Value::Push(items) => {
            out.extend_from_slice(format!(">{}\r\n", items.len()).as_bytes());
            for i in items {
                encode_resp3(i, out);
            }
        }
        Value::Attribute { attrs, value } => {
            out.extend_from_slice(format!("|{}\r\n", attrs.len()).as_bytes());
            for (k, val) in attrs {
                encode_resp3(k, out);
                encode_resp3(val, out);
            }
            encode_resp3(value, out);
        }
    }
}

// ---------------------------------------------------------------- event rendering

/// One NDJSON event.
///
/// Built by hand rather than through a serialiser because the field order is part of the
/// contract a consumer reads: `kind` first, so a line can be dispatched before it is fully
/// parsed.
fn event_line(kind: &str, seq: usize, v: &Value) -> String {
    format!(
        r#"{{"v":1,"kind":"{kind}","seq":{seq},"type":"{}","value":{}}}"#,
        type_name(v),
        json_value(v)
    )
}

/// The type tag carried in every event.
#[must_use]
pub fn type_name(v: &Value) -> &'static str {
    match v {
        Value::Simple(_) => "simple",
        Value::Error(_) => "error",
        Value::Integer(_) => "integer",
        Value::Bulk(_) => "bulk",
        Value::Null(_) => "null",
        Value::Array(_) => "array",
        Value::Boolean(_) => "boolean",
        Value::Double(_) => "double",
        Value::BigNumber(_) => "bignumber",
        Value::BulkError(_) => "bulkerror",
        Value::Verbatim { .. } => "verbatim",
        Value::Map(_) => "map",
        Value::Set(_) => "set",
        Value::Attribute { .. } => "attribute",
        Value::Push(_) => "push",
    }
}

/// Render a value as JSON for an event.
///
/// Binary that is not valid UTF-8 becomes `{"b64":"..."}` rather than a lossy string: a
/// consumer that needs the bytes can get them, and one that does not can tell the difference
/// (§18.1 typed-json).
fn json_value(v: &Value) -> String {
    match v {
        Value::Simple(b) | Value::Error(b) | Value::BulkError(b) | Value::Bulk(b) => bytes_json(b),
        Value::Integer(n) => n.to_string(),
        Value::Null(_) => "null".to_owned(),
        Value::Boolean(t) => t.to_string(),
        // A double keeps its lexeme: `1.2300` and `inf` are not JSON numbers, and rounding
        // them here would lose what the server actually said.
        Value::Double(lex) | Value::BigNumber(lex) => bytes_json(lex),
        Value::Verbatim { format, data } => format!(
            r#"{{"format":{},"data":{}}}"#,
            bytes_json(format),
            bytes_json(data)
        ),
        Value::Array(items) | Value::Set(items) | Value::Push(items) => {
            let parts: Vec<String> = items.iter().map(json_value).collect();
            format!("[{}]", parts.join(","))
        }
        Value::Map(entries) => {
            // A list of pairs, not an object: Redis map keys can repeat and can be binary,
            // and an object would silently drop a duplicate (ADR-005).
            let parts: Vec<String> = entries
                .iter()
                .map(|(k, val)| format!("[{},{}]", json_value(k), json_value(val)))
                .collect();
            format!("[{}]", parts.join(","))
        }
        Value::Attribute { attrs, value } => {
            let parts: Vec<String> = attrs
                .iter()
                .map(|(k, val)| format!("[{},{}]", json_value(k), json_value(val)))
                .collect();
            format!(
                r#"{{"attributes":[{}],"value":{}}}"#,
                parts.join(","),
                json_value(value)
            )
        }
    }
}

fn bytes_json(b: &[u8]) -> String {
    match std::str::from_utf8(b) {
        Ok(s) => json_string(s),
        Err(_) => format!(r#"{{"b64":"{}"}}"#, base64(b)),
    }
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

/// Standard base64 with padding.
///
/// Sixteen lines rather than a dependency: this is the only place the project needs it, and a
/// supply-chain entry costs more to review than the function costs to read (ADR-023).
#[must_use]
pub fn base64(input: &[u8]) -> String {
    const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(input.len().div_ceil(3) * 4);
    for chunk in input.chunks(3) {
        let b = [
            chunk[0],
            chunk.get(1).copied().unwrap_or(0),
            chunk.get(2).copied().unwrap_or(0),
        ];
        let n = (u32::from(b[0]) << 16) | (u32::from(b[1]) << 8) | u32::from(b[2]);
        out.push(A[(n >> 18) as usize & 63] as char);
        out.push(A[(n >> 12) as usize & 63] as char);
        out.push(if chunk.len() > 1 {
            A[(n >> 6) as usize & 63] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            A[n as usize & 63] as char
        } else {
            '='
        });
    }
    out
}

/// The line the reply-shaped modes write.
///
/// Phase 0 only needs enough of each to prove the routing; the real renderers are Phase 2's
/// (`pr-render`). What matters here is that a reply always produces exactly one record and a
/// push never produces any.
fn render_simple(mode: OutputMode, v: &Value) -> String {
    match mode {
        OutputMode::Json => format!("{}\n", json_value(v)),
        OutputMode::Csv => format!("{}\n", csv_cells(v).join(",")),
        _ => format!("{}\n", raw_text(v)),
    }
}

fn raw_text(v: &Value) -> String {
    match v {
        Value::Simple(b) | Value::Bulk(b) | Value::Error(b) | Value::BulkError(b) => {
            String::from_utf8_lossy(b).into_owned()
        }
        Value::Double(b) | Value::BigNumber(b) => String::from_utf8_lossy(b).into_owned(),
        Value::Integer(n) => n.to_string(),
        Value::Boolean(t) => if *t { "1" } else { "0" }.to_owned(),
        Value::Null(NullForm::Bulk | NullForm::Array | NullForm::Resp3) => String::new(),
        Value::Array(items) | Value::Set(items) | Value::Push(items) => {
            items.iter().map(raw_text).collect::<Vec<_>>().join("\n")
        }
        Value::Map(entries) => entries
            .iter()
            .map(|(k, val)| format!("{}\n{}", raw_text(k), raw_text(val)))
            .collect::<Vec<_>>()
            .join("\n"),
        Value::Verbatim { data, .. } => String::from_utf8_lossy(data).into_owned(),
        Value::Attribute { value, .. } => raw_text(value),
    }
}

fn csv_cells(v: &Value) -> Vec<String> {
    match v {
        Value::Array(items) | Value::Set(items) => items.iter().flat_map(csv_cells).collect(),
        Value::Map(entries) => entries
            .iter()
            .flat_map(|(k, val)| [csv_cell(k), csv_cell(val)])
            .collect(),
        other => vec![csv_cell(other)],
    }
}

fn csv_cell(v: &Value) -> String {
    let t = raw_text(v);
    if t.contains([',', '"', '\n']) {
        format!("\"{}\"", t.replace('"', "\"\""))
    } else {
        t
    }
}
