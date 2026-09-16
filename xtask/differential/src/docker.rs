//! Two independent servers from one pinned digest (v2.1 §32.2, ADR-011).
//!
//! "Two independent servers" is not a nicety. The alternative — one server, both clients in
//! turn — makes every write case meaningless, because the second client sees the first one's
//! effects. §32.2 says so in as many words, and this module is how the harness obeys it.

use std::process::{Command, Stdio};

/// Why the harness could not get a server.
#[derive(Debug, thiserror::Error)]
pub enum DockerError {
    /// `docker` is not on PATH or refused to run.
    #[error("docker is unavailable: {0}")]
    Unavailable(String),
    /// A container did not start, or did not answer PING in time.
    #[error("{0}")]
    Start(String),
}

/// A running server, torn down when dropped.
#[derive(Debug)]
pub struct Server {
    name: String,
    port: u16,
}

impl Server {
    /// Start a container from `image` (a digest reference) and wait until it answers.
    ///
    /// # Errors
    /// [`DockerError`] if docker is missing or the server never became ready.
    pub fn start(name: &str, image: &str) -> Result<Self, DockerError> {
        ensure_docker()?;
        // A leftover from a killed run would otherwise be adopted silently, and its state is
        // not the state this run seeded.
        let _ = run(&["rm", "-f", name]);

        let out = run(&[
            "run",
            "-d",
            "--name",
            name,
            "-P",
            "--health-cmd",
            "redis-cli ping",
            image,
        ])
        .map_err(DockerError::Start)?;
        if out.trim().is_empty() {
            return Err(DockerError::Start(format!(
                "{name} produced no container id"
            )));
        }

        let port = published_port(name)?;
        let me = Self {
            name: name.to_owned(),
            port,
        };
        me.wait_ready()?;
        Ok(me)
    }

    /// The host port the server is published on.
    #[must_use]
    pub fn port(&self) -> u16 {
        self.port
    }

    /// `127.0.0.1:<port>`, for the proxy's upstream.
    #[must_use]
    pub fn address(&self) -> String {
        format!("127.0.0.1:{}", self.port)
    }

    /// The container name, for `docker exec`.
    #[must_use]
    pub fn container(&self) -> &str {
        &self.name
    }

    fn wait_ready(&self) -> Result<(), DockerError> {
        for _ in 0..100 {
            if let Ok(out) = run(&["exec", &self.name, "redis-cli", "PING"])
                && out.trim() == "PONG"
            {
                return Ok(());
            }
            std::thread::sleep(std::time::Duration::from_millis(100));
        }
        Err(DockerError::Start(format!(
            "{} never answered PING",
            self.name
        )))
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        let _ = run(&["rm", "-f", &self.name]);
    }
}

/// Is docker usable here?
///
/// # Errors
/// [`DockerError::Unavailable`] with whatever docker said, so a CI failure names the cause.
pub fn ensure_docker() -> Result<(), DockerError> {
    run(&["version", "--format", "{{.Server.Version}}"])
        .map(|_| ())
        .map_err(DockerError::Unavailable)
}

fn published_port(name: &str) -> Result<u16, DockerError> {
    let out = run(&["port", name, "6379/tcp"]).map_err(DockerError::Start)?;
    out.lines()
        .find_map(|l| l.rsplit(':').next()?.trim().parse().ok())
        .ok_or_else(|| DockerError::Start(format!("no published port for {name}: {out:?}")))
}

/// Run `docker <args>` and return stdout, or stderr as the error.
///
/// # Errors
/// The command's stderr, or the spawn failure.
pub fn run(args: &[&str]) -> Result<String, String> {
    let out = Command::new("docker")
        .args(args)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| e.to_string())?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        Err(String::from_utf8_lossy(&out.stderr).trim().to_owned())
    }
}
