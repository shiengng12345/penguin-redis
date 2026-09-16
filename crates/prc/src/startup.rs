//! Startup paths and the idle probes (v2.1 §24.6, V-H01).
//!
//! §24.6 gives three numbers: `prc --help` in 100 ms, an idle REPL under 30 MiB RSS, an idle
//! TUI under 60 MiB. V-H01's question is not whether *our* code fits — it is whether the
//! dependency chain has already eaten the budget before we write a line of feature code.
//!
//! So `prc` links everything: tokio, rusqlite, keyring, crossterm, ratatui, blake3, the
//! embedded catalog. And the fast path is defended by construction rather than by hope:
//!
//! - `--help` reads no catalog, opens no database, claims no terminal and starts no runtime.
//!   The catalog's `is_loaded()` flag lets a test prove it, so making the catalog eager
//!   becomes a failing test rather than a startup regression nobody measures.
//! - the idle probes initialise exactly one subsystem each and report RSS, so the two numbers
//!   are attributable.

use std::fmt::Write as _;

/// What the process was asked to do before any target is considered.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fast {
    /// `--help`.
    Help,
    /// `--version`.
    Version,
    /// `--probe-width` (§14.6): measure this terminal and report what it does.
    ProbeWidth,
    /// `--diagnostics` (§23.4): the support bundle, which says what this build is and nothing
    /// about the session.
    Diagnostics,
    /// Nothing special; carry on to the normal path.
    None,
}

/// Recognise the paths that must stay cheap.
///
/// Checked before argument parsing so that `prc --help` is answered even when the rest of the
/// command line is nonsense — a user asking for help is usually a user who got it wrong.
#[must_use]
pub fn fast_path(argv: &[String]) -> Fast {
    for a in argv {
        match a.as_str() {
            "--help" | "-h" if argv.len() == 1 => return Fast::Help,
            "--help" => return Fast::Help,
            "--version" | "-V" => return Fast::Version,
            "--probe-width" => return Fast::ProbeWidth,
            "--diagnostics" => return Fast::Diagnostics,
            _ => {}
        }
    }
    Fast::None
}

/// The help text.
///
/// Built from a literal rather than assembled from the catalog: help that needs the catalog
/// is help that costs 600 KB of parsing to print.
#[must_use]
pub fn help() -> String {
    concat!(
        "prc — Penguin Redis client\n",
        "\n",
        "USAGE:\n",
        "    prc [@profile | -h HOST -p PORT | -u URL] [OPTIONS] [COMMAND ...]\n",
        "\n",
        "OUTPUT:\n",
        "    --raw                 the official CLI's raw behaviour\n",
        "    --json                the official CLI's JSON (RESP3 unless -2 is given)\n",
        "    --csv                 CSV for one command\n",
        "    --bytes               one blob, no delimiter\n",
        "    --output FORMAT       pretty | raw | json | csv | typed-json | ndjson | resp\n",
        "    --show-pushes         write push frames to stderr as NDJSON events\n",
        "    --trace-wire FILE     record the raw wire bytes, unnormalised\n",
        "\n",
        "PROTOCOL:\n",
        "    -2, -3                choose the RESP version explicitly\n",
        "\n",
        "OTHER:\n",
        "    --mcp-stdio           speak JSON-RPC over stdin/stdout for an MCP host\n",
        "    --probe-width         measure this terminal's character widths (§14.6)\n",
        "    --diagnostics         print a support bundle: build and platform, no session data\n",
        "    --tui                 open the terminal UI\n",
        "    --help                this text\n",
        "    --version             version and the catalog it was built from\n",
    )
    .to_owned()
        + &tls_section()
}

/// The TLS section of `--help`, generated from the parser's own flag table.
///
/// Generated rather than written out, so a flag added to the parser cannot quietly go
/// undocumented — and the closing sentence is part of the contract, not decoration: refusing
/// `--insecure` only works as a redirection if the destination is written where people look.
fn tls_section() -> String {
    let mut s = String::from("\nTLS (\u{a7}21.1, \u{a7}21.3):\n");
    for (flag, detail) in crate::args::TLS_CONFIGURING {
        let _ = writeln!(s, "    {flag}{detail}");
    }
    s.push_str(
        "  There is no option that disables verification. If a certificate does not verify,\n\
         \x20 configure the CA or the name above; reachability is not a reason to stop \
         checking.\n",
    );
    s
}

/// Version text, including which pinned servers the catalog was captured from.
///
/// This one *does* load the catalog: a version string that cannot say which catalog is inside
/// the binary is no use in a bug report.
#[must_use]
pub fn version() -> String {
    let mut s = format!("prc {}\n", env!("CARGO_PKG_VERSION"));
    for (family, detail) in pr_catalog::embedded::provenance() {
        let _ = writeln!(s, "catalog {family}: {detail}");
    }
    match pr_catalog::embedded::verify() {
        Ok(()) => s.push_str("catalog integrity: verified\n"),
        Err(e) => {
            let _ = writeln!(s, "catalog integrity: FAILED — {e}");
        }
    }
    s
}

/// Measure the terminal and report what it does with the characters that terminals disagree
/// about (§14.6).
///
/// Prints the table and says whether tables will switch to the cursor-reset drawing mode.
/// Never run implicitly: it writes to the screen and waits for a reply.
#[must_use]
pub fn probe_width() -> (String, bool) {
    use pr_render::WidthPolicy;
    use pr_terminal::probe;

    let policy = WidthPolicy::from_locale(
        std::env::var("LC_CTYPE")
            .or_else(|_| std::env::var("LANG"))
            .ok()
            .as_deref(),
    );
    match probe::run_on_terminal(policy, std::time::Duration::from_millis(500)) {
        Err(e) => (format!("width probe: {e}\n"), false),
        Ok(samples) => {
            let c = probe::conclude(&samples);
            let mut out = String::from("char      assumed  measured\n");
            for s in &samples {
                let mark = if s.agrees() { ' ' } else { '!' };
                let _ = writeln!(
                    out,
                    "{mark} U+{:04X}  {:>7}  {:>8}",
                    s.ch as u32, s.assumed, s.measured
                );
            }
            let _ = writeln!(
                out,
                "\n{} disagreement(s); tables will draw {:?}",
                c.disagreements.len(),
                c.draw
            );
            (out, c.disagreements.is_empty())
        }
    }
}

/// Which subsystem an idle probe should bring up.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Probe {
    /// Everything an idle REPL holds.
    Repl,
    /// Everything an idle TUI holds, on top of the REPL.
    Tui,
    /// The compiled catalog alone, to attribute its share.
    Catalog,
    /// Catalog loaded, assistance **off** — the baseline for V-F09's difference.
    AssistanceOff,
    /// Catalog loaded, assistance **on** and every §12.5 sub-budget filled to its limit.
    ///
    /// Saturated rather than idle on purpose: an empty working set would measure the cost of
    /// having the feature, and §12.5's 12 MiB is a cap on what it may grow to.
    AssistanceOn,
}

impl Probe {
    /// Parse the probe name.
    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "repl" => Some(Self::Repl),
            "tui" => Some(Self::Tui),
            "catalog" => Some(Self::Catalog),
            "assistance-off" => Some(Self::AssistanceOff),
            "assistance-on" => Some(Self::AssistanceOn),
            _ => None,
        }
    }

    /// Its name, for the report.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Repl => "repl",
            Self::Tui => "tui",
            Self::Catalog => "catalog",
            Self::AssistanceOff => "assistance-off",
            Self::AssistanceOn => "assistance-on",
        }
    }
}

/// Bring `probe` up, hold it, and report RSS.
///
/// The value is *kept alive* across the measurement — a probe that lets the subsystem drop
/// before sampling measures nothing at all.
///
/// # Errors
/// A string describing what could not be initialised.
pub fn run_probe(probe: Probe) -> Result<String, String> {
    let rss = match probe {
        // V-F09's two halves. Both load the catalog first, so the catalog is on *both* sides
        // of the difference and cancels — §24.7 requires the read-only catalog to be excluded
        // from the heap increment and reported separately.
        Probe::AssistanceOff => {
            let catalog = pr_catalog::embedded::merged();
            let rss = pr_core::mem::rss_bytes();
            format_probe(
                probe,
                rss,
                &format!("commands={} assistance=off", catalog.commands.len()),
            )
        }
        Probe::AssistanceOn => {
            let catalog = pr_catalog::embedded::merged();
            let scope = pr_intelligence::ObservationScope::new("probe", "service", 0);
            let mut ws = pr_intelligence::WorkingSet::new();
            ws.saturate(&scope);
            // The purpose-search index belongs on this side of the difference too, and for a
            // while it was not here: `prc` has no code path that calls `Finder::new()` yet, so
            // the linker dropped the whole phrase table and the "assistance heap" number was
            // measured without the largest thing in it. The probe is what decides what counts
            // as assistance, so it has to build everything that will be built.
            let finder = pr_intelligence::find::Finder::new();
            let rss = pr_core::mem::rss_bytes();
            let detail = format!(
                "commands={} assistance=on keys={} fields={}B accounted={}B find_commands={}",
                catalog.commands.len(),
                ws.keys.len(),
                ws.fields.bytes(),
                ws.accounted_bytes(),
                finder.len()
            );
            // Kept alive across the sample: a probe that lets the thing being measured drop
            // before sampling measures nothing at all.
            drop(ws);
            drop(finder);
            format_probe(probe, rss, &detail)
        }
        Probe::Catalog => {
            let n = pr_catalog::embedded::merged().commands.len();
            let rss = pr_core::mem::rss_bytes();
            format_probe(probe, rss, &format!("commands={n}"))
        }
        Probe::Repl => {
            // What an idle REPL actually holds: the line editor, the terminal coordinator,
            // the renderer's theme, the result store and the compiled catalog.
            let mut buffer = pr_repl::LineBuffer::new();
            buffer.set_text("");
            let theme =
                pr_render::Theme::new(pr_render::ThemeKind::Dark, pr_render::ColorDepth::TrueColor);
            let coordinator = pr_terminal::Coordinator::new(pr_terminal::Capture::new(), 80, 24)
                .map_err(|e| e.to_string())?;
            let catalog = pr_catalog::embedded::merged();
            let rss = pr_core::mem::rss_bytes();
            let detail = format!(
                "commands={} theme={:?} rows={}",
                catalog.commands.len(),
                theme.background(),
                coordinator.size().1
            );
            drop(coordinator);
            format_probe(probe, rss, &detail)
        }
        Probe::Tui => {
            let mut tui = pr_tui::Idle::new(200, 60).map_err(|e| e.to_string())?;
            tui.draw("@r2 / DB 0 / result #17")
                .map_err(|e| e.to_string())?;
            let catalog = pr_catalog::embedded::merged();
            let rss = pr_core::mem::rss_bytes();
            let detail = format!(
                "commands={} lines={}",
                catalog.commands.len(),
                tui.lines().len()
            );
            format_probe(probe, rss, &detail)
        }
    };
    Ok(rss)
}

fn format_probe(probe: Probe, rss: Option<u64>, detail: &str) -> String {
    match rss {
        // Reported in bytes, not megabytes: rounding at the source is how a budget quietly
        // gains 500 KB of headroom.
        Some(b) => format!("probe={} rss_bytes={b} {detail}\n", probe.name()),
        None => format!("probe={} rss_bytes=unavailable {detail}\n", probe.name()),
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn argv(a: &[&str]) -> Vec<String> {
        a.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn help_is_recognised_even_alongside_a_broken_command_line() {
        // Someone asking for help usually got the rest wrong.
        assert_eq!(fast_path(&argv(&["--help"])), Fast::Help);
        assert_eq!(
            fast_path(&argv(&["@nosuch", "--bogus", "--help"])),
            Fast::Help
        );
        assert_eq!(fast_path(&argv(&["--version"])), Fast::Version);
        assert_eq!(fast_path(&argv(&["-V"])), Fast::Version);
        assert_eq!(fast_path(&argv(&["GET", "k"])), Fast::None);
    }

    #[test]
    fn a_bare_dash_h_is_help_but_dash_h_with_a_host_is_not() {
        // `-h` means "host" in the official CLI, and silently turning `prc -h myhost` into a
        // help screen would be a surprising incompatibility.
        assert_eq!(fast_path(&argv(&["-h"])), Fast::Help);
        assert_eq!(fast_path(&argv(&["-h", "redis.example"])), Fast::None);
    }

    #[test]
    fn the_help_text_names_every_output_mode() {
        let h = help();
        for mode in [
            "--raw",
            "--json",
            "--csv",
            "--bytes",
            "typed-json",
            "ndjson",
            "resp",
            "--show-pushes",
            "--trace-wire",
        ] {
            assert!(h.contains(mode), "help does not mention {mode}");
        }
    }

    #[test]
    fn version_reports_the_catalog_it_was_built_from() {
        let v = version();
        assert!(v.starts_with("prc "));
        assert!(v.contains("catalog redis:"));
        assert!(v.contains("catalog valkey:"));
        assert!(v.contains("@sha256:"), "the pinned image is named: {v}");
        assert!(v.contains("catalog integrity: verified"), "{v}");
    }

    #[test]
    fn probe_width_is_its_own_fast_path_and_is_documented() {
        assert_eq!(fast_path(&argv(&["--probe-width"])), Fast::ProbeWidth);
        assert!(help().contains("--probe-width"));
    }

    #[test]
    fn probe_width_against_a_pipe_reports_rather_than_hangs() {
        // Under `cargo test` stdout is not a terminal, which is the case that must not write
        // an escape sequence into the user's data and then wait forever.
        let (text, agreed) = probe_width();
        assert!(!agreed);
        assert!(text.contains("not a tty"), "{text}");
    }

    #[test]
    fn probe_names_round_trip() {
        for p in [Probe::Repl, Probe::Tui, Probe::Catalog] {
            assert_eq!(Probe::parse(p.name()), Some(p));
        }
        assert_eq!(Probe::parse("nosuch"), None);
    }

    #[test]
    fn a_probe_reports_a_plausible_number() {
        let out = run_probe(Probe::Catalog).unwrap();
        assert!(out.starts_with("probe=catalog rss_bytes="), "{out}");
        // Both families merged: ~577 Redis plus the Valkey-only entries.
        let n: usize = out
            .split("commands=")
            .nth(1)
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        assert!(n > 500, "the whole catalog is loaded, got {n}");
        let n: u64 = out
            .split("rss_bytes=")
            .nth(1)
            .unwrap()
            .split(' ')
            .next()
            .unwrap()
            .parse()
            .expect("rss is a number on this platform");
        assert!(n > 1024 * 1024, "implausibly small: {n}");
    }

    #[test]
    fn help_lists_every_tls_flag_that_configures_verification_and_none_that_removes_it() {
        // A flag nobody can find is a flag nobody uses, and the refusal in `args` only works
        // as a redirection if the destination is written down where people look.
        let h = help();
        for (flag, _) in crate::args::TLS_CONFIGURING {
            assert!(h.contains(flag), "--help does not mention {flag}");
        }
        for banned in ["--insecure", "--no-verify", "--tls-verify=none"] {
            assert!(!h.contains(banned), "--help advertises {banned}");
        }
        assert!(h.contains("no option that disables verification"));
    }
}
