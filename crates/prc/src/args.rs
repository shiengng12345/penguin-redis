//! Top-level argument parsing (v2.1 §3.2, §3.3, ADR-001, R26).
//!
//! ```text
//! prc [client-options] [@profile] [client-options] [--] COMMAND [redis-arguments...]
//! ```
//!
//! Two rules do the heavy lifting:
//! - once `COMMAND` starts, every remaining token belongs to Redis. `SET example '--raw'`
//!   stores the string `--raw` (CMD-01).
//! - `-h` always means host. Help is `--help`, never `-h`.
//!
//! The profile×flag matrix (R26) is here because the review found §3.3 defined only `-n`,
//! leaving `-h`/`-p`/`-u`/`--user` undefined next to an `@profile` — two implementers would
//! have built different products.

use bytes::Bytes;
use thiserror::Error;

/// Why parsing failed. Each maps to exit code 2 (`ExitCode::Usage`).
#[derive(Clone, Debug, PartialEq, Eq, Error)]
pub enum ArgError {
    /// A flag that changes the target or identity was combined with `@profile`.
    #[error("--{0} changes the target and cannot be combined with @{1}; create a profile instead")]
    TargetConflict(String, String),
    /// Two profiles were named.
    #[error("more than one profile given: @{0} and @{1}")]
    MultipleProfiles(String, String),
    /// A profile and a full connection URL together.
    #[error("@{0} and an explicit URL cannot be combined")]
    ProfileAndUrl(String),
    /// A flag needing a value had none.
    #[error("--{0} needs a value")]
    MissingValue(String),
    /// A value could not be parsed.
    #[error("invalid value for --{0}: {1}")]
    BadValue(String, String),
    /// Mutually exclusive output modes.
    #[error("output modes are mutually exclusive: {0} and {1}")]
    OutputConflict(String, String),
    /// A TLS option that would loosen a profile's verification.
    #[error("--{0} would weaken @{1}'s TLS verification")]
    TlsWeakening(String, String),
    /// A TLS option that would disable verification entirely.
    ///
    /// NET-01: 「拒绝或显式配置；**无静默 insecure**」. Rejected everywhere, not only when a
    /// profile is present — a flag that works without a profile is a flag that gets used.
    /// The message names the configured alternative, because refusing without saying what to
    /// do instead is how a workaround gets invented.
    #[error(
        "{0} would disable TLS verification, and there is no such mode. \
         Use --tls-ca <file> to trust a private CA, or --tls-name <name> when the server's \
         certificate names something other than the address you are connecting to"
    )]
    InsecureTlsRefused(String),
    /// A connection URI that is not one.
    ///
    /// **The URI is not in the message**, only the shape of what was wrong. §23.4's first
    /// named secret path is a URI parse error, because the failure case is the one a person
    /// pastes into a terminal, a chat window and a bug report — with the password still in it.
    /// `pr_security::uri::UriError` is built so it has nowhere to put one.
    #[error("--{0}: {1}")]
    Uri(String, pr_security::uri::UriError),
    /// Unknown flag.
    #[error("unknown option: {0}")]
    Unknown(String),
}

/// Output mode (v2.1 §18.1).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum OutputMode {
    /// Table on a TTY, raw otherwise.
    #[default]
    Auto,
    /// Forced table.
    Pretty,
    /// redis-cli raw.
    Raw,
    /// redis-cli JSON.
    Json,
    /// CSV.
    Csv,
    /// Versioned typed JSON.
    TypedJson,
    /// One event per line.
    Ndjson,
    /// Normalised RESP re-encoding.
    Resp,
    /// A single blob, no delimiter.
    BytesOnly,
}

impl OutputMode {
    fn name(self) -> &'static str {
        match self {
            Self::Auto => "auto",
            Self::Pretty => "--output pretty",
            Self::Raw => "--raw",
            Self::Json => "--json",
            Self::Csv => "--csv",
            Self::TypedJson => "--output typed-json",
            Self::Ndjson => "--output ndjson",
            Self::Resp => "--output resp",
            Self::BytesOnly => "--bytes",
        }
    }
}

/// How the target was specified.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Target {
    /// Nothing given: open the connection selector on a TTY.
    #[default]
    Unspecified,
    /// A saved profile.
    Profile(String),
    /// A direct connection.
    Direct {
        /// Host, if given.
        host: Option<String>,
        /// Port, if given.
        port: Option<u16>,
        /// Full URL, if given, already parsed.
        ///
        /// Parsed rather than kept as a string: the string contains the password, and a
        /// `#[derive(Debug)]` anywhere above it would print it. `uri::Redis` hides the
        /// password in its own `Debug`, and parsing registers it with `pr_security::secrets`
        /// so every other sink scrubs it from then on.
        url: Option<pr_security::uri::Redis>,
    },
}

/// A fully parsed command line.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Invocation {
    /// Resolved target.
    pub target: Target,
    /// One-shot DB override (`-n`).
    pub db: Option<u32>,
    /// Output mode.
    pub output: OutputMode,
    /// Explicit RESP version (`-2` / `-3`).
    pub protocol: Option<u8>,
    /// `--no-color` / `--color`.
    pub color: Option<String>,
    /// `--plain`.
    pub plain: bool,
    /// `--tui`.
    pub tui: bool,
    /// TLS configuration (§21.1, §21.3). There is no field for disabling verification.
    pub tls: TlsArgs,
    /// Redis command and its arguments, exact bytes.
    pub command: Vec<Bytes>,
}

/// How TLS was configured on the command line.
///
/// Every field either turns verification **on** or tells it what to check. §21.1's NET-01
/// requires that no field can turn it off, so there is none — the loosening flags are refused
/// during parsing and never reach a value.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TlsArgs {
    /// `--tls`, or implied by any of the others.
    pub enabled: bool,
    /// `--tls-ca <file>`: the CA set that must have signed the server's certificate.
    pub ca: Option<String>,
    /// `--tls-name <name>`: the name to verify, separate from the address (§21.3).
    pub name: Option<String>,
    /// `--tls-cert <file>`: client certificate.
    pub cert: Option<String>,
    /// `--tls-key <file>`: client key.
    pub key: Option<String>,
}

/// Flags that change *which server or identity* we talk to. Combining any of these with an
/// `@profile` is a conflict, not an override (R26): silently re-pointing a profile is how a
/// credential ends up at the wrong service.
const TARGET_FLAGS: &[&str] = &[
    "-h",
    "-p",
    "-s",
    "-u",
    "--user",
    "--askpass",
    "--sentinel",
    "--cluster",
];

/// TLS flags that would *loosen* verification; never allowed to override a profile.
const TLS_WEAKENING: &[&str] = &["--insecure", "--no-verify", "--tls-verify=none"];

/// TLS options that *configure* verification rather than removing it.
///
/// These exist so that refusing [`TLS_WEAKENING`] is a redirection rather than the removal of
/// a capability: §21.3 separates the TCP target, the verified name and the trusted CA, and
/// each of those is something an operator can set.
pub const TLS_CONFIGURING: &[(&str, &str)] = &[
    ("--tls", "                 require TLS"),
    (
        "--tls-ca",
        " FILE         the CA set that must have signed the server certificate",
    ),
    (
        "--tls-name",
        " NAME       the name to verify, when it differs from the address",
    ),
    ("--tls-cert", " FILE       client certificate"),
    ("--tls-key", " FILE        client private key"),
];

fn is_profile(tok: &str) -> bool {
    tok.len() > 1 && tok.starts_with('@')
}

/// Parse an argv (excluding argv[0]).
///
/// # Errors
/// [`ArgError`] for any conflict or malformed value; the caller maps it to exit code 2.
#[allow(clippy::too_many_lines)]
pub fn parse(argv: &[String]) -> Result<Invocation, ArgError> {
    let mut inv = Invocation::default();
    let mut profile: Option<String> = None;
    let mut direct_host: Option<String> = None;
    let mut direct_port: Option<u16> = None;
    let mut direct_url: Option<pr_security::uri::Redis> = None;
    let mut seen_target_flags: Vec<String> = Vec::new();
    // NET-01: any flag that would disable verification, refused once the profile is known.
    let mut weakening: Option<String> = None;
    let mut i = 0usize;

    let set_output = |inv: &mut Invocation, m: OutputMode| -> Result<(), ArgError> {
        if inv.output != OutputMode::Auto && inv.output != m {
            return Err(ArgError::OutputConflict(
                inv.output.name().into(),
                m.name().into(),
            ));
        }
        inv.output = m;
        Ok(())
    };

    while i < argv.len() {
        let a = argv[i].as_str();

        // `--` ends the client-option region; everything after is the Redis command.
        if a == "--" {
            i += 1;
            inv.command = argv[i..]
                .iter()
                .map(|s| Bytes::copy_from_slice(s.as_bytes()))
                .collect();
            break;
        }

        if is_profile(a) {
            let name = a[1..].to_owned();
            if let Some(p) = &profile {
                return Err(ArgError::MultipleProfiles(p.clone(), name));
            }
            profile = Some(name);
            i += 1;
            continue;
        }

        // A non-flag token that is not a profile starts the Redis command (§3.2).
        if !a.starts_with('-') {
            inv.command = argv[i..]
                .iter()
                .map(|s| Bytes::copy_from_slice(s.as_bytes()))
                .collect();
            break;
        }

        let need = |i: &mut usize, flag: &str| -> Result<String, ArgError> {
            *i += 1;
            argv.get(*i)
                .cloned()
                .ok_or_else(|| ArgError::MissingValue(flag.trim_start_matches('-').into()))
        };

        match a {
            "-h" => {
                seen_target_flags.push("-h".into());
                direct_host = Some(need(&mut i, "-h")?);
            }
            "-p" => {
                seen_target_flags.push("-p".into());
                let v = need(&mut i, "-p")?;
                direct_port = Some(
                    v.parse::<u16>()
                        .map_err(|_| ArgError::BadValue("p".into(), v.clone()))?,
                );
            }
            "-s" | "-u" | "--user" | "--askpass" | "--sentinel" | "--cluster" => {
                seen_target_flags.push(a.to_owned());
                if a == "-u" {
                    {
                        let raw = need(&mut i, "-u")?;
                        direct_url = Some(
                            pr_security::uri::parse(&raw)
                                .map_err(|e| ArgError::Uri("u".to_owned(), e))?,
                        );
                    }
                } else if matches!(a, "-s" | "--user") {
                    let _ = need(&mut i, a)?;
                }
            }
            "-n" => {
                let v = need(&mut i, "-n")?;
                inv.db = Some(
                    v.parse::<u32>()
                        .map_err(|_| ArgError::BadValue("n".into(), v.clone()))?,
                );
            }
            "-2" => inv.protocol = Some(2),
            "-3" => inv.protocol = Some(3),
            "--raw" => set_output(&mut inv, OutputMode::Raw)?,
            "--json" => set_output(&mut inv, OutputMode::Json)?,
            "--csv" => set_output(&mut inv, OutputMode::Csv)?,
            "--bytes" => set_output(&mut inv, OutputMode::BytesOnly)?,
            "--output" => {
                let v = need(&mut i, "--output")?;
                let m = match v.as_str() {
                    "pretty" => OutputMode::Pretty,
                    "raw" => OutputMode::Raw,
                    "json" => OutputMode::Json,
                    "csv" => OutputMode::Csv,
                    "typed-json" => OutputMode::TypedJson,
                    "ndjson" => OutputMode::Ndjson,
                    "resp" => OutputMode::Resp,
                    other => return Err(ArgError::BadValue("output".into(), other.into())),
                };
                set_output(&mut inv, m)?;
            }
            "--no-color" => inv.color = Some("never".into()),
            "--color" => inv.color = Some(need(&mut i, "--color")?),
            "--plain" => inv.plain = true,
            "--tui" => inv.tui = true,
            "--tls" => inv.tls.enabled = true,
            "--tls-ca" => {
                inv.tls.enabled = true;
                inv.tls.ca = Some(need(&mut i, "--tls-ca")?);
            }
            // §21.3: the verified name is configured separately from the address, because a
            // node announcing an address you can reach does not get to choose the name its
            // certificate is checked against.
            "--tls-name" => {
                inv.tls.enabled = true;
                inv.tls.name = Some(need(&mut i, "--tls-name")?);
            }
            "--tls-cert" => {
                inv.tls.enabled = true;
                inv.tls.cert = Some(need(&mut i, "--tls-cert")?);
            }
            "--tls-key" => {
                inv.tls.enabled = true;
                inv.tls.key = Some(need(&mut i, "--tls-key")?);
            }
            other => {
                // NET-01: refused here, unconditionally. The previous shape recorded these and
                // rejected them only when a profile was present, which meant
                // `prc -h host --insecure` was accepted and silently ignored — a silent
                // insecure in the exact sense §21.1 forbids, and the worse kind, because the
                // operator believes the flag did something.
                // Recorded rather than refused on the spot, because `@prod` may still be
                // ahead of us in argv and the message is more useful when it can say *whose*
                // verification would have been weakened. It is refused either way, below.
                if TLS_WEAKENING.contains(&other) {
                    weakening.get_or_insert_with(|| other.to_owned());
                    i += 1;
                    continue;
                }
                if let Some(v) = other.strip_prefix("--tls-verify=") {
                    if v == "full" {
                        inv.tls.enabled = true;
                    } else {
                        weakening.get_or_insert_with(|| other.to_owned());
                    }
                    i += 1;
                    continue;
                }
                return Err(ArgError::Unknown(other.to_owned()));
            }
        }
        i += 1;
    }

    // NET-01: 「拒绝或显式配置；无静默 insecure」. Refused whether or not a profile is
    // present. Before V-G04 this was checked only inside the profile branch, so
    // `prc -h host --insecure` parsed, was ignored, and left the operator believing the flag
    // had done something.
    if let Some(w) = weakening {
        return match &profile {
            Some(p) => Err(ArgError::TlsWeakening(
                w.trim_start_matches('-').to_owned(),
                p.clone(),
            )),
            None => Err(ArgError::InsecureTlsRefused(w)),
        };
    }

    // Resolve the target and apply the R26 matrix.
    if let Some(p) = profile {
        if direct_url.is_some() {
            return Err(ArgError::ProfileAndUrl(p));
        }
        for f in &seen_target_flags {
            if let Some(w) = f.strip_prefix("__weaken") {
                return Err(ArgError::TlsWeakening(
                    w.trim_start_matches('-').to_owned(),
                    p,
                ));
            }
            if TARGET_FLAGS.contains(&f.as_str()) {
                return Err(ArgError::TargetConflict(
                    f.trim_start_matches('-').to_owned(),
                    p,
                ));
            }
        }
        inv.target = Target::Profile(p);
    } else if direct_host.is_some() || direct_port.is_some() || direct_url.is_some() {
        inv.target = Target::Direct {
            host: direct_host,
            port: direct_port,
            url: direct_url,
        };
    }

    Ok(inv)
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn p(args: &[&str]) -> Result<Invocation, ArgError> {
        parse(&args.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>())
    }

    fn cmd(inv: &Invocation) -> Vec<Vec<u8>> {
        inv.command.iter().map(|b| b.to_vec()).collect()
    }

    #[test]
    fn bare_invocation_has_no_target() {
        let i = p(&[]).unwrap();
        assert_eq!(i.target, Target::Unspecified);
        assert!(i.command.is_empty());
    }

    #[test]
    fn profile_then_command() {
        let i = p(&["@dev", "HGETALL", "player:10001"]).unwrap();
        assert_eq!(i.target, Target::Profile("dev".into()));
        assert_eq!(cmd(&i), vec![b"HGETALL".to_vec(), b"player:10001".to_vec()]);
    }

    #[test]
    fn everything_after_the_command_belongs_to_redis() {
        // CMD-01: the whole point of §3.2.
        let i = p(&["@dev", "SET", "example", "--raw"]).unwrap();
        assert_eq!(
            cmd(&i),
            vec![b"SET".to_vec(), b"example".to_vec(), b"--raw".to_vec()]
        );
        assert_eq!(
            i.output,
            OutputMode::Auto,
            "--raw after the command is a value, not a flag"
        );

        let i2 = p(&["@dev", "SET", "example", "@prod"]).unwrap();
        assert_eq!(cmd(&i2)[2], b"@prod".to_vec());
        assert_eq!(
            i2.target,
            Target::Profile("dev".into()),
            "@prod after the command is a value"
        );
    }

    #[test]
    fn double_dash_ends_client_options() {
        let i = p(&["@dev", "--", "GET", "--raw"]).unwrap();
        assert_eq!(cmd(&i), vec![b"GET".to_vec(), b"--raw".to_vec()]);
        assert_eq!(i.output, OutputMode::Auto);
    }

    #[test]
    fn client_options_may_appear_before_and_after_the_profile() {
        let i = p(&["--raw", "@dev", "-n", "3", "GET", "k"]).unwrap();
        assert_eq!(i.target, Target::Profile("dev".into()));
        assert_eq!(i.db, Some(3));
        assert_eq!(i.output, OutputMode::Raw);
        assert_eq!(cmd(&i), vec![b"GET".to_vec(), b"k".to_vec()]);
    }

    // ---------------------------------------------------------------- R26 matrix
    #[test]
    fn db_override_is_allowed_with_a_profile() {
        assert_eq!(p(&["@dev", "-n", "3"]).unwrap().db, Some(3));
    }

    #[test]
    fn presentation_flags_are_allowed_with_a_profile() {
        for f in [
            vec!["--raw"],
            vec!["--json"],
            vec!["--csv"],
            vec!["--plain"],
            vec!["--tui"],
            vec!["--no-color"],
            vec!["-2"],
            vec!["-3"],
            vec!["--output", "typed-json"],
            vec!["--color", "always"],
        ] {
            let mut a = vec!["@dev"];
            a.extend(f.iter().copied());
            assert!(p(&a).is_ok(), "{f:?} should be allowed with a profile");
        }
    }

    #[test]
    fn target_changing_flags_conflict_with_a_profile() {
        // The review's point: silently re-pointing a profile ships credentials elsewhere.
        for (flag, extra) in [
            ("-h", Some("other.host")),
            ("-p", Some("6380")),
            ("-u", Some("redis://x")),
            ("--user", Some("bob")),
            ("-s", Some("/tmp/r.sock")),
            ("--askpass", None),
            ("--sentinel", None),
            ("--cluster", None),
        ] {
            let mut a = vec!["@dev", flag];
            if let Some(e) = extra {
                a.push(e);
            }
            let err = p(&a).unwrap_err();
            assert!(
                matches!(
                    err,
                    ArgError::TargetConflict(..) | ArgError::ProfileAndUrl(_)
                ),
                "{flag} with a profile must conflict, got {err:?}"
            );
        }
    }

    #[test]
    fn the_same_flags_are_fine_without_a_profile() {
        let i = p(&["-h", "127.0.0.1", "-p", "6379", "PING"]).unwrap();
        assert_eq!(
            i.target,
            Target::Direct {
                host: Some("127.0.0.1".into()),
                port: Some(6379),
                url: None
            }
        );
        assert_eq!(cmd(&i), vec![b"PING".to_vec()]);
    }

    #[test]
    fn tls_weakening_is_refused_with_a_profile() {
        let err = p(&["@prod", "--insecure"]).unwrap_err();
        assert!(matches!(err, ArgError::TlsWeakening(..)), "got {err:?}");
    }

    #[test]
    fn two_profiles_conflict() {
        assert!(matches!(
            p(&["@dev", "@prod"]),
            Err(ArgError::MultipleProfiles(..))
        ));
    }

    #[test]
    fn profile_plus_url_conflicts() {
        assert!(matches!(
            p(&["@dev", "-u", "redis://h:6379"]),
            Err(ArgError::ProfileAndUrl(_))
        ));
    }

    #[test]
    fn output_modes_are_mutually_exclusive() {
        // §18.1: no silent last-wins.
        assert!(matches!(
            p(&["--raw", "--json"]),
            Err(ArgError::OutputConflict(..))
        ));
        assert!(matches!(
            p(&["--json", "--csv"]),
            Err(ArgError::OutputConflict(..))
        ));
        assert!(matches!(
            p(&["--bytes", "--raw"]),
            Err(ArgError::OutputConflict(..))
        ));
        // Repeating the same mode is harmless.
        assert!(p(&["--raw", "--raw"]).is_ok());
    }

    #[test]
    fn dash_h_is_always_host_never_help() {
        // §3.2: help is --help.
        let i = p(&["-h", "myhost", "PING"]).unwrap();
        match i.target {
            Target::Direct { host, .. } => assert_eq!(host.as_deref(), Some("myhost")),
            other => panic!("expected direct target, got {other:?}"),
        }
    }

    #[test]
    fn protocol_flags_are_recorded_explicitly() {
        assert_eq!(p(&["-3", "@dev"]).unwrap().protocol, Some(3));
        assert_eq!(p(&["-2", "@dev"]).unwrap().protocol, Some(2));
        assert_eq!(
            p(&["@dev"]).unwrap().protocol,
            None,
            "no implicit protocol choice"
        );
    }

    #[test]
    fn missing_and_bad_values_are_usage_errors() {
        assert!(matches!(p(&["-n"]), Err(ArgError::MissingValue(_))));
        assert!(matches!(p(&["-n", "abc"]), Err(ArgError::BadValue(..))));
        assert!(matches!(
            p(&["-p", "99999999"]),
            Err(ArgError::BadValue(..))
        ));
        assert!(matches!(
            p(&["--output", "nope"]),
            Err(ArgError::BadValue(..))
        ));
        assert!(matches!(p(&["--nonsense"]), Err(ArgError::Unknown(_))));
    }

    #[test]
    fn command_bytes_are_preserved_exactly() {
        let i = p(&["@dev", "SET", "k", "a b"]).unwrap();
        assert_eq!(
            cmd(&i)[2],
            b"a b".to_vec(),
            "argv boundaries come from the OS, not re-split"
        );
        assert_eq!(i.command.len(), 3, "SET, k, \"a b\"");
    }

    // ---------------------------------------------------------------- V-G04 / NET-01
    //
    // 「拒绝或显式配置；无静默 insecure」. The refusal and the alternative are one requirement:
    // refusing without saying what to do instead is how a workaround gets invented.

    #[test]
    fn a_flag_that_would_disable_verification_is_refused_with_no_profile_too() {
        // The gap V-G04 found. These used to be recorded and then silently ignored when no
        // profile was present, which is the worst kind of silent insecure: the operator
        // believes the flag did something.
        for flag in ["--insecure", "--no-verify", "--tls-verify=none"] {
            let err = p(&["-h", "redis.internal", flag]).unwrap_err();
            assert!(
                matches!(err, ArgError::InsecureTlsRefused(ref f) if f == flag),
                "{flag} was not refused: {err:?}"
            );
        }
    }

    #[test]
    fn the_refusal_names_the_configured_alternative() {
        // An operator who is only told "no" reaches for the next workaround. The two things
        // §21.3 separates -- the trusted CA and the verified name -- are both named.
        let msg = p(&["-h", "10.0.0.7", "--insecure"])
            .unwrap_err()
            .to_string();
        assert!(msg.contains("--tls-ca"), "{msg}");
        assert!(msg.contains("--tls-name"), "{msg}");
    }

    #[test]
    fn a_profile_still_gets_the_more_specific_refusal() {
        // With a profile, the interesting fact is *whose* verification would be weakened.
        let err = p(&["@prod", "--insecure"]).unwrap_err();
        assert!(
            matches!(err, ArgError::TlsWeakening(_, ref prof) if prof == "prod"),
            "{err:?}"
        );
    }

    #[test]
    fn every_configuring_tls_flag_parses_and_turns_verification_on() {
        // The flags that make the refusal a redirection rather than a removal.
        let inv = p(&[
            "-h",
            "10.0.0.7",
            "--tls-ca",
            "/etc/penguin/ca.pem",
            "--tls-name",
            "redis.internal",
            "--tls-cert",
            "/etc/penguin/client.pem",
            "--tls-key",
            "/etc/penguin/client.key",
        ])
        .unwrap();
        assert!(inv.tls.enabled);
        assert_eq!(inv.tls.ca.as_deref(), Some("/etc/penguin/ca.pem"));
        assert_eq!(inv.tls.name.as_deref(), Some("redis.internal"));
        assert_eq!(inv.tls.cert.as_deref(), Some("/etc/penguin/client.pem"));
        assert_eq!(inv.tls.key.as_deref(), Some("/etc/penguin/client.key"));
        // Every flag the help table advertises must actually parse. The two are generated
        // from one list, and this is what proves the list is not fiction.
        for (flag, _) in TLS_CONFIGURING {
            let argv: Vec<&str> = if *flag == "--tls" {
                vec!["-h", "h", flag]
            } else {
                vec!["-h", "h", flag, "value"]
            };
            let inv = p(&argv).unwrap_or_else(|e| panic!("{flag} does not parse: {e}"));
            assert!(inv.tls.enabled, "{flag} did not turn TLS on");
        }
    }

    #[test]
    fn the_verified_name_is_not_taken_from_the_address() {
        // §21.3 in the argument contract: connecting to an address while checking a different
        // name is configured, not inferred.
        let inv = p(&["-h", "10.0.0.7", "--tls-name", "redis.internal"]).unwrap();
        assert_eq!(inv.tls.name.as_deref(), Some("redis.internal"));
        match &inv.target {
            Target::Direct { host, .. } => assert_eq!(host.as_deref(), Some("10.0.0.7")),
            other => panic!("expected a direct target, got {other:?}"),
        }
    }

    #[test]
    fn tls_verify_full_is_the_only_accepted_value() {
        assert!(p(&["-h", "h", "--tls-verify=full"]).unwrap().tls.enabled);
        for bad in [
            "--tls-verify=none",
            "--tls-verify=optional",
            "--tls-verify=off",
        ] {
            assert!(
                matches!(p(&["-h", "h", bad]), Err(ArgError::InsecureTlsRefused(_))),
                "{bad} was not refused"
            );
        }
    }
}
