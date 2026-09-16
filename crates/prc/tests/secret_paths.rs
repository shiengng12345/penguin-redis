//! V-D06 / SEC-01 — the secret-path audit (v2.1 §23.4, §19.5, R24, R41).
//!
//! §23.4 names ten ways a password can leave this process, and its method is one sentence:
//! 每路径注入已知 token 后全量 grep. That is what this file does. One token, ten paths, and a
//! grep of everything each path produced.
//!
//! It lives in `prc` because `prc` is the only crate that can see all ten: the URI parser in
//! `pr-security`, the clipboard boundary in `pr-render`, the wire trace in `pr-application`,
//! the history in `pr-repl`, and the panic hook in the binary itself. A test per crate would
//! have nowhere to stand and nothing to compare.
//!
//! **The token is unique per path.** A single shared token would let one path's scrubbing
//! cover for another's: with everything registered, a value that leaked from the clipboard
//! would be removed by the time the trace was written. Ten tokens keep the ten answers
//! independent.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic, missing_docs)]

use pr_security::sink::Destination;
use std::path::{Path, PathBuf};

/// A token nothing else in the workspace contains.
fn token(path: &str) -> String {
    format!("v-d06-{path}-hunter2-c0ffee")
}

fn prc() -> PathBuf {
    if let Ok(p) = std::env::var("PRC_BINARY") {
        return PathBuf::from(p);
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("repo root");
    for profile in ["debug", "release"] {
        let p = root
            .join("target")
            .join(profile)
            .join(format!("prc{}", std::env::consts::EXE_SUFFIX));
        if p.exists() {
            return p;
        }
    }
    panic!("prc is not built; run `cargo build -p prc`");
}

/// Everything a run produced, for grepping.
struct Produced {
    what: &'static str,
    bytes: Vec<u8>,
}

fn assert_absent(secret: &str, produced: &[Produced]) {
    for p in produced {
        let hay = String::from_utf8_lossy(&p.bytes);
        assert!(!hay.contains(secret), "{} leaked the token:\n{hay}", p.what);
    }
    // And the run has to have produced *something*. A path that emits nothing passes a grep
    // trivially, which is the way this audit could quietly stop testing anything.
    assert!(
        produced.iter().any(|p| !p.bytes.is_empty()),
        "every artifact was empty, so the grep proved nothing"
    );
}

// =============================================================================================
// 1. URI parse error
// =============================================================================================

#[test]
fn path_01_a_uri_parse_error_never_repeats_the_password() {
    let secret = token("uri");
    // Through the real binary, because the thing being tested is what a person sees on their
    // terminal after pasting a URI that does not work.
    let out = std::process::Command::new(prc())
        .arg("-u")
        .arg(format!("redis://alice:{secret}@host:notaport/0"))
        .output()
        .expect("run prc");
    assert!(!out.status.success(), "a bad URI must not be accepted");
    assert_absent(
        &secret,
        &[
            Produced {
                what: "stdout",
                bytes: out.stdout,
            },
            Produced {
                what: "stderr",
                bytes: out.stderr,
            },
        ],
    );
}

// =============================================================================================
// 2. TLS error
// =============================================================================================

#[test]
fn path_02_a_tls_error_never_contains_key_material() {
    let secret = token("tls");
    assert!(pr_security::secrets::remember(secret.as_bytes()));
    // A CA bundle that is not one, with the secret inside it. §21.1's rule is that a bad bundle
    // is an error rather than a silent fall back to the system roots, so this is a real error
    // path with real key-shaped material in it.
    let err = pr_transport::tls::TlsConfig::new(
        format!("-----BEGIN CERTIFICATE-----\n{secret}\n-----END CERTIFICATE-----\n").as_bytes(),
        "server.example",
    )
    .expect_err("a bundle that is not a certificate must be refused");
    let rendered = format!("{err} | {err:?}");
    assert_absent(
        &secret,
        &[Produced {
            what: "the TLS error",
            bytes: rendered.into_bytes(),
        }],
    );
}

// =============================================================================================
// 3. `Debug`
// =============================================================================================

#[test]
fn path_03_no_debug_implementation_prints_a_secret() {
    let secret = token("debug");
    let uri = pr_security::uri::parse(&format!("redis://alice:{secret}@h:6379/0")).unwrap();
    let debugged = format!("{uri:?} {uri:#?}");
    assert!(debugged.contains("has_password: true"), "{debugged}");
    assert_absent(
        &secret,
        &[Produced {
            what: "Debug of a parsed URI",
            bytes: debugged.into_bytes(),
        }],
    );
}

/// The same question asked of the source rather than of one value.
///
/// A `#[derive(Debug)]` on a struct with a secret field is a leak that looks like nothing, and
/// it reaches a person through a panic message, a log line or `dbg!`. This is a source
/// assertion for the same reason `pr-transport`'s "there is no insecure mode" test is one: the
/// mistake is invisible at every call site and obvious at the definition.
#[test]
fn path_03b_no_type_derives_debug_over_a_field_that_holds_a_secret() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("repo root")
        .join("crates");
    let mut offenders = Vec::new();
    let mut files = Vec::new();
    collect_rs(&root, &mut files);
    for f in files {
        let text = std::fs::read_to_string(&f).unwrap_or_default();
        let lines: Vec<&str> = text.lines().collect();
        for (i, line) in lines.iter().enumerate() {
            if !line.contains("#[derive(") || !line.contains("Debug") {
                continue;
            }
            // Look at the fields of the item this derive is attached to, up to its closing
            // brace or the next item.
            for l in lines.iter().skip(i + 1).take(60) {
                if l.starts_with('}') {
                    break;
                }
                let name = l.trim().trim_start_matches("pub ");
                let Some((field, _)) = name.split_once(':') else {
                    continue;
                };
                let field = field.trim();
                if SECRET_FIELD_NAMES.contains(&field) {
                    offenders.push(format!("{}: field `{field}`", f.display()));
                }
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "these types derive Debug over a field that holds a secret; write the impl by hand and \
         leave the field out, as `uri::Redis` and `TlsConfig` do:\n{}",
        offenders.join("\n")
    );
}

/// Field names that mean "this is a secret".
///
/// Deliberately exact rather than substring: `key` is the most common word in this codebase and
/// almost never means a cryptographic one, and a check that fires on every `key: Vec<u8>` is a
/// check somebody turns off.
const SECRET_FIELD_NAMES: &[&str] = &[
    "password",
    "passphrase",
    "secret",
    "private_key",
    "key_der",
    "credential",
    "token_value",
];

fn collect_rs(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            collect_rs(&p, out);
        } else if p.extension().and_then(|x| x.to_str()) == Some("rs") {
            out.push(p);
        }
    }
}

// =============================================================================================
// 4. trace
// =============================================================================================

#[test]
fn path_04_a_trace_line_is_scrubbed() {
    let secret = token("trace");
    assert!(pr_security::secrets::remember(secret.as_bytes()));
    let line = pr_security::emit(
        Destination::Trace,
        &format!("connecting to h:6379 as alice/{secret}"),
    );
    assert!(line.text().contains("connecting to h:6379"));
    assert_absent(
        &secret,
        &[Produced {
            what: "a trace line",
            bytes: line.text().as_bytes().to_vec(),
        }],
    );
}

// =============================================================================================
// 5. history
// =============================================================================================

#[test]
fn path_05_the_history_file_on_disk_contains_no_password() {
    let secret = token("history");
    let dir = std::env::temp_dir().join(format!("pr-vd06-history-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("history.sqlite3");
    {
        let h = pr_repl::History::open(&path).unwrap();
        let scope = pr_repl::history::Scope {
            profile_uuid: "prod".to_owned(),
            identity_epoch: 1,
            db: 0,
        };
        h.record(
            &scope,
            &[b"AUTH".to_vec(), secret.clone().into_bytes()],
            pr_repl::history::RedactionLevel::for_environment("prod"),
            1_700_000_000_000,
        )
        .unwrap();
    }
    // The file, not the API: an accessor that redacts on the way out is no help if the bytes
    // are on the disk. §23.4's method is a grep of what was produced.
    let raw = std::fs::read(&path).unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    assert_absent(
        &secret,
        &[Produced {
            what: "the history database file",
            bytes: raw,
        }],
    );
}

// =============================================================================================
// 6. clipboard
// =============================================================================================

#[test]
fn path_06_the_clipboard_gets_neither_the_password_nor_an_escape() {
    let secret = token("clipboard");
    assert!(pr_security::secrets::remember(secret.as_bytes()));
    let out = pr_render::clipboard::prepare(format!("\x1b]52;c;x\x07 value={secret}").as_bytes());
    assert!(!out.text().contains('\x1b'), "{}", out.text());
    assert_absent(
        &secret,
        &[Produced {
            what: "the clipboard payload",
            bytes: out.text().as_bytes().to_vec(),
        }],
    );
}

// =============================================================================================
// 7. crash report
// =============================================================================================

#[test]
fn path_07_a_panic_message_is_scrubbed_before_it_reaches_the_terminal() {
    let secret = token("panic");
    let out = std::process::Command::new(prc())
        .env("PR_PHASE0_PANIC", &secret)
        .output()
        .expect("run prc");
    assert!(!out.status.success(), "the probe must actually panic");
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    assert!(
        stderr.contains("deliberate panic"),
        "the panic did not happen: {stderr}"
    );
    assert!(
        stderr.contains("[redacted]"),
        "nothing was redacted: {stderr}"
    );
    assert_absent(
        &secret,
        &[
            Produced {
                what: "panic stderr",
                bytes: out.stderr,
            },
            Produced {
                what: "panic stdout",
                bytes: out.stdout,
            },
        ],
    );
}

// =============================================================================================
// 8. --trace-wire
// =============================================================================================

#[test]
fn path_08_the_wire_trace_keeps_the_frames_and_drops_the_password() {
    let secret = token("wire");
    assert!(pr_security::secrets::remember(secret.as_bytes()));
    let mut e =
        pr_application::output::Emitter::new(pr_application::output::OutputMode::Raw, false)
            .with_wire_trace();
    // The bytes as they really crossed the socket: the AUTH that carried the password, and the
    // reply to it.
    let wire = format!(
        "*2\r\n$4\r\nAUTH\r\n${}\r\n{secret}\r\n+OK\r\n",
        secret.len()
    );
    let value = pr_protocol::Value::Simple(bytes::Bytes::from_static(b"OK"));
    e.accept(&pr_application::output::Arrival {
        value: &value,
        wire: wire.as_bytes(),
    });

    // What crossed the socket still has it -- that is what the socket carried, and a test that
    // compares against the wire needs the truth.
    assert!(
        String::from_utf8_lossy(e.wire_trace().unwrap()).contains(&secret),
        "the in-memory trace should be the real bytes"
    );
    // What may be written to a file does not.
    let for_file = e.wire_trace_for_file().unwrap();
    assert!(
        String::from_utf8_lossy(for_file.bytes()).contains("AUTH"),
        "the capture must still be a capture"
    );
    assert_absent(
        &secret,
        &[Produced {
            what: "the wire trace as written to a file",
            bytes: for_file.bytes().to_vec(),
        }],
    );
}

// =============================================================================================
// 9. diagnostics bundle
// =============================================================================================

#[test]
fn path_09_the_support_bundle_says_nothing_about_the_session() {
    let secret = token("bundle");
    let out = std::process::Command::new(prc())
        .arg("--diagnostics")
        .env("PR_PHASE0_PANIC_UNUSED", &secret)
        .output()
        .expect("run prc");
    assert!(out.status.success());
    let text = String::from_utf8_lossy(&out.stdout).into_owned();
    assert!(text.contains("telemetry: none"), "{text}");
    // The environment is where a token most often is, and a bundle that dumps it is the
    // classic version of this leak.
    assert_absent(
        &secret,
        &[Produced {
            what: "the diagnostics bundle",
            bytes: out.stdout,
        }],
    );
}

// =============================================================================================
// 10. telemetry
// =============================================================================================

#[test]
fn path_10_there_is_nowhere_for_anything_to_be_sent() {
    assert!(
        pr_security::diagnostics::TELEMETRY_ENDPOINTS.is_empty(),
        "an endpoint appeared: {:?}",
        pr_security::diagnostics::TELEMETRY_ENDPOINTS
    );
    // And the claim `prc --diagnostics` makes about itself has to match.
    let out = std::process::Command::new(prc())
        .arg("--diagnostics")
        .output()
        .expect("run prc");
    assert!(String::from_utf8_lossy(&out.stdout).contains("telemetry: none"));
}

// =============================================================================================
// The audit itself
// =============================================================================================

#[test]
fn all_ten_paths_named_by_section_23_4_have_a_test_here() {
    // The failure this guards is the quiet one: a destination is added to `Destination`, and
    // the audit keeps passing because it never knew about it. Every variant must be exercised
    // by a test in this file, matched by its name.
    let source = include_str!("secret_paths.rs");
    let body = source
        .split("fn all_ten_paths_named_by_section_23_4_have_a_test_here")
        .next()
        .expect("this function is in this file");
    let mut missing = Vec::new();
    for d in Destination::ALL {
        let key = d.name().replace('-', "_");
        let stem = key.split('_').next().unwrap_or(&key);
        if !body.contains(&key) && !body.contains(stem) {
            missing.push(d.name());
        }
    }
    assert!(
        missing.is_empty(),
        "these §23.4 destinations have no test in this file: {missing:?}"
    );
    assert_eq!(Destination::ALL.len(), 10);
}
