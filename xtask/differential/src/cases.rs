//! The CMD-01…CMD-05 cases, with their seeds, their state probes, and the layer-4 differences
//! that are expected (v2.1 §33).
//!
//! A case names its expected output difference **in advance**. That is what separates
//! "recorded divergence" from "we looked at the diff and decided it was fine": a difference
//! not listed here fails the run.

/// One command read back from both servers after the case ran, to compare final state.
///
/// Both sides are read by the harness's own client, so this layer measures the servers and not
/// the clients — otherwise a rendering bug would look like a state divergence.
#[derive(Clone, Debug)]
pub struct StateProbe {
    /// The read command, argv by argv.
    pub argv: &'static [&'static str],
}

/// How layer 4 (output and exit code) is allowed to differ.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutputExpectation {
    /// Byte-identical stdout, and the same exit code.
    Identical,
    /// stdout is identical; the exit code differs for the stated reason.
    SameBytesDifferentExit {
        /// Why, in one line, quoting the section that decides it.
        why: &'static str,
    },
    /// Both differ, for the stated reason.
    Divergent {
        /// Why, in one line.
        why: &'static str,
    },
}

/// One differential case.
#[derive(Clone, Debug)]
pub struct Case {
    /// `CMD-01` … `CMD-05`.
    pub id: &'static str,
    /// What §33 says this case is for.
    pub scenario: &'static str,
    /// §33's pass criterion, verbatim.
    pub criterion: &'static str,
    /// Named fixture applied to **both** servers before the case runs.
    pub initial_state_fixture: &'static str,
    /// The seed commands that fixture expands to.
    pub seed: &'static [&'static [&'static str]],
    /// The command under test.
    pub request: &'static [&'static str],
    /// Read back from both servers afterwards.
    pub state_probes: &'static [StateProbe],
    /// What layer 4 is permitted to do.
    pub output: OutputExpectation,
    /// What the reply should be, in the words of appendix B.2's template.
    pub expected_reply: &'static str,
}

/// Why `redis-cli` and `prc` report errors differently.
///
/// Measured, not assumed: `redis-cli` 7.4.11 writes the error text to **stdout** and exits
/// **0**. §18.6 gives a command error its own exit code so a script can branch on it, and an
/// error on stdout is an error that lands in the middle of a `jq` pipeline.
pub const ERROR_REPORTING: &str = "redis-cli writes the error to stdout and exits 0; §18.6 gives a command error exit code 4 \
     and stderr, so a pipeline can tell a result from a failure";

/// The five cases V-A02 requires.
#[must_use]
pub fn cases() -> Vec<Case> {
    vec![
        Case {
            id: "CMD-01",
            scenario: "SET k '--raw'",
            criterion: "`--raw` 原样作为 value",
            initial_state_fixture: "empty",
            seed: &[&["DEL", "diff:cmd01"]],
            request: &["SET", "diff:cmd01", "--raw"],
            state_probes: &[StateProbe {
                argv: &["GET", "diff:cmd01"],
            }],
            output: OutputExpectation::Identical,
            expected_reply: "simple string OK",
        },
        Case {
            id: "CMD-02",
            scenario: "大小写命令与混合大小写 key",
            criterion: "只识别命令，不修改 key/value",
            initial_state_fixture: "empty",
            seed: &[
                &["DEL", "diff:CMD02:MixedKey"],
                &["DEL", "diff:cmd02:mixedkey"],
            ],
            // Lower-case command, mixed-case key, mixed-case value: the command is the only
            // token whose case may be ignored.
            request: &["set", "diff:CMD02:MixedKey", "VaLuE"],
            state_probes: &[
                StateProbe {
                    argv: &["GET", "diff:CMD02:MixedKey"],
                },
                // If anything downcased the key, this one stops being a miss.
                StateProbe {
                    argv: &["EXISTS", "diff:cmd02:mixedkey"],
                },
            ],
            output: OutputExpectation::Identical,
            expected_reply: "simple string OK",
        },
        Case {
            id: "CMD-03",
            scenario: "MGET k k missing",
            criterion: "保留重复、顺序、nil",
            initial_state_fixture: "one-string",
            seed: &[
                &["SET", "diff:cmd03", "present"],
                &["DEL", "diff:cmd03:absent"],
            ],
            request: &["MGET", "diff:cmd03", "diff:cmd03", "diff:cmd03:absent"],
            state_probes: &[StateProbe {
                argv: &["GET", "diff:cmd03"],
            }],
            output: OutputExpectation::Identical,
            expected_reply: "array of three: bulk, the same bulk again, null",
        },
        Case {
            id: "CMD-04",
            scenario: "HMGET 缺字段和空字段值",
            criterion: "nil 与 empty string 明确区分",
            initial_state_fixture: "hash-with-empty-field",
            seed: &[
                &["DEL", "diff:cmd04"],
                &["HSET", "diff:cmd04", "present", "P", "empty", ""],
            ],
            request: &["HMGET", "diff:cmd04", "empty", "absent"],
            state_probes: &[StateProbe {
                argv: &["HGETALL", "diff:cmd04"],
            }],
            // Both render an empty string and a missing field to nothing, because raw output
            // has nowhere to put the difference. That is a property of raw mode, not a defect
            // in either client, and §18.1's table mode is where the distinction lives. Layer 2
            // still proves the wire carried `$0` and `$-1` — the distinction survives where it
            // matters, and it is only the *rendering* that flattens it.
            output: OutputExpectation::Identical,
            expected_reply: "array of two: empty bulk, null — distinguishable on the wire only",
        },
        Case {
            id: "CMD-05",
            scenario: "HSET 覆盖返回 0",
            criterion: "不显示失败，不捏造修改数",
            initial_state_fixture: "hash-existing-field",
            seed: &[
                &["DEL", "diff:cmd05"],
                &["HSET", "diff:cmd05", "status", "ACTIVE"],
            ],
            // The field exists with a different value: HSET updates it and returns 0, because
            // the number it returns is *new* fields, not changed ones (R04).
            request: &["HSET", "diff:cmd05", "status", "SUSPENDED"],
            state_probes: &[StateProbe {
                argv: &["HGET", "diff:cmd05", "status"],
            }],
            output: OutputExpectation::Identical,
            expected_reply: "integer 0",
        },
    ]
}
