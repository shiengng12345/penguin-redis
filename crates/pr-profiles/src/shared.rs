//! Shared-file concurrency: advisory lock, version check, atomic replace (v2.1 §30.2, §12.10,
//! R20, R39, WIN-03, LIFE-03, V-G05).
//!
//! §12.10 spells out the protocol, and every clause of it is there because of a way the
//! obvious implementation loses data:
//!
//! > 写入前取 advisory lock（POSIX `flock` / Windows `LockFileEx`），读取版本号 → 修改 →
//! > 临时文件 → fsync → 原子 rename → 版本号 +1；**版本号不匹配时不覆盖**，写入 `conflicts/`
//! > 目录并提示用户合并。
//!
//! - **The lock** stops two processes interleaving a read-modify-write.
//! - **The version check** catches the case the lock cannot: a process that read the file,
//!   lost the lock (or never held it), and came back to write a value computed from stale
//!   input. LIFE-03 — 「多终端同时改 profile | 无丢更新」 — is exactly this.
//! - **The temp file, the fsync and the rename** mean a crash leaves either the old file or
//!   the new one, never half of either. Without the fsync, the rename can land before the
//!   contents do, and the file that survives a power cut is a valid-looking empty one.
//! - **`conflicts/`** exists because refusing to write and then discarding the user's edit is
//!   still losing it.
//!
//! The lock comes from `std::fs::File::lock`, stabilised in Rust 1.89: `flock` on Unix,
//! `LockFileEx` on Windows, no dependency and no `unsafe` of ours. Advisory, as §12.10 says —
//! it coordinates processes that agree to use it, and does not stop `cat > file`.

use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

/// Why a shared write did not happen.
#[derive(Debug, thiserror::Error)]
pub enum SharedFileError {
    /// The file system said no.
    #[error("{0}")]
    Io(#[from] std::io::Error),
    /// Another process holds the lock and `try` was asked for.
    #[error("another process is writing {0}")]
    Locked(PathBuf),
    /// The file changed underneath us. The edit is preserved; see `conflict`.
    #[error(
        "{path} moved from version {expected} to {found} while this edit was being made. \
         The edit was not discarded: it is in {conflict}"
    )]
    Conflict {
        /// The file that was not overwritten.
        path: PathBuf,
        /// The version this edit was based on.
        expected: u64,
        /// The version actually on disk.
        found: u64,
        /// Where the rejected edit was written instead.
        conflict: PathBuf,
    },
    /// The file exists but carries no version line.
    #[error("{0} has no version header, so a safe update cannot be attempted")]
    NoVersion(PathBuf),
}

/// A versioned file that several processes may edit.
///
/// The version lives in the file, as its first line, rather than in a sidecar: a sidecar can
/// be lost, copied without its file, or left behind by a crash, and then the version says
/// something about a file that no longer exists.
#[derive(Debug, Clone)]
pub struct SharedFile {
    path: PathBuf,
}

/// The first line of a versioned file.
const VERSION_PREFIX: &str = "# penguin-version: ";

/// What one read found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Versioned {
    /// Version as recorded in the file.
    pub version: u64,
    /// Everything after the version line.
    pub body: String,
}

impl SharedFile {
    /// Address a versioned file. Nothing is opened yet.
    #[must_use]
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// The path.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Where a rejected edit goes.
    #[must_use]
    pub fn conflicts_dir(&self) -> PathBuf {
        self.path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .join("conflicts")
    }

    /// Read the current version and body.
    ///
    /// A missing file reads as version 0 with an empty body, so a first write is not a special
    /// case the caller has to remember.
    ///
    /// # Errors
    /// [`SharedFileError::Io`], or [`SharedFileError::NoVersion`] if the file exists without a
    /// version header — which means something else wrote it, and guessing would be how the
    /// version protocol stops protecting anything.
    pub fn read(&self) -> Result<Versioned, SharedFileError> {
        let text = match std::fs::read_to_string(&self.path) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Ok(Versioned {
                    version: 0,
                    body: String::new(),
                });
            }
            Err(e) => return Err(e.into()),
        };
        parse(&text).ok_or_else(|| SharedFileError::NoVersion(self.path.clone()))
    }

    /// Read, let `edit` change the body, and write it back — under a lock, with a version
    /// check, through a temp file, atomically.
    ///
    /// `edit` may be called only once; if the version moved, the new body is written to
    /// `conflicts/` and [`SharedFileError::Conflict`] is returned rather than the edit being
    /// dropped.
    ///
    /// # Errors
    /// [`SharedFileError`]. A conflict names where the rejected edit went.
    pub fn update<F>(&self, edit: F) -> Result<u64, SharedFileError>
    where
        F: FnOnce(&str) -> String,
    {
        // The lock is on a separate file, not on the data file. The data file is replaced by
        // rename, so a lock held on it would be a lock on an inode that is about to stop being
        // the file everyone else is looking at.
        let lock_path = self.lock_path();
        if let Some(parent) = lock_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let lock = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(false)
            .open(&lock_path)?;
        crate::perms::restrict_to_current_user(&lock_path)?;
        lock.lock()?;

        let before = self.read()?;
        let body = edit(&before.body);
        let after = self.read()?;
        if after.version != before.version {
            // Cannot happen while we hold the lock and every writer takes it — but a process
            // that did not take the lock is exactly what this check is for, and "cannot
            // happen" is not a reason to lose somebody's edit.
            let conflict = self.write_conflict(&body, before.version, after.version)?;
            return Err(SharedFileError::Conflict {
                path: self.path.clone(),
                expected: before.version,
                found: after.version,
                conflict,
            });
        }

        let next = before.version + 1;
        self.write_atomically(next, &body)?;
        // `lock` drops here, releasing it. Explicit rather than implicit because the ordering
        // matters: the rename must complete before another writer sees the lock free.
        drop(lock);
        Ok(next)
    }

    /// Write a body against an expected version, without holding the lock across an edit.
    ///
    /// For a caller that read earlier, thought about it, and is now writing — the case the
    /// version check exists for.
    ///
    /// # Errors
    /// [`SharedFileError::Conflict`] if the file moved on, with the edit preserved.
    pub fn write_if_unchanged(&self, expected: u64, body: &str) -> Result<u64, SharedFileError> {
        let lock_path = self.lock_path();
        if let Some(parent) = lock_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let lock = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(false)
            .open(&lock_path)?;
        lock.lock()?;

        let current = self.read()?;
        if current.version != expected {
            let conflict = self.write_conflict(body, expected, current.version)?;
            return Err(SharedFileError::Conflict {
                path: self.path.clone(),
                expected,
                found: current.version,
                conflict,
            });
        }
        let next = expected + 1;
        self.write_atomically(next, body)?;
        drop(lock);
        Ok(next)
    }

    /// The lock file beside the data file.
    #[must_use]
    pub fn lock_path(&self) -> PathBuf {
        let mut p = self.path.clone();
        let name = p.file_name().map_or_else(
            || "penguin.lock".to_owned(),
            |n| format!("{}.lock", n.to_string_lossy()),
        );
        p.set_file_name(name);
        p
    }

    /// temp file → fsync → atomic rename. Never writes the destination in place.
    fn write_atomically(&self, version: u64, body: &str) -> Result<(), SharedFileError> {
        let parent = self.path.parent().unwrap_or_else(|| Path::new("."));
        std::fs::create_dir_all(parent)?;
        // In the same directory, because a rename across file systems is not atomic and
        // silently becomes a copy.
        let tmp = parent.join(format!(
            ".{}.{}.tmp",
            self.path
                .file_name()
                .map_or_else(|| "penguin".into(), |n| n.to_string_lossy()),
            std::process::id()
        ));
        {
            let mut f = File::create(&tmp)?;
            write!(f, "{VERSION_PREFIX}{version}\n{body}")?;
            // Before the rename, not after: without this the rename can land before the bytes
            // do, and a power cut leaves a file that looks valid and is empty.
            f.sync_all()?;
        }
        crate::perms::restrict_to_current_user(&tmp)?;
        // `rename` is `MoveFileEx(MOVEFILE_REPLACE_EXISTING)` on Windows and `rename(2)` on
        // Unix: both replace atomically.
        std::fs::rename(&tmp, &self.path)?;
        // Then the directory, so the rename itself is durable.
        if let Ok(dir) = File::open(parent) {
            let _ = dir.sync_all();
        }
        Ok(())
    }

    /// Preserve a rejected edit. Refusing to write and then dropping the edit is still losing
    /// it, so §12.10 asks for this directory by name.
    fn write_conflict(
        &self,
        body: &str,
        expected: u64,
        found: u64,
    ) -> Result<PathBuf, SharedFileError> {
        let dir = self.conflicts_dir();
        std::fs::create_dir_all(&dir)?;
        let name = format!(
            "{}.v{expected}-vs-v{found}.{}.conflict",
            self.path
                .file_name()
                .map_or_else(|| "penguin".into(), |n| n.to_string_lossy()),
            std::process::id()
        );
        let out = dir.join(name);
        let mut f = File::create(&out)?;
        write!(f, "{VERSION_PREFIX}{expected}\n{body}")?;
        f.sync_all()?;
        crate::perms::restrict_to_current_user(&out)?;
        Ok(out)
    }
}

fn parse(text: &str) -> Option<Versioned> {
    let (first, rest) = text.split_once('\n').unwrap_or((text, ""));
    let v = first.strip_prefix(VERSION_PREFIX)?.trim().parse().ok()?;
    Some(Versioned {
        version: v,
        body: rest.to_owned(),
    })
}

/// Read a file's whole contents with the lock held, for a reader that must not see a torn
/// state on a platform where rename is not enough.
///
/// # Errors
/// [`SharedFileError::Io`].
pub fn read_locked(path: &Path) -> Result<String, SharedFileError> {
    let f = File::open(path)?;
    f.lock_shared()?;
    let mut s = String::new();
    let mut r = &f;
    r.read_to_string(&mut s)?;
    drop(f);
    Ok(s)
}
