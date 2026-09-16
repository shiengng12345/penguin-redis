//! L2 plugin isolation: the host side (v2.1 §30.4, R45, V-D09).
//!
//! §30.4 settles a question v2.0 got wrong, and says why:
//!
//! > 不存在「进程内加载原生动态库」的级别：Rust 的 panic/abort/OOM 无法在同进程内可靠隔离。
//!
//! So an L2 plugin is a separate executable and the isolation is the OS process boundary. That
//! makes the host's job narrow and checkable: spawn it, feed it, take at most N bytes back,
//! kill it after T, and **never let anything it does become something the host does**.
//!
//! Every failure ends the same way — the caller falls back to the generic view and the request
//! that already succeeded is untouched. A plugin is a rendering preference, and a rendering
//! preference must not be able to lose data the user already has.
//!
//! ## What a plugin does not get
//!
//! §30.4: 「L2 插件默认 deny，逐项授权，参数与返回按 `SafeText` 处理，不获得 credential store、
//! 网络或文件系统能力（除显式声明并批准的路径）」.
//!
//! - **Default deny.** [`PluginHost::call`] refuses a plugin that is not in the grant list
//!   *before* spawning anything. Not "spawns it and ignores the result" — does not spawn.
//! - **A scrubbed environment.** The child gets an explicitly built environment rather than an
//!   inherited one. Inheriting and then removing known-bad names is the wrong default: it
//!   fails open for every variable nobody thought of.
//! - **`SafeText` on the way out.** A plugin's output is no more trusted than a server's.

use pr_core::SafeText;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// Limits applied to one plugin call (§30.4's "预算执行" column).
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    /// Killed after this.
    pub timeout: Duration,
    /// Output past this is discarded and the call fails.
    ///
    /// Truncation is a *failure*, not a shortened success: half a rendering is not a rendering,
    /// and showing one would be the silent truncation §24.4 forbids.
    pub max_output_bytes: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            // Generous enough that a slow machine is not a failure, short enough that a user
            // waiting for a table notices nothing.
            timeout: Duration::from_secs(2),
            // A rendering, not a data channel. The result store is where large data lives.
            max_output_bytes: 1024 * 1024,
        }
    }
}

/// Why a plugin call did not produce a rendering.
///
/// Every variant means the same thing to the caller — use the generic view — and they are
/// kept apart anyway, because "your plugin is slow" and "your plugin crashed" are different
/// things to tell someone, and an operator who is told neither will blame Penguin.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PluginFailure {
    /// Not in the grant list. Nothing was spawned.
    NotGranted(String),
    /// The executable could not be started.
    SpawnFailed(String),
    /// Killed at the timeout.
    TimedOut {
        /// The limit it passed.
        after: Duration,
    },
    /// Exited non-zero, or was killed by a signal.
    Crashed {
        /// Exit status as text, including the signal where there was one.
        status: String,
    },
    /// Produced more than the output budget.
    OutputTooLarge {
        /// The limit.
        limit: usize,
    },
    /// The output was not a rendering this host understands.
    Malformed(String),
}

impl PluginFailure {
    /// What to tell the user, above the generic view.
    #[must_use]
    pub fn message(&self, plugin: &str) -> String {
        match self {
            Self::NotGranted(p) => format!(
                "the plugin {p} is not granted, so it was not run. Plugins are denied by \
                 default and granted one at a time (§30.4)"
            ),
            Self::SpawnFailed(e) => format!("{plugin} could not be started: {e}"),
            Self::TimedOut { after } => format!(
                "{plugin} was still running after {after:?} and was stopped; showing the \
                 built-in view instead"
            ),
            Self::Crashed { status } => format!(
                "{plugin} stopped unexpectedly ({status}); showing the built-in view instead. \
                 Your results are unaffected"
            ),
            Self::OutputTooLarge { limit } => format!(
                "{plugin} produced more than {limit} bytes, which is more than a rendering \
                 should be; showing the built-in view instead"
            ),
            Self::Malformed(why) => format!(
                "{plugin} produced output this version does not understand ({why}); showing \
                 the built-in view instead"
            ),
        }
    }
}

/// A rendering a plugin produced.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rendering {
    /// Lines, already through [`SafeText`].
    pub lines: Vec<SafeText>,
}

/// A plugin the user granted, one at a time.
#[derive(Clone, Debug)]
pub struct Grant {
    /// The name used in configuration and in messages.
    pub name: String,
    /// The executable.
    pub program: PathBuf,
    /// Fixed arguments. Nothing from the rendered data reaches argv — a value that becomes an
    /// argument is an injection waiting for the right value.
    pub args: Vec<String>,
}

/// Runs granted L2 plugins and survives them.
#[derive(Debug, Default)]
pub struct PluginHost {
    granted: Vec<Grant>,
}

impl PluginHost {
    /// A host with nothing granted, which is the default a user starts from.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Grant one plugin.
    #[must_use]
    pub fn grant(mut self, g: Grant) -> Self {
        self.granted.push(g);
        self
    }

    /// Whether a name is granted, without running anything.
    #[must_use]
    pub fn is_granted(&self, name: &str) -> bool {
        self.granted.iter().any(|g| g.name == name)
    }

    /// Call a plugin with `input` and take at most `limits.max_output_bytes` back.
    ///
    /// # Errors
    /// A [`PluginFailure`]. Every one of them means "use the generic view".
    pub fn call(
        &self,
        name: &str,
        input: &str,
        limits: Limits,
    ) -> Result<Rendering, PluginFailure> {
        let Some(grant) = self.granted.iter().find(|g| g.name == name) else {
            // Default deny, enforced before anything is spawned.
            return Err(PluginFailure::NotGranted(name.to_owned()));
        };
        let raw = run(grant, input, limits)?;
        parse(&raw)
    }
}

/// Spawn, feed, bound, and reap. Split out so the bounding logic is one readable thing.
fn run(grant: &Grant, input: &str, limits: Limits) -> Result<Vec<u8>, PluginFailure> {
    let mut cmd = Command::new(&grant.program);
    cmd.args(&grant.args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null());

    // Built, not inherited. Inheriting and then removing known-bad names fails open for every
    // variable nobody thought of, and a credential path is exactly the kind of thing nobody
    // thinks of. `PATH` is not passed either: the program is an absolute path by the time it
    // gets here, so the child has no reason to resolve anything.
    cmd.env_clear();
    cmd.env("PENGUIN_PLUGIN_API", "1");

    let mut child = cmd
        .spawn()
        .map_err(|e| PluginFailure::SpawnFailed(e.to_string()))?;

    // The input goes in on its own thread. Writing inline would deadlock against a plugin
    // that fills its output pipe before reading its input — and "flood" does exactly that.
    if let Some(mut stdin) = child.stdin.take() {
        let payload = input.as_bytes().to_vec();
        std::thread::spawn(move || {
            let _ = stdin.write_all(&payload);
            // Dropping closes the pipe, which is how the plugin learns the input ended.
        });
    }

    // `stdout` was configured as a pipe just above, so this is `Some`. Returning an error
    // rather than unwrapping keeps the crate free of panic paths (workspace lint).
    let Some(mut stdout) = child.stdout.take() else {
        let _ = child.kill();
        let _ = child.wait();
        return Err(PluginFailure::SpawnFailed(
            "the plugin's stdout pipe was not created".to_owned(),
        ));
    };
    let (tx, rx) = std::sync::mpsc::channel::<Result<Vec<u8>, PluginFailure>>();
    let cap = limits.max_output_bytes;
    let reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let mut chunk = vec![0u8; 64 * 1024];
        loop {
            match stdout.read(&mut chunk) {
                Ok(0) | Err(_) => break,
                Ok(n) => {
                    if buf.len() + n > cap {
                        // Stop reading *and stop growing*. A host that read to the end of the
                        // stream would be the thing that died here, not the plugin.
                        let _ = tx.send(Err(PluginFailure::OutputTooLarge { limit: cap }));
                        return;
                    }
                    buf.extend_from_slice(&chunk[..n]);
                }
            }
        }
        let _ = tx.send(Ok(buf));
    });

    let started = Instant::now();
    let outcome = loop {
        if let Ok(r) = rx.try_recv() {
            break r;
        }
        match child.try_wait() {
            Ok(Some(_)) => {
                // Exited. Give the reader a moment to drain what is still in the pipe, then
                // take whatever it has.
                break rx
                    .recv_timeout(Duration::from_millis(500))
                    .unwrap_or_else(|_| Ok(Vec::new()));
            }
            Ok(None) => {}
            Err(e) => break Err(PluginFailure::SpawnFailed(e.to_string())),
        }
        if started.elapsed() > limits.timeout {
            break Err(PluginFailure::TimedOut {
                after: limits.timeout,
            });
        }
        std::thread::sleep(Duration::from_millis(2));
    };

    // Kill unconditionally and then wait. `kill` on an exited child is harmless; skipping the
    // `wait` is not — it leaves a zombie per call, and a renderer is called often.
    let _ = child.kill();
    let status = child.wait();
    drop(reader);

    match outcome {
        Err(e) => Err(e),
        Ok(buf) => match status {
            Ok(s) if s.success() => Ok(buf),
            Ok(s) => Err(PluginFailure::Crashed {
                status: describe(s),
            }),
            Err(e) => Err(PluginFailure::Crashed {
                status: e.to_string(),
            }),
        },
    }
}

/// Exit status as text, naming the signal where there was one.
///
/// "exit status: 134" tells an operator nothing; "killed by signal 6 (SIGABRT)" tells them the
/// plugin aborted, which is a different conversation from "the plugin returned an error".
fn describe(s: std::process::ExitStatus) -> String {
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt as _;
        if let Some(sig) = s.signal() {
            let name = match sig {
                2 => " (SIGINT)",
                6 => " (SIGABRT)",
                9 => " (SIGKILL)",
                11 => " (SIGSEGV)",
                15 => " (SIGTERM)",
                _ => "",
            };
            return format!("killed by signal {sig}{name}");
        }
    }
    match s.code() {
        Some(c) => format!("exit status {c}"),
        None => "ended without an exit status".to_owned(),
    }
}

/// Parse the typed output (§18.4's shape, reduced to what a renderer needs).
///
/// Hand-parsed rather than pulled through a JSON crate because the shape is two fields and the
/// interesting property is what happens to the *contents*: every line goes through
/// [`SafeText`] before it is a line. A plugin is no more trusted than a server (§23.6).
fn parse(raw: &[u8]) -> Result<Rendering, PluginFailure> {
    let text = std::str::from_utf8(raw)
        .map_err(|e| PluginFailure::Malformed(format!("output is not UTF-8: {e}")))?;
    let text = text.trim();
    if text.is_empty() {
        return Err(PluginFailure::Malformed("no output".to_owned()));
    }
    let v: serde_json::Value = serde_json::from_str(text)
        .map_err(|e| PluginFailure::Malformed(format!("not JSON: {e}")))?;
    let Some(lines) = v.get("lines").and_then(|l| l.as_array()) else {
        return Err(PluginFailure::Malformed(
            "no `lines` array in the output".to_owned(),
        ));
    };
    let mut out = Vec::with_capacity(lines.len());
    for l in lines {
        let Some(s) = l.as_str() else {
            return Err(PluginFailure::Malformed(
                "a line is not a string".to_owned(),
            ));
        };
        // The one place plugin output becomes displayable, and it is not a choice made per
        // call site.
        out.push(SafeText::from_bytes(bytes::Bytes::from(
            s.as_bytes().to_vec(),
        )));
    }
    Ok(Rendering { lines: out })
}

/// Where the fixture plugin is, for tests — building it first if it is not there.
///
/// Cargo does not build a dependency's *binaries*, only its lib, and `plugin-fixtures` has no
/// lib to depend on. Rather than leave `cargo test` failing on a clean checkout with a message
/// about a missing file, the first caller builds it. `OnceLock` so fifty tests running in
/// parallel produce one `cargo build`, not fifty.
///
/// # Errors
/// A message naming what failed, because "not found" sends a reader looking in the wrong place.
pub fn fixture_plugin(root: &Path) -> Result<PathBuf, String> {
    static BUILT: std::sync::OnceLock<Result<PathBuf, String>> = std::sync::OnceLock::new();
    BUILT.get_or_init(|| build_fixture_plugin(root)).clone()
}

fn fixture_plugin_path(root: &Path) -> Option<PathBuf> {
    let exe = if cfg!(windows) {
        "misbehaving-plugin.exe"
    } else {
        "misbehaving-plugin"
    };
    ["debug", "release"]
        .iter()
        .map(|p| root.join("target").join(p).join(exe))
        .find(|p| p.exists())
}

fn build_fixture_plugin(root: &Path) -> Result<PathBuf, String> {
    if let Some(p) = fixture_plugin_path(root) {
        return Ok(p);
    }
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_owned());
    let out = Command::new(cargo)
        .args(["build", "-p", "plugin-fixtures"])
        .current_dir(root)
        .output()
        .map_err(|e| format!("could not run cargo to build plugin-fixtures: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "building plugin-fixtures failed:\n{}",
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    fixture_plugin_path(root)
        .ok_or_else(|| "plugin-fixtures built but the binary is not where it should be".to_owned())
}
