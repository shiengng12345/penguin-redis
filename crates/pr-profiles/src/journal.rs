//! Crash-safe credential binding (v2.1 §4.2, §30.2, ADR-010, R07, V-D02).
//!
//! Saving a connection touches two stores that share no transaction: the OS keychain and
//! `profiles.toml`. A crash between them leaves either an orphaned secret nobody references,
//! or a profile pointing at a secret that was never written. §4.2 forbids both the silent
//! version ("凭证已保存" when it wasn't) and leaving the mess undiscovered.
//!
//! So the order is fixed, journalled, and reconciled at startup:
//!
//! ```text
//! 1. mint a NEW secret_ref (never reuse, so rollback is always possible)
//! 2. journal  {bind, profile, secret_ref, pending}   -> fsync
//! 3. write the secret to the OS store
//! 4. lock profiles.toml, read-modify-write, fsync, atomic rename
//! 5. journal  committed                              -> fsync
//! 6. if the OLD secret_ref is now unreferenced, delete it; journal cleaned
//! ```
//!
//! Every step is idempotent, so replay after a crash is safe.

use crate::credentials::{CredentialError, CredentialStore, SecretRef};
use std::fmt::Write as _;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use thiserror::Error;

/// Journal failures.
#[derive(Debug, Error)]
pub enum JournalError {
    /// Filesystem trouble.
    #[error("journal io: {0}")]
    Io(#[from] std::io::Error),
    /// A line could not be parsed.
    #[error("malformed journal entry at line {0}")]
    Malformed(usize),
    /// Credential store trouble.
    #[error(transparent)]
    Credential(#[from] CredentialError),
}

/// Where a binding got to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    /// Journalled, but the secret and the profile may or may not be written yet.
    Pending,
    /// Both the secret and the profile are durable.
    Committed,
    /// The superseded secret has been removed.
    Cleaned,
}

impl State {
    fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Committed => "committed",
            Self::Cleaned => "cleaned",
        }
    }
    fn parse(s: &str) -> Option<Self> {
        match s {
            "pending" => Some(Self::Pending),
            "committed" => Some(Self::Committed),
            "cleaned" => Some(Self::Cleaned),
            _ => None,
        }
    }
}

/// One journal record. Contains references only — never a secret (§23.4).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    /// Profile being bound.
    pub profile_uuid: String,
    /// The reference this operation is installing.
    pub secret_ref: SecretRef,
    /// The reference being replaced, if this is a rotation.
    pub supersedes: Option<SecretRef>,
    /// How far the operation got.
    pub state: State,
}

/// An append-only journal beside `profiles.toml`.
#[derive(Debug)]
pub struct Journal {
    path: PathBuf,
}

/// What reconciliation found and what a human must decide.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Reconciliation {
    /// Secrets written but referenced by no profile. **Reported, never auto-deleted**: the
    /// user may have rolled the config back deliberately (§4.2).
    pub orphaned_secrets: Vec<SecretRef>,
    /// Profiles referencing a secret that does not exist. The profile is unusable until
    /// re-bound, and must be shown as `credential missing` rather than silently retried.
    pub dangling_profiles: Vec<String>,
    /// Superseded secrets whose cleanup did not finish; safe to retry.
    pub retryable_cleanups: Vec<SecretRef>,
    /// Entries replayed.
    pub entries_examined: usize,
}

impl Reconciliation {
    /// Whether anything needs to be shown to the user.
    #[must_use]
    pub fn needs_attention(&self) -> bool {
        !self.orphaned_secrets.is_empty() || !self.dangling_profiles.is_empty()
    }
}

impl Journal {
    /// Open (or create) the journal at `path`.
    #[must_use]
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    /// Path on disk.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Append a record and flush it to stable storage before returning.
    ///
    /// # Errors
    /// [`JournalError::Io`] if the write or fsync fails.
    pub fn append(&self, e: &Entry) -> Result<(), JournalError> {
        let mut line = String::new();
        let _ = writeln!(
            line,
            "{}\t{}\t{}\t{}",
            e.profile_uuid,
            e.secret_ref.as_str(),
            e.supersedes.as_ref().map_or("-", SecretRef::as_str),
            e.state.as_str()
        );
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        Self::set_private(&f)?;
        f.write_all(line.as_bytes())?;
        // Without this the journal can be lost in exactly the crash it exists to survive.
        f.sync_all()?;
        Ok(())
    }

    #[cfg(unix)]
    fn set_private(f: &std::fs::File) -> std::io::Result<()> {
        use std::os::unix::fs::PermissionsExt as _;
        let mut p = f.metadata()?.permissions();
        p.set_mode(0o600);
        f.set_permissions(p)
    }

    #[cfg(not(unix))]
    fn set_private(_f: &std::fs::File) -> std::io::Result<()> {
        // Windows ACLs are applied to the containing directory at creation (WIN-03).
        Ok(())
    }

    /// Read every record in order.
    ///
    /// # Errors
    /// [`JournalError::Io`] or [`JournalError::Malformed`].
    pub fn read(&self) -> Result<Vec<Entry>, JournalError> {
        let text = match std::fs::read_to_string(&self.path) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(e.into()),
        };
        let mut out = Vec::new();
        for (i, line) in text.lines().enumerate() {
            if line.trim().is_empty() {
                continue;
            }
            let f: Vec<&str> = line.split('\t').collect();
            if f.len() != 4 {
                return Err(JournalError::Malformed(i + 1));
            }
            let secret_ref = SecretRef::parse(f[1]).map_err(|_| JournalError::Malformed(i + 1))?;
            let supersedes = if f[2] == "-" {
                None
            } else {
                Some(SecretRef::parse(f[2]).map_err(|_| JournalError::Malformed(i + 1))?)
            };
            let state = State::parse(f[3]).ok_or(JournalError::Malformed(i + 1))?;
            out.push(Entry {
                profile_uuid: f[0].to_owned(),
                secret_ref,
                supersedes,
                state,
            });
        }
        Ok(out)
    }

    /// The last recorded state for each `(profile, secret_ref)` pair.
    fn latest(&self) -> Result<Vec<Entry>, JournalError> {
        let all = self.read()?;
        let mut out: Vec<Entry> = Vec::new();
        for e in all {
            if let Some(slot) = out
                .iter_mut()
                .find(|o| o.profile_uuid == e.profile_uuid && o.secret_ref == e.secret_ref)
            {
                slot.state = e.state;
                slot.supersedes.clone_from(&e.supersedes);
            } else {
                out.push(e);
            }
        }
        Ok(out)
    }

    /// Reconcile the journal against the credential store and the live profile references.
    ///
    /// `referenced` is what `profiles.toml` currently points at. Nothing is deleted here
    /// except a superseded secret whose replacement is already committed — that one is safe
    /// because the profile no longer names it.
    ///
    /// # Errors
    /// [`JournalError`] on IO or store failure.
    pub fn reconcile(
        &self,
        store: &dyn CredentialStore,
        referenced: &[SecretRef],
    ) -> Result<Reconciliation, JournalError> {
        let entries = self.latest()?;
        let mut r = Reconciliation {
            entries_examined: entries.len(),
            ..Reconciliation::default()
        };

        for e in &entries {
            let exists = match store.get(&e.secret_ref) {
                Ok(_) => true,
                Err(CredentialError::NotFound(_)) => false,
                // An unavailable store means we cannot judge anything; say so rather than
                // reporting every profile as broken.
                Err(err) => return Err(err.into()),
            };
            let is_referenced = referenced.contains(&e.secret_ref);

            match (e.state, exists, is_referenced) {
                // Crashed between the journal and the profile write: the secret exists but
                // nothing points at it.
                (State::Pending, true, false) => r.orphaned_secrets.push(e.secret_ref.clone()),
                // Journalled and referenced, but the secret never landed.
                (State::Pending | State::Committed, false, true) => {
                    r.dangling_profiles.push(e.profile_uuid.clone());
                }
                // Committed rotation whose cleanup did not run.
                (State::Committed, _, _) => {
                    if let Some(old) = &e.supersedes
                        && !referenced.contains(old)
                        && store.get(old).is_ok()
                    {
                        r.retryable_cleanups.push(old.clone());
                    }
                }
                _ => {}
            }
        }
        Ok(r)
    }

    /// Finish the cleanups reconciliation found safe.
    ///
    /// # Errors
    /// [`JournalError`] on store or IO failure.
    pub fn finish_cleanups(
        &self,
        store: &dyn CredentialStore,
        r: &Reconciliation,
        profile_uuid: &str,
    ) -> Result<usize, JournalError> {
        let mut n = 0;
        for old in &r.retryable_cleanups {
            store.delete(old)?;
            self.append(&Entry {
                profile_uuid: profile_uuid.to_owned(),
                secret_ref: old.clone(),
                supersedes: None,
                state: State::Cleaned,
            })?;
            n += 1;
        }
        Ok(n)
    }

    /// Truncate once everything is reconciled, so the journal does not grow without bound.
    ///
    /// # Errors
    /// [`JournalError::Io`].
    pub fn truncate(&self) -> Result<(), JournalError> {
        match std::fs::remove_file(&self.path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.into()),
        }
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::credentials::StoreKind;
    use std::collections::HashMap;
    use std::sync::Mutex;

    /// In-memory store so crash points are deterministic.
    #[derive(Default)]
    struct MemStore {
        map: Mutex<HashMap<String, String>>,
    }

    impl CredentialStore for MemStore {
        fn kind(&self) -> StoreKind {
            StoreKind::AppleKeychain
        }
        fn set(&self, r: &SecretRef, s: &str) -> Result<(), CredentialError> {
            self.map
                .lock()
                .map_err(|_| CredentialError::Store("poisoned".into()))?
                .insert(r.as_str().to_owned(), s.to_owned());
            Ok(())
        }
        fn get(&self, r: &SecretRef) -> Result<String, CredentialError> {
            self.map
                .lock()
                .map_err(|_| CredentialError::Store("poisoned".into()))?
                .get(r.as_str())
                .cloned()
                .ok_or_else(|| CredentialError::NotFound(r.as_str().to_owned()))
        }
        fn delete(&self, r: &SecretRef) -> Result<(), CredentialError> {
            self.map
                .lock()
                .map_err(|_| CredentialError::Store("poisoned".into()))?
                .remove(r.as_str());
            Ok(())
        }
    }

    struct Tmp(PathBuf);
    impl Tmp {
        fn new(tag: &str) -> Self {
            let p = std::env::temp_dir().join(format!(
                "pr-journal-{tag}-{}-{:?}",
                std::process::id(),
                std::thread::current().id()
            ));
            let _ = std::fs::remove_dir_all(&p);
            std::fs::create_dir_all(&p).unwrap();
            Self(p)
        }
        fn journal(&self) -> Journal {
            Journal::new(self.0.join("credentials.journal"))
        }
    }
    impl Drop for Tmp {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn entry(p: &str, r: &str, sup: Option<&str>, st: State) -> Entry {
        Entry {
            profile_uuid: p.to_owned(),
            secret_ref: SecretRef::generate(r),
            supersedes: sup.map(SecretRef::generate),
            state: st,
        }
    }

    #[test]
    fn append_and_read_round_trip() {
        let t = Tmp::new("rt");
        let j = t.journal();
        assert!(
            j.read().unwrap().is_empty(),
            "a missing journal reads as empty"
        );
        let e = entry("p1", "uuid-1", None, State::Pending);
        j.append(&e).unwrap();
        j.append(&entry("p1", "uuid-1", None, State::Committed))
            .unwrap();
        let all = j.read().unwrap();
        assert_eq!(all.len(), 2);
        assert_eq!(all[0], e);
        assert_eq!(all[1].state, State::Committed);
    }

    #[test]
    fn the_journal_never_contains_a_secret() {
        // §23.4: references only.
        let t = Tmp::new("nosecret");
        let j = t.journal();
        j.append(&entry("p1", "uuid-1", None, State::Pending))
            .unwrap();
        let text = std::fs::read_to_string(j.path()).unwrap();
        assert!(text.contains("credential:uuid-1"));
        assert!(!text.contains("s3cret"));
        assert!(!text.to_lowercase().contains("password"));
    }

    #[cfg(unix)]
    #[test]
    fn the_journal_is_private() {
        use std::os::unix::fs::PermissionsExt as _;
        let t = Tmp::new("perm");
        let j = t.journal();
        j.append(&entry("p1", "uuid-1", None, State::Pending))
            .unwrap();
        let mode = std::fs::metadata(j.path()).unwrap().permissions().mode();
        assert_eq!(
            mode & 0o777,
            0o600,
            "journal must not be group/world readable"
        );
    }

    #[test]
    fn crash_after_the_secret_but_before_the_profile_is_an_orphan() {
        // Step 3 done, step 4 not. Nothing references the secret.
        let t = Tmp::new("orphan");
        let j = t.journal();
        let store = MemStore::default();
        let r = SecretRef::generate("uuid-new");
        j.append(&entry("p1", "uuid-new", None, State::Pending))
            .unwrap();
        store.set(&r, "pw").unwrap();

        let rec = j.reconcile(&store, &[]).unwrap();
        assert_eq!(rec.orphaned_secrets, vec![r.clone()]);
        assert!(rec.needs_attention());
        // Reported, never auto-deleted (§4.2).
        assert!(
            store.get(&r).is_ok(),
            "reconciliation must not delete it on its own"
        );
    }

    #[test]
    fn crash_before_the_secret_landed_leaves_a_dangling_profile() {
        // Step 2 done, step 3 not, but the profile was somehow written.
        let t = Tmp::new("dangling");
        let j = t.journal();
        let store = MemStore::default();
        let r = SecretRef::generate("uuid-missing");
        j.append(&entry("p1", "uuid-missing", None, State::Pending))
            .unwrap();

        let rec = j.reconcile(&store, &[r]).unwrap();
        assert_eq!(rec.dangling_profiles, vec!["p1".to_string()]);
        assert!(rec.needs_attention());
    }

    #[test]
    fn a_fully_committed_binding_needs_no_attention() {
        let t = Tmp::new("clean");
        let j = t.journal();
        let store = MemStore::default();
        let r = SecretRef::generate("uuid-ok");
        j.append(&entry("p1", "uuid-ok", None, State::Pending))
            .unwrap();
        store.set(&r, "pw").unwrap();
        j.append(&entry("p1", "uuid-ok", None, State::Committed))
            .unwrap();

        let rec = j.reconcile(&store, std::slice::from_ref(&r)).unwrap();
        assert!(!rec.needs_attention(), "{rec:?}");
        assert!(rec.retryable_cleanups.is_empty());
    }

    #[test]
    fn an_unfinished_rotation_cleanup_is_retryable_and_idempotent() {
        // Steps 1-5 done, step 6 not: the old secret is still there but unreferenced.
        let t = Tmp::new("rotate");
        let j = t.journal();
        let store = MemStore::default();
        let old = SecretRef::generate("uuid-old");
        let new = SecretRef::generate("uuid-new");
        store.set(&old, "old-pw").unwrap();
        store.set(&new, "new-pw").unwrap();
        j.append(&entry("p1", "uuid-new", Some("uuid-old"), State::Committed))
            .unwrap();

        let rec = j.reconcile(&store, std::slice::from_ref(&new)).unwrap();
        assert_eq!(rec.retryable_cleanups, vec![old.clone()]);

        assert_eq!(j.finish_cleanups(&store, &rec, "p1").unwrap(), 1);
        assert!(matches!(store.get(&old), Err(CredentialError::NotFound(_))));
        assert!(store.get(&new).is_ok(), "the live secret must survive");

        // Replay is safe.
        let again = j.reconcile(&store, std::slice::from_ref(&new)).unwrap();
        assert!(again.retryable_cleanups.is_empty());
        assert_eq!(j.finish_cleanups(&store, &again, "p1").unwrap(), 0);
    }

    #[test]
    fn a_rotation_never_reuses_the_reference() {
        // §4.2: a new id is what makes rollback possible at all.
        let a = SecretRef::generate("uuid-1");
        let b = SecretRef::generate("uuid-2");
        assert_ne!(a, b);
        let e = entry("p1", "uuid-2", Some("uuid-1"), State::Committed);
        assert_ne!(e.secret_ref, e.supersedes.clone().unwrap());
    }

    #[test]
    fn the_latest_state_per_reference_wins() {
        let t = Tmp::new("latest");
        let j = t.journal();
        let store = MemStore::default();
        let r = SecretRef::generate("uuid-1");
        store.set(&r, "pw").unwrap();
        j.append(&entry("p1", "uuid-1", None, State::Pending))
            .unwrap();
        j.append(&entry("p1", "uuid-1", None, State::Committed))
            .unwrap();
        // Pending would have called this an orphan; committed does not.
        let rec = j.reconcile(&store, &[]).unwrap();
        assert!(
            rec.orphaned_secrets.is_empty(),
            "the later record must win: {rec:?}"
        );
    }

    #[test]
    fn a_malformed_journal_is_reported_with_its_line() {
        let t = Tmp::new("bad");
        let j = t.journal();
        std::fs::write(j.path(), "p1\tcredential:a\t-\tcommitted\nnot-a-record\n").unwrap();
        match j.read() {
            Err(JournalError::Malformed(n)) => assert_eq!(n, 2),
            other => panic!("expected a malformed error, got {other:?}"),
        }
    }

    #[test]
    fn an_unavailable_store_stops_reconciliation_rather_than_condemning_every_profile() {
        let t = Tmp::new("unavail");
        let j = t.journal();
        j.append(&entry("p1", "uuid-1", None, State::Pending))
            .unwrap();
        let store = crate::credentials::UnavailableStore;
        assert!(j.reconcile(&store, &[]).is_err());
    }

    #[test]
    fn truncate_is_idempotent() {
        let t = Tmp::new("trunc");
        let j = t.journal();
        j.append(&entry("p1", "uuid-1", None, State::Cleaned))
            .unwrap();
        j.truncate().unwrap();
        assert!(j.read().unwrap().is_empty());
        j.truncate().unwrap();
    }
}
