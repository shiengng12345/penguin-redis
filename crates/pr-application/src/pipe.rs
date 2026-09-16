//! `--pipe` frame-by-frame admission (v2.1 §18.5, ADR-025, R01, V-B03).
//!
//! This is the fix for the first of the two review BLOCKERs. v2.0 promised `--pipe`
//! compatibility without ever saying how each frame would be classified, which left exactly
//! two readings, both wrong: treat the stream as opaque and every production guard is
//! bypassable by piping, or refuse to parse it and the compatibility claim is empty.
//!
//! So every frame is decoded, turned into a `CommandRequest`, and put through the **same**
//! catalog classification and policy as an interactive command (ADR-008). Malformed input is
//! fail-closed: reading stops, nothing further is sent, and the offending frame index and
//! byte offset are reported — a batch that silently skips what it could not parse is worse
//! than one that stops.

use pr_catalog::{LocalSpec, resolve};
use pr_core::{CommandRequest, Effects, ExitCode, RequestOrigin};
use pr_protocol::{DecodeError, Decoder, Step, Value};
use thiserror::Error;

/// Why the pipe stopped.
#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum PipeError {
    /// A frame was not a RESP array of bulk strings.
    #[error("frame {index} at byte {offset}: not an array of bulk strings")]
    NotACommand {
        /// Zero-based frame index.
        index: usize,
        /// Byte offset where the frame began.
        offset: usize,
    },
    /// The stream is malformed.
    #[error("frame {index} at byte {offset}: {reason}")]
    Malformed {
        /// Zero-based frame index.
        index: usize,
        /// Byte offset where decoding failed.
        offset: usize,
        /// Decoder message.
        reason: String,
    },
    /// The stream ended mid-frame.
    #[error("stream ended inside frame {index} at byte {offset}")]
    Truncated {
        /// Zero-based frame index.
        index: usize,
        /// Byte offset where the partial frame began.
        offset: usize,
    },
    /// Policy refused a frame.
    #[error("frame {index} ({command}) refused: {reason}")]
    Refused {
        /// Zero-based frame index.
        index: usize,
        /// Command name.
        command: String,
        /// Why.
        reason: &'static str,
    },
}

impl PipeError {
    /// The process exit code this maps to (§18.6).
    #[must_use]
    pub fn exit_code(&self) -> ExitCode {
        match self {
            // Malformed input is a usage error: the caller sent something invalid.
            Self::NotACommand { .. } | Self::Malformed { .. } | Self::Truncated { .. } => {
                ExitCode::Usage
            }
            // A policy refusal is its own code, so a script can tell them apart.
            Self::Refused { .. } => ExitCode::PolicyDenied,
        }
    }
}

/// The policy a piped stream is admitted under.
// Four independent permissions, not a state machine: a profile can allow writes but deny
// destructive commands, or allow unclassified but deny writes.
#[allow(clippy::struct_excessive_bools)]
#[derive(Clone, Copy, Debug)]
pub struct PipePolicy {
    /// Whether writes are permitted at all on this profile.
    pub allow_writes: bool,
    /// Whether destructive commands are permitted.
    pub allow_destructive: bool,
    /// Whether commands the local catalog does not classify may run.
    pub allow_unclassified: bool,
    /// Whether this origin may use `--pipe` at all. Agents may not (ADR-025).
    pub allow_pipe: bool,
}

impl PipePolicy {
    /// A development profile: permissive, but still classified.
    #[must_use]
    pub fn development() -> Self {
        Self {
            allow_writes: true,
            allow_destructive: false,
            allow_unclassified: true,
            allow_pipe: true,
        }
    }
    /// A production profile: read-only by default (§23.1).
    #[must_use]
    pub fn production() -> Self {
        Self {
            allow_writes: false,
            allow_destructive: false,
            allow_unclassified: false,
            allow_pipe: true,
        }
    }
    /// An agent: `--pipe` is not available at all.
    #[must_use]
    pub fn agent() -> Self {
        Self {
            allow_writes: false,
            allow_destructive: false,
            allow_unclassified: false,
            allow_pipe: false,
        }
    }
}

/// One admitted frame.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Admitted {
    /// Frame index.
    pub index: usize,
    /// The request, ready for the kernel.
    pub request: CommandRequest,
}

/// Outcome of admitting a whole stream.
#[derive(Debug)]
pub struct Admission {
    /// Frames accepted, in order. Empty when the first frame was refused.
    pub accepted: Vec<Admitted>,
    /// Why admission stopped, if it did.
    pub stopped: Option<PipeError>,
}

impl Admission {
    /// Exit code for the run.
    #[must_use]
    pub fn exit_code(&self) -> ExitCode {
        self.stopped
            .as_ref()
            .map_or(ExitCode::Success, PipeError::exit_code)
    }
}

/// Look a command up in the catalog. In the real product this consults the compiled catalog;
/// the shape is what matters here — classification happens **before** policy, and an unknown
/// command stays unknown.
fn classify(name: &str, catalog: &dyn Fn(&str) -> Option<Effects>) -> LocalSpec {
    catalog(name).map_or_else(|| LocalSpec::unknown(name), |e| LocalSpec::known(name, e))
}

/// Decode and admit a piped stream.
///
/// Stops at the first frame that cannot be parsed or is refused. Everything accepted before
/// that point is returned, because those frames may already have been sent.
///
/// # Errors
/// Never returns `Err`; the failure is carried in [`Admission::stopped`] alongside whatever
/// was accepted first, which is what a caller needs to report per-frame outcomes.
#[must_use]
pub fn admit(
    input: &[u8],
    policy: PipePolicy,
    catalog: &dyn Fn(&str) -> Option<Effects>,
) -> Admission {
    let mut accepted = Vec::new();

    if !policy.allow_pipe {
        return Admission {
            accepted,
            stopped: Some(PipeError::Refused {
                index: 0,
                command: String::new(),
                reason: "this origin may not use --pipe",
            }),
        };
    }

    let mut dec = Decoder::with_defaults();
    dec.feed(input);
    let mut index = 0usize;
    let mut offset = 0usize;

    loop {
        let before = dec.buffered();
        match dec.decode() {
            Ok(Step::Value(v)) => {
                let consumed = before - dec.buffered();
                let frame_offset = offset;
                offset += consumed;

                let Some(args) = as_command(&v) else {
                    return Admission {
                        accepted,
                        stopped: Some(PipeError::NotACommand {
                            index,
                            offset: frame_offset,
                        }),
                    };
                };
                let name = String::from_utf8_lossy(&args[0]).to_uppercase();
                let spec = classify(&name, catalog);
                let resolved = resolve(&spec, None);

                if let Some(reason) = refuse_reason(resolved.effects, policy) {
                    return Admission {
                        accepted,
                        stopped: Some(PipeError::Refused {
                            index,
                            command: name,
                            reason,
                        }),
                    };
                }

                let request = CommandRequest::new(args, RequestOrigin::Pipe, resolved.effects);
                accepted.push(Admitted { index, request });
                index += 1;
            }
            Ok(Step::Incomplete) => {
                if dec.buffered() == 0 {
                    return Admission {
                        accepted,
                        stopped: None,
                    }; // clean end of stream
                }
                return Admission {
                    accepted,
                    stopped: Some(PipeError::Truncated { index, offset }),
                };
            }
            Err(e) => {
                let reason = match &e {
                    DecodeError::Protocol(m) | DecodeError::Budget(m) => (*m).to_owned(),
                };
                return Admission {
                    accepted,
                    stopped: Some(PipeError::Malformed {
                        index,
                        offset,
                        reason,
                    }),
                };
            }
        }
    }
}

/// A command frame is an array of bulk strings with at least one element.
fn as_command(v: &Value) -> Option<Vec<bytes::Bytes>> {
    let Value::Array(items) = v else { return None };
    if items.is_empty() {
        return None;
    }
    let mut out = Vec::with_capacity(items.len());
    for i in items {
        match i {
            Value::Bulk(b) => out.push(b.clone()),
            _ => return None,
        }
    }
    Some(out)
}

/// Whether policy refuses these effects.
fn refuse_reason(e: Effects, p: PipePolicy) -> Option<&'static str> {
    if e.unknown && !p.allow_unclassified {
        return Some("command is not classified by the local catalog");
    }
    if e.destructive && !p.allow_destructive {
        return Some("destructive commands are denied on this profile");
    }
    if (e.writes_data || e.consumes || e.admin) && !p.allow_writes {
        return Some("writes are denied on this profile");
    }
    None
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    /// A small stand-in catalog with the classifications that matter here.
    fn catalog(name: &str) -> Option<Effects> {
        match name {
            "GET" | "HGETALL" | "PING" => Some(Effects::read()),
            "SET" | "DEL" | "HSET" => Some(Effects::write()),
            "FLUSHDB" | "FLUSHALL" => Some(Effects {
                writes_data: true,
                destructive: true,
                admin: true,
                ..Effects::default()
            }),
            _ => None,
        }
    }

    fn wire(cmds: &[&[&str]]) -> Vec<u8> {
        let mut out = Vec::new();
        for c in cmds {
            out.extend_from_slice(format!("*{}\r\n", c.len()).as_bytes());
            for a in *c {
                out.extend_from_slice(format!("${}\r\n{a}\r\n", a.len()).as_bytes());
            }
        }
        out
    }

    fn names(a: &Admission) -> Vec<String> {
        a.accepted
            .iter()
            .map(|x| x.request.command_name().unwrap_or_default())
            .collect()
    }

    #[test]
    fn a_clean_read_only_stream_is_fully_admitted() {
        let input = wire(&[&["GET", "a"], &["GET", "b"], &["PING"]]);
        let a = admit(&input, PipePolicy::production(), &catalog);
        assert!(a.stopped.is_none(), "{:?}", a.stopped);
        assert_eq!(names(&a), vec!["GET", "GET", "PING"]);
        assert_eq!(a.exit_code(), ExitCode::Success);
    }

    #[test]
    fn every_frame_goes_through_the_same_classification() {
        // ADR-008: piping must not be a second, weaker path.
        let input = wire(&[&["SET", "k", "v"]]);
        let a = admit(&input, PipePolicy::development(), &catalog);
        assert!(a.stopped.is_none());
        assert!(
            a.accepted[0].request.effects.writes_data,
            "effects came from the catalog"
        );
        assert_eq!(a.accepted[0].request.origin, RequestOrigin::Pipe);
    }

    #[test]
    fn production_stops_at_the_first_write_and_does_not_skip_it() {
        // SEC-09: the BLOCKER. A silent skip would be worse than stopping.
        let input = wire(&[&["GET", "a"], &["SET", "k", "v"], &["GET", "b"]]);
        let a = admit(&input, PipePolicy::production(), &catalog);
        assert_eq!(names(&a), vec!["GET"], "only the frames before the refusal");
        match a.stopped {
            Some(PipeError::Refused {
                index, ref command, ..
            }) => {
                assert_eq!(index, 1);
                assert_eq!(command, "SET");
            }
            other => panic!("expected a refusal, got {other:?}"),
        }
        assert_eq!(a.exit_code(), ExitCode::PolicyDenied);
    }

    #[test]
    fn a_destructive_command_is_refused_even_where_writes_are_allowed() {
        let input = wire(&[&["SET", "k", "v"], &["FLUSHDB"]]);
        let a = admit(&input, PipePolicy::development(), &catalog);
        assert_eq!(names(&a), vec!["SET"]);
        assert!(matches!(
            a.stopped,
            Some(PipeError::Refused { index: 1, .. })
        ));
    }

    #[test]
    fn an_unclassified_command_is_refused_in_production() {
        // §23.1: unknown is maximum risk, not "probably fine".
        let input = wire(&[&["NEWMODULE.DOTHING", "x"]]);
        let a = admit(&input, PipePolicy::production(), &catalog);
        assert!(a.accepted.is_empty());
        match a.stopped {
            Some(PipeError::Refused { reason, .. }) => {
                assert!(reason.contains("not classified"), "{reason}");
            }
            other => panic!("expected a refusal, got {other:?}"),
        }
        // ...but permitted on a dev profile that opted in.
        let dev = admit(&input, PipePolicy::development(), &catalog);
        assert!(dev.stopped.is_none());
        assert!(dev.accepted[0].request.effects.unknown);
    }

    #[test]
    fn an_agent_may_not_use_pipe_at_all() {
        // ADR-025.
        let input = wire(&[&["GET", "a"]]);
        let a = admit(&input, PipePolicy::agent(), &catalog);
        assert!(a.accepted.is_empty());
        assert_eq!(a.exit_code(), ExitCode::PolicyDenied);
    }

    // ---------------------------------------------------------------- fail-closed (PIPE-08)
    #[test]
    fn a_truncated_frame_stops_the_stream_and_reports_where() {
        let mut input = wire(&[&["GET", "a"]]);
        input.extend_from_slice(b"*2\r\n$3\r\nGET\r\n$5\r\nabc"); // cut short
        let a = admit(&input, PipePolicy::development(), &catalog);
        assert_eq!(
            names(&a),
            vec!["GET"],
            "the complete frame was still admitted"
        );
        match a.stopped {
            Some(PipeError::Truncated { index, offset }) => {
                assert_eq!(index, 1);
                assert!(offset > 0, "the offset must locate the partial frame");
            }
            other => panic!("expected Truncated, got {other:?}"),
        }
        assert_eq!(a.exit_code(), ExitCode::Usage);
    }

    #[test]
    fn a_malformed_frame_stops_the_stream_and_reports_where() {
        let first = wire(&[&["GET", "a"]]);
        let first_len = first.len();
        let mut input = first;
        input.extend_from_slice(b"@nonsense\r\n");
        let a = admit(&input, PipePolicy::development(), &catalog);
        assert_eq!(names(&a), vec!["GET"]);
        match a.stopped {
            Some(PipeError::Malformed {
                index,
                offset,
                ref reason,
            }) => {
                assert_eq!(index, 1);
                assert_eq!(
                    offset, first_len,
                    "the offset must point at the start of the bad frame"
                );
                assert!(!reason.is_empty());
            }
            other => panic!("expected Malformed, got {other:?}"),
        }
        assert_eq!(a.exit_code(), ExitCode::Usage);
    }

    #[test]
    fn a_non_command_frame_is_refused() {
        // A bare integer or a nested array is not a command, and must not be forwarded.
        for bad in [
            &b":42\r\n"[..],
            &b"+OK\r\n"[..],
            &b"*1\r\n*1\r\n$1\r\na\r\n"[..],
            &b"*0\r\n"[..],
        ] {
            let mut input = wire(&[&["GET", "a"]]);
            input.extend_from_slice(bad);
            let a = admit(&input, PipePolicy::development(), &catalog);
            assert_eq!(names(&a), vec!["GET"], "for {bad:?}");
            assert!(
                matches!(a.stopped, Some(PipeError::NotACommand { index: 1, .. })),
                "for {bad:?} got {:?}",
                a.stopped
            );
        }
    }

    #[test]
    fn nothing_after_the_stopping_frame_is_admitted() {
        // The core fail-closed property: no frame past the failure is forwarded.
        let mut input = wire(&[&["GET", "a"]]);
        input.extend_from_slice(b"@bad\r\n");
        input.extend_from_slice(&wire(&[&["SET", "k", "v"], &["FLUSHALL"]]));
        let a = admit(&input, PipePolicy::development(), &catalog);
        assert_eq!(a.accepted.len(), 1);
        assert!(!names(&a).contains(&"FLUSHALL".to_string()));
    }

    #[test]
    fn binary_arguments_survive_the_pipe_intact() {
        // §18.5: --pipe is how a script sends bytes argv cannot carry.
        let mut input = Vec::new();
        input.extend_from_slice(b"*3\r\n$3\r\nSET\r\n$1\r\nk\r\n$3\r\n");
        input.extend_from_slice(&[0x00, 0xff, 0x1b]);
        input.extend_from_slice(b"\r\n");
        let a = admit(&input, PipePolicy::development(), &catalog);
        assert!(a.stopped.is_none(), "{:?}", a.stopped);
        assert_eq!(a.accepted[0].request.args[2].as_ref(), &[0x00, 0xff, 0x1b]);
    }

    #[test]
    fn an_empty_stream_is_a_clean_success() {
        let a = admit(b"", PipePolicy::production(), &catalog);
        assert!(a.accepted.is_empty());
        assert!(a.stopped.is_none());
        assert_eq!(a.exit_code(), ExitCode::Success);
    }

    #[test]
    fn frame_indices_are_contiguous_and_zero_based() {
        let input = wire(&[&["GET", "a"], &["GET", "b"], &["GET", "c"]]);
        let a = admit(&input, PipePolicy::production(), &catalog);
        let idx: Vec<usize> = a.accepted.iter().map(|x| x.index).collect();
        assert_eq!(idx, vec![0, 1, 2]);
    }

    #[test]
    fn usage_and_policy_failures_have_different_exit_codes() {
        // A script must be able to tell "I sent rubbish" from "I was not allowed".
        let malformed = admit(b"@bad\r\n", PipePolicy::development(), &catalog);
        let refused = admit(&wire(&[&["FLUSHALL"]]), PipePolicy::production(), &catalog);
        assert_eq!(malformed.exit_code(), ExitCode::Usage);
        assert_eq!(refused.exit_code(), ExitCode::PolicyDenied);
        assert_ne!(malformed.exit_code(), refused.exit_code());
    }
}
