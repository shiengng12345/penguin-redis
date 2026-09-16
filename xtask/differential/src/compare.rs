//! The four-layer comparison (v2.1 §32.2).
//!
//! Kept free of I/O so it can be tested without a server: the layers take bytes and return a
//! verdict, and the live runner is only responsible for producing the bytes.

use crate::cases::OutputExpectation;
use pr_protocol::decoder::{Decoder, Step};
use pr_protocol::value::Value;

/// One layer's result.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "verdict", rename_all = "kebab-case")]
pub enum LayerVerdict {
    /// The two sides are identical.
    Same,
    /// They differ, and the case said they would.
    ExpectedDifference {
        /// The reason the case gave.
        why: String,
        /// What `redis-cli` did.
        baseline: String,
        /// What `prc` did.
        penguin: String,
    },
    /// They differ and nothing said they would. This fails the run.
    UnexpectedDifference {
        /// What `redis-cli` did.
        baseline: String,
        /// What `prc` did.
        penguin: String,
    },
}

impl LayerVerdict {
    /// Whether this layer passed.
    #[must_use]
    pub fn ok(&self) -> bool {
        !matches!(self, Self::UnexpectedDifference { .. })
    }
}

/// What one side of one case did.
#[derive(Clone, Debug, Default)]
pub struct Side {
    /// Bytes the client sent, verbatim, as recorded on the wire.
    pub sent: Vec<u8>,
    /// Bytes the server sent back, verbatim.
    pub received: Vec<u8>,
    /// The client's stdout.
    pub stdout: Vec<u8>,
    /// The client's stderr.
    pub stderr: Vec<u8>,
    /// The client's exit code.
    pub exit: i32,
    /// The state read back from that client's server afterwards.
    pub final_state: Vec<Vec<u8>>,
}

/// All four layers for one case.
#[derive(Clone, Debug, serde::Serialize)]
pub struct Outcome {
    /// Layer 1 — argv bytes on the wire.
    pub argv_bytes: LayerVerdict,
    /// Layer 2 — reply bytes on the wire.
    pub reply_bytes: LayerVerdict,
    /// Layer 3 — final server state.
    pub final_state: LayerVerdict,
    /// Layer 4 — output and exit code.
    pub output_and_exit: LayerVerdict,
}

impl Outcome {
    /// Whether every layer passed.
    #[must_use]
    pub fn ok(&self) -> bool {
        self.argv_bytes.ok()
            && self.reply_bytes.ok()
            && self.final_state.ok()
            && self.output_and_exit.ok()
    }
}

/// Compare the two sides of one case.
#[must_use]
pub fn compare(baseline: &Side, penguin: &Side, output: OutputExpectation) -> Outcome {
    Outcome {
        // Layer 1 compares the *commands* rather than the raw byte stream, because a client is
        // entitled to send a handshake we do not: `redis-cli` may open with `COMMAND DOCS`.
        // Dropping to a byte comparison would report that as a difference in the argv of the
        // command under test, which it is not. The command list still holds every byte of
        // every argument, so nothing is softened — only correctly attributed.
        argv_bytes: layer(
            &render_commands(&baseline.sent),
            &render_commands(&penguin.sent),
            None,
        ),
        // Layer 2 is the raw stream. Framing is the thing being compared here.
        reply_bytes: layer(
            &escape(&baseline.received),
            &escape(&penguin.received),
            None,
        ),
        final_state: layer(
            &render_state(&baseline.final_state),
            &render_state(&penguin.final_state),
            None,
        ),
        output_and_exit: output_layer(baseline, penguin, output),
    }
}

fn layer(baseline: &str, penguin: &str, allowed: Option<&str>) -> LayerVerdict {
    if baseline == penguin {
        return LayerVerdict::Same;
    }
    allowed.map_or_else(
        || LayerVerdict::UnexpectedDifference {
            baseline: baseline.to_owned(),
            penguin: penguin.to_owned(),
        },
        |why| LayerVerdict::ExpectedDifference {
            why: why.to_owned(),
            baseline: baseline.to_owned(),
            penguin: penguin.to_owned(),
        },
    )
}

fn output_layer(baseline: &Side, penguin: &Side, expect: OutputExpectation) -> LayerVerdict {
    let b = format!(
        "exit={} stdout={} stderr={}",
        baseline.exit,
        escape(&baseline.stdout),
        escape(&baseline.stderr)
    );
    let p = format!(
        "exit={} stdout={} stderr={}",
        penguin.exit,
        escape(&penguin.stdout),
        escape(&penguin.stderr)
    );
    match expect {
        OutputExpectation::Identical => layer(&b, &p, None),
        OutputExpectation::SameBytesDifferentExit { why } => {
            if baseline.stdout != penguin.stdout {
                // The exit code was allowed to differ; the bytes were not.
                return LayerVerdict::UnexpectedDifference {
                    baseline: b,
                    penguin: p,
                };
            }
            layer(&b, &p, Some(why))
        }
        OutputExpectation::Divergent { why } => layer(&b, &p, Some(why)),
    }
}

/// Every RESP command in a recorded client→server stream, as readable text.
///
/// Anything that is not a well-formed command array is reported as such rather than skipped:
/// a client that sent something unparseable is a fact the comparison must not lose.
#[must_use]
pub fn render_commands(stream: &[u8]) -> String {
    let mut d = Decoder::with_defaults();
    d.feed(stream);
    let mut out = Vec::new();
    loop {
        match d.decode() {
            Ok(Step::Value(Value::Array(items))) => {
                let argv: Vec<String> = items
                    .iter()
                    .map(|i| match i {
                        Value::Bulk(b) | Value::Simple(b) => escape(b),
                        other => format!("<non-string: {other:?}>"),
                    })
                    .collect();
                out.push(format!("[{}]", argv.join(" ")));
            }
            Ok(Step::Value(other)) => out.push(format!("<not a command: {other:?}>")),
            Ok(Step::Incomplete) => break,
            Err(e) => {
                out.push(format!("<undecodable: {e}>"));
                break;
            }
        }
    }
    out.join("\n")
}

/// Bytes as a readable, lossless, single-line string.
///
/// `from_utf8_lossy` was deliberately not used: this text goes into a report a human reads to
/// decide whether two things are the same, and a replacement character in that report would be
/// a difference that cannot be seen.
#[must_use]
pub fn escape(b: &[u8]) -> String {
    let mut s = String::with_capacity(b.len() + 2);
    for &c in b {
        match c {
            b'\r' => s.push_str("\\r"),
            b'\n' => s.push_str("\\n"),
            b'\\' => s.push_str("\\\\"),
            0x20..=0x7e => s.push(c as char),
            other => {
                use std::fmt::Write as _;
                let _ = write!(s, "\\x{other:02x}");
            }
        }
    }
    s
}

fn render_state(replies: &[Vec<u8>]) -> String {
    replies
        .iter()
        .map(|r| escape(r))
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn side(sent: &[u8], received: &[u8], stdout: &[u8], exit: i32) -> Side {
        Side {
            sent: sent.to_vec(),
            received: received.to_vec(),
            stdout: stdout.to_vec(),
            stderr: Vec::new(),
            exit,
            final_state: vec![b"ok".to_vec()],
        }
    }

    #[test]
    fn identical_runs_pass_every_layer() {
        let s = side(b"*1\r\n$4\r\nPING\r\n", b"+PONG\r\n", b"PONG\n", 0);
        let o = compare(&s, &s, OutputExpectation::Identical);
        assert!(o.ok());
        assert_eq!(o.argv_bytes, LayerVerdict::Same);
    }

    #[test]
    fn a_handshake_the_baseline_sends_is_not_mistaken_for_an_argv_difference() {
        // redis-cli is entitled to open with its own commands. Layer 1 compares command
        // sequences, so this shows up as an extra command rather than as corrupted argv —
        // and it still shows up, because an extra round trip is worth knowing about.
        let b = side(
            b"*2\r\n$7\r\nCOMMAND\r\n$4\r\nDOCS\r\n*1\r\n$4\r\nPING\r\n",
            b"+PONG\r\n",
            b"PONG\n",
            0,
        );
        let p = side(b"*1\r\n$4\r\nPING\r\n", b"+PONG\r\n", b"PONG\n", 0);
        let o = compare(&b, &p, OutputExpectation::Identical);
        let LayerVerdict::UnexpectedDifference { baseline, penguin } = &o.argv_bytes else {
            panic!("expected a difference, got {:?}", o.argv_bytes);
        };
        assert!(baseline.contains("COMMAND DOCS"));
        assert_eq!(penguin, "[PING]");
    }

    #[test]
    fn a_byte_that_differs_inside_an_argument_is_caught() {
        // The layer exists for this: a value that looks like a flag must survive as a value.
        let b = side(
            b"*3\r\n$3\r\nSET\r\n$1\r\nk\r\n$5\r\n--raw\r\n",
            b"+OK\r\n",
            b"OK\n",
            0,
        );
        let p = side(b"*2\r\n$3\r\nSET\r\n$1\r\nk\r\n", b"+OK\r\n", b"OK\n", 0);
        assert!(
            !compare(&b, &p, OutputExpectation::Identical)
                .argv_bytes
                .ok()
        );
    }

    #[test]
    fn the_wire_distinguishes_an_empty_field_from_a_missing_one_even_when_output_does_not() {
        // CMD-04's whole point. Both render to nothing, and layer 2 still separates them.
        let empty = side(b"*1\r\n$4\r\nPING\r\n", b"*1\r\n$0\r\n\r\n", b"\n", 0);
        let missing = side(b"*1\r\n$4\r\nPING\r\n", b"*1\r\n$-1\r\n", b"\n", 0);
        let o = compare(&empty, &missing, OutputExpectation::Identical);
        assert!(!o.reply_bytes.ok(), "layer 2 must see the difference");
        assert!(
            o.output_and_exit.ok(),
            "layer 4 cannot, and is not asked to"
        );
    }

    #[test]
    fn an_exit_code_difference_is_only_allowed_when_the_case_named_it() {
        let b = side(b"*1\r\n$4\r\nPING\r\n", b"-ERR x\r\n", b"ERR x\n", 0);
        let p = side(b"*1\r\n$4\r\nPING\r\n", b"-ERR x\r\n", b"ERR x\n", 4);
        assert!(
            !compare(&b, &p, OutputExpectation::Identical)
                .output_and_exit
                .ok()
        );
        let allowed = compare(
            &b,
            &p,
            OutputExpectation::SameBytesDifferentExit { why: "§18.6" },
        );
        assert!(allowed.output_and_exit.ok());
        assert!(matches!(
            allowed.output_and_exit,
            LayerVerdict::ExpectedDifference { .. }
        ));
    }

    #[test]
    fn naming_an_exit_difference_does_not_also_excuse_the_bytes() {
        // The looser expectation must stay narrow, or it becomes a way to pass anything.
        let b = side(b"*1\r\n$4\r\nPING\r\n", b"+A\r\n", b"A\n", 0);
        let p = side(b"*1\r\n$4\r\nPING\r\n", b"+A\r\n", b"DIFFERENT\n", 4);
        let o = compare(
            &b,
            &p,
            OutputExpectation::SameBytesDifferentExit { why: "§18.6" },
        );
        assert!(!o.output_and_exit.ok());
    }

    #[test]
    fn a_final_state_difference_fails_even_when_everything_else_matched() {
        // The layer §32.2 insists on: two clients can print the same thing and leave two
        // different servers behind.
        let mut b = side(b"*1\r\n$4\r\nPING\r\n", b"+OK\r\n", b"OK\n", 0);
        let mut p = b.clone();
        b.final_state = vec![b"ACTIVE".to_vec()];
        p.final_state = vec![b"SUSPENDED".to_vec()];
        assert!(
            !compare(&b, &p, OutputExpectation::Identical)
                .final_state
                .ok()
        );
    }

    #[test]
    fn escaping_is_lossless_for_bytes_that_are_not_text() {
        assert_eq!(escape(b"a\x00\xffb\r\n"), "a\\x00\\xffb\\r\\n");
    }
}
