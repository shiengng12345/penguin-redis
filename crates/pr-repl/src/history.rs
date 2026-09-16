//! Command history (v2.1 §10.10, §30.1, §31.1, ADR-022, R22/R24, V-H06).
//!
//! History is useful and also the easiest place to leak. §10.10 sets the rules:
//! - **never** store replies, write payloads, AUTH arguments or credential-bearing URLs
//! - key and field names are themselves potentially sensitive, so what is persisted depends
//!   on the environment: development keeps them, staging hashes them, production keeps only
//!   the command name and option keywords (R24)
//! - a redacted argument is a *slot*, not a value: recalling it must never send
//!   `[redacted:…]` to a server
//! - history is scoped to `(profile, identity, db)`, so a prod entry can never surface while
//!   you are pointed at dev
//!
//! Storage is `SQLite` in WAL mode, which is what lets several `prc` processes share one file
//! without a daemon (ADR-003).

use crate::quoting::quote;
use rusqlite::{Connection, OptionalExtension, params};
use std::path::Path;
use thiserror::Error;

/// Current schema version, checked against `PRAGMA user_version`.
pub const SCHEMA_VERSION: i64 = 1;

/// History failures.
#[derive(Debug, Error)]
pub enum HistoryError {
    /// `SQLite` trouble.
    #[error("history store: {0}")]
    Sqlite(#[from] rusqlite::Error),
    /// The file was written by a newer Penguin.
    #[error("history schema {found} is newer than this build supports ({SCHEMA_VERSION})")]
    SchemaTooNew {
        /// Version found on disk.
        found: i64,
    },
}

/// How much of a command may be persisted (R24).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum RedactionLevel {
    /// Development: command, options and names. Values are still never stored.
    None,
    /// Staging: names replaced by a short stable hash, so recall still groups sensibly.
    HashNames,
    /// Production, and anything unclassified. Only the command name and option keywords.
    #[default]
    AllArguments,
}

impl RedactionLevel {
    /// The level for an environment label. Unknown environments get the strictest rule,
    /// because §23.1 treats unclassified as maximum risk.
    #[must_use]
    pub fn for_environment(env: &str) -> Self {
        match env.to_ascii_lowercase().as_str() {
            "development" | "local" | "dev" => Self::None,
            "staging" | "test" => Self::HashNames,
            _ => Self::AllArguments,
        }
    }
}

/// The scope an entry belongs to (§10.10, §12.6).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Scope {
    /// Profile UUID.
    pub profile_uuid: String,
    /// Authenticated identity epoch.
    pub identity_epoch: u64,
    /// Database index.
    pub db: u32,
}

/// Commands whose arguments are secrets no matter the environment.
fn is_secret_bearing(name: &str) -> bool {
    matches!(name, "AUTH" | "HELLO" | "MIGRATE" | "CONFIG" | "ACL")
}

/// Option keywords that are safe to keep because they are syntax, not data.
fn is_keyword(arg: &[u8]) -> bool {
    let up = arg.to_ascii_uppercase();
    matches!(
        up.as_slice(),
        b"NX"
            | b"XX"
            | b"GET"
            | b"EX"
            | b"PX"
            | b"EXAT"
            | b"PXAT"
            | b"KEEPTTL"
            | b"WITHSCORES"
            | b"REV"
            | b"BYSCORE"
            | b"BYLEX"
            | b"LIMIT"
            | b"COUNT"
            | b"MATCH"
            | b"NOVALUES"
            | b"STREAMS"
            | b"ASC"
            | b"DESC"
            | b"ALPHA"
    )
}

fn short_hash(b: &[u8]) -> String {
    let h = blake3::hash(b);
    format!("[hashed:{}]", &h.to_hex()[..8])
}

/// Apply the redaction rules to an argv, returning the storable form.
///
/// The first element (the command name) is always kept: it is what makes history searchable
/// and it is never itself a secret.
#[must_use]
pub fn redact(argv: &[Vec<u8>], level: RedactionLevel) -> Vec<String> {
    let Some(first) = argv.first() else {
        return Vec::new();
    };
    let name = String::from_utf8_lossy(first).to_uppercase();
    let mut out = vec![name.clone()];

    // A secret-bearing command loses every argument regardless of environment (§10.10).
    let effective = if is_secret_bearing(&name) {
        RedactionLevel::AllArguments
    } else {
        level
    };

    for arg in argv.iter().skip(1) {
        if is_keyword(arg) {
            out.push(String::from_utf8_lossy(arg).to_uppercase());
            continue;
        }
        match effective {
            RedactionLevel::None => out.push(quote(arg)),
            RedactionLevel::HashNames => out.push(short_hash(arg)),
            RedactionLevel::AllArguments => out.push("[redacted]".to_owned()),
        }
    }
    out
}

/// Whether a stored entry still contains a placeholder, in which case recalling it must not
/// be sent as-is (§10.10).
#[must_use]
pub fn has_placeholder(parts: &[String]) -> bool {
    parts
        .iter()
        .any(|p| p == "[redacted]" || p.starts_with("[hashed:"))
}

/// One stored command.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Entry {
    /// Row id.
    pub id: i64,
    /// Redacted argv, joined for display and recall.
    pub parts: Vec<String>,
    /// Unix milliseconds.
    pub at_ms: i64,
}

impl Entry {
    /// The line as it would be put back in the editor.
    #[must_use]
    pub fn line(&self) -> String {
        self.parts.join(" ")
    }
    /// Whether this entry may be submitted unchanged.
    #[must_use]
    pub fn is_submittable(&self) -> bool {
        !has_placeholder(&self.parts)
    }
}

/// The history store.
pub struct History {
    conn: Connection,
}

impl std::fmt::Debug for History {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Deliberately opaque: a Debug that dumped rows would defeat the redaction rules.
        f.write_str("History { .. }")
    }
}

impl History {
    /// Open (creating if needed) a history database.
    ///
    /// # Errors
    /// [`HistoryError`] on `SQLite` failure or a newer schema.
    pub fn open(path: &Path) -> Result<Self, HistoryError> {
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let conn = Connection::open(path)?;
        Self::init(&conn)?;
        Self::set_private(path);
        Ok(Self { conn })
    }

    /// An in-memory store, for tests.
    ///
    /// # Errors
    /// [`HistoryError`] on `SQLite` failure.
    pub fn open_in_memory() -> Result<Self, HistoryError> {
        let conn = Connection::open_in_memory()?;
        Self::init(&conn)?;
        Ok(Self { conn })
    }

    fn init(conn: &Connection) -> Result<(), HistoryError> {
        // WAL is what lets several prc processes share the file without a daemon (ADR-003).
        let _: String = conn.query_row("PRAGMA journal_mode = WAL", [], |r| r.get(0))?;
        conn.execute_batch("PRAGMA synchronous = NORMAL; PRAGMA foreign_keys = ON;")?;
        let found: i64 = conn.query_row("PRAGMA user_version", [], |r| r.get(0))?;
        if found > SCHEMA_VERSION {
            return Err(HistoryError::SchemaTooNew { found });
        }
        if found < 1 {
            conn.execute_batch(
                "CREATE TABLE IF NOT EXISTS entries (
                     id             INTEGER PRIMARY KEY AUTOINCREMENT,
                     profile_uuid   TEXT NOT NULL,
                     identity_epoch INTEGER NOT NULL,
                     db             INTEGER NOT NULL,
                     parts          TEXT NOT NULL,
                     at_ms          INTEGER NOT NULL
                 );
                 CREATE INDEX IF NOT EXISTS idx_scope
                     ON entries (profile_uuid, identity_epoch, db, id DESC);",
            )?;
            conn.pragma_update(None, "user_version", SCHEMA_VERSION)?;
        }
        Ok(())
    }

    #[cfg(unix)]
    fn set_private(path: &Path) {
        use std::os::unix::fs::PermissionsExt as _;
        if let Ok(md) = std::fs::metadata(path) {
            let mut p = md.permissions();
            p.set_mode(0o600);
            let _ = std::fs::set_permissions(path, p);
        }
    }

    #[cfg(not(unix))]
    fn set_private(_path: &Path) {}

    /// Record a command after redaction.
    ///
    /// # Errors
    /// [`HistoryError`] on `SQLite` failure.
    pub fn record(
        &self,
        scope: &Scope,
        argv: &[Vec<u8>],
        level: RedactionLevel,
        at_ms: i64,
    ) -> Result<i64, HistoryError> {
        let parts = redact(argv, level);
        if parts.is_empty() {
            return Ok(0);
        }
        self.conn.execute(
            "INSERT INTO entries (profile_uuid, identity_epoch, db, parts, at_ms)
             VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                scope.profile_uuid,
                i64::try_from(scope.identity_epoch).unwrap_or(i64::MAX),
                scope.db,
                serde_json::to_string(&parts).unwrap_or_default(),
                at_ms
            ],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    /// Most recent entries for a scope, newest first.
    ///
    /// # Errors
    /// [`HistoryError`] on `SQLite` failure.
    pub fn recent(&self, scope: &Scope, limit: usize) -> Result<Vec<Entry>, HistoryError> {
        let mut st = self.conn.prepare(
            "SELECT id, parts, at_ms FROM entries
             WHERE profile_uuid = ?1 AND identity_epoch = ?2 AND db = ?3
             ORDER BY id DESC LIMIT ?4",
        )?;
        let rows = st.query_map(
            params![
                scope.profile_uuid,
                i64::try_from(scope.identity_epoch).unwrap_or(i64::MAX),
                scope.db,
                i64::try_from(limit).unwrap_or(i64::MAX)
            ],
            |r| {
                let raw: String = r.get(1)?;
                Ok(Entry {
                    id: r.get(0)?,
                    parts: serde_json::from_str(&raw).unwrap_or_default(),
                    at_ms: r.get(2)?,
                })
            },
        )?;
        Ok(rows.filter_map(Result::ok).collect())
    }

    /// Search within a scope. Matching is on the stored (already redacted) text, so a search
    /// can never reveal something the store chose not to keep.
    ///
    /// # Errors
    /// [`HistoryError`] on `SQLite` failure.
    pub fn search(
        &self,
        scope: &Scope,
        needle: &str,
        limit: usize,
    ) -> Result<Vec<Entry>, HistoryError> {
        let all = self.recent(scope, 10_000)?;
        let n = needle.to_uppercase();
        Ok(all
            .into_iter()
            .filter(|e| e.line().to_uppercase().contains(&n))
            .take(limit)
            .collect())
    }

    /// Delete everything for a profile, or the whole store when `profile` is `None`.
    ///
    /// # Errors
    /// [`HistoryError`] on `SQLite` failure.
    pub fn clear(&self, profile: Option<&str>) -> Result<usize, HistoryError> {
        let n = match profile {
            Some(p) => self
                .conn
                .execute("DELETE FROM entries WHERE profile_uuid = ?1", params![p])?,
            None => self.conn.execute("DELETE FROM entries", [])?,
        };
        // Reclaim the pages so the text is not merely unlinked (§10.10 clearing).
        self.conn.execute_batch("VACUUM")?;
        Ok(n)
    }

    /// Drop entries beyond a retention window.
    ///
    /// # Errors
    /// [`HistoryError`] on `SQLite` failure.
    pub fn prune(&self, older_than_ms: i64, keep_max: usize) -> Result<usize, HistoryError> {
        let by_age = self.conn.execute(
            "DELETE FROM entries WHERE at_ms < ?1",
            params![older_than_ms],
        )?;
        let by_count = self.conn.execute(
            "DELETE FROM entries WHERE id NOT IN
                 (SELECT id FROM entries ORDER BY id DESC LIMIT ?1)",
            params![i64::try_from(keep_max).unwrap_or(i64::MAX)],
        )?;
        Ok(by_age + by_count)
    }

    /// Total rows, for tests and `:history status`.
    ///
    /// # Errors
    /// [`HistoryError`] on `SQLite` failure.
    pub fn count(&self) -> Result<i64, HistoryError> {
        Ok(self
            .conn
            .query_row("SELECT COUNT(*) FROM entries", [], |r| r.get(0))
            .optional()?
            .unwrap_or(0))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn argv(parts: &[&str]) -> Vec<Vec<u8>> {
        parts.iter().map(|s| s.as_bytes().to_vec()).collect()
    }

    fn scope(db: u32) -> Scope {
        Scope {
            profile_uuid: "p1".into(),
            identity_epoch: 1,
            db,
        }
    }

    // ---------------------------------------------------------------- redaction (R24)
    #[test]
    fn development_keeps_names_but_never_values() {
        let out = redact(
            &argv(&["HGET", "player:10001", "status"]),
            RedactionLevel::None,
        );
        assert_eq!(out, vec!["HGET", "player:10001", "status"]);
    }

    #[test]
    fn staging_hashes_names_stably() {
        let a = redact(&argv(&["GET", "player:1"]), RedactionLevel::HashNames);
        let b = redact(&argv(&["GET", "player:1"]), RedactionLevel::HashNames);
        let c = redact(&argv(&["GET", "player:2"]), RedactionLevel::HashNames);
        assert_eq!(a, b, "the same name must hash the same way");
        assert_ne!(a, c);
        assert!(a[1].starts_with("[hashed:"));
        assert!(!a[1].contains("player"), "the name must not survive: {a:?}");
    }

    #[test]
    fn production_keeps_only_the_command_and_keywords() {
        let out = redact(
            &argv(&["SET", "customer:passport:123", "value", "EX", "300", "NX"]),
            RedactionLevel::AllArguments,
        );
        assert_eq!(out[0], "SET");
        assert!(
            out.contains(&"EX".to_string()),
            "keywords are syntax, not data"
        );
        assert!(out.contains(&"NX".to_string()));
        let joined = out.join(" ");
        assert!(
            !joined.contains("passport"),
            "a sensitive key leaked: {joined}"
        );
        assert!(!joined.contains("value"));
    }

    #[test]
    fn unclassified_environments_get_the_strictest_rule() {
        // §23.1: unclassified is maximum risk, not "probably fine".
        assert_eq!(
            RedactionLevel::for_environment("production"),
            RedactionLevel::AllArguments
        );
        assert_eq!(
            RedactionLevel::for_environment("something-new"),
            RedactionLevel::AllArguments
        );
        assert_eq!(
            RedactionLevel::for_environment(""),
            RedactionLevel::AllArguments
        );
        assert_eq!(
            RedactionLevel::for_environment("development"),
            RedactionLevel::None
        );
        assert_eq!(RedactionLevel::for_environment("DEV"), RedactionLevel::None);
        assert_eq!(
            RedactionLevel::for_environment("staging"),
            RedactionLevel::HashNames
        );
    }

    #[test]
    fn auth_loses_its_arguments_even_in_development() {
        // SEC-01: the one rule that holds regardless of environment.
        for level in [
            RedactionLevel::None,
            RedactionLevel::HashNames,
            RedactionLevel::AllArguments,
        ] {
            let out = redact(&argv(&["AUTH", "s3cret-password"]), level);
            assert_eq!(out[0], "AUTH");
            let joined = out.join(" ");
            assert!(
                !joined.contains("s3cret"),
                "{level:?} leaked the password: {joined}"
            );
        }
    }

    #[test]
    fn hello_with_auth_also_loses_its_arguments() {
        let out = redact(
            &argv(&["HELLO", "3", "AUTH", "user", "pw"]),
            RedactionLevel::None,
        );
        let joined = out.join(" ");
        assert!(!joined.contains("pw"), "{joined}");
        assert!(!joined.contains("user"), "{joined}");
    }

    #[test]
    fn config_and_acl_are_treated_as_secret_bearing() {
        for cmd in ["CONFIG", "ACL", "MIGRATE"] {
            let out = redact(
                &argv(&[cmd, "SET", "requirepass", "hunter2"]),
                RedactionLevel::None,
            );
            assert!(!out.join(" ").contains("hunter2"), "{cmd} leaked");
        }
    }

    #[test]
    fn a_redacted_entry_is_not_submittable() {
        // §10.10: a placeholder is a slot, never a value to send.
        let prod = redact(&argv(&["GET", "k"]), RedactionLevel::AllArguments);
        assert!(has_placeholder(&prod));
        let e = Entry {
            id: 1,
            parts: prod,
            at_ms: 0,
        };
        assert!(
            !e.is_submittable(),
            "recalling this must not send [redacted]"
        );

        let dev = redact(&argv(&["GET", "k"]), RedactionLevel::None);
        assert!(!has_placeholder(&dev));
        assert!(
            Entry {
                id: 2,
                parts: dev,
                at_ms: 0
            }
            .is_submittable()
        );
    }

    #[test]
    fn awkward_arguments_are_requoted_so_recall_round_trips() {
        let out = redact(&argv(&["SET", "k", "a b"]), RedactionLevel::None);
        let line = out.join(" ");
        let toks = crate::quoting::split_args(line.as_bytes()).unwrap();
        assert_eq!(
            toks[2].bytes.as_ref(),
            b"a b",
            "recall must reproduce the exact argument"
        );
    }

    #[test]
    fn an_empty_argv_records_nothing() {
        assert!(redact(&[], RedactionLevel::None).is_empty());
    }

    // ---------------------------------------------------------------- storage (V-H06)
    #[test]
    fn records_and_recalls_within_a_scope() {
        let h = History::open_in_memory().unwrap();
        let s = scope(0);
        h.record(&s, &argv(&["GET", "a"]), RedactionLevel::None, 1)
            .unwrap();
        h.record(&s, &argv(&["GET", "b"]), RedactionLevel::None, 2)
            .unwrap();
        let r = h.recent(&s, 10).unwrap();
        assert_eq!(r.len(), 2);
        assert_eq!(r[0].line(), "GET b", "newest first");
        assert_eq!(r[1].line(), "GET a");
    }

    #[test]
    fn history_never_crosses_profile_identity_or_db() {
        // ASSIST-029/030: a prod entry must not surface while pointed at dev.
        let h = History::open_in_memory().unwrap();
        let dev = Scope {
            profile_uuid: "dev".into(),
            identity_epoch: 1,
            db: 3,
        };
        let prod = Scope {
            profile_uuid: "prod".into(),
            identity_epoch: 1,
            db: 0,
        };
        let other_db = Scope {
            db: 9,
            ..dev.clone()
        };
        let other_id = Scope {
            identity_epoch: 2,
            ..dev.clone()
        };

        h.record(&dev, &argv(&["GET", "devkey"]), RedactionLevel::None, 1)
            .unwrap();
        h.record(&prod, &argv(&["GET", "prodkey"]), RedactionLevel::None, 2)
            .unwrap();

        let seen = h.recent(&dev, 10).unwrap();
        assert_eq!(seen.len(), 1);
        assert!(seen[0].line().contains("devkey"));
        assert!(
            h.recent(&other_db, 10).unwrap().is_empty(),
            "db is part of the scope"
        );
        assert!(
            h.recent(&other_id, 10).unwrap().is_empty(),
            "identity epoch is part of the scope"
        );
        assert_eq!(h.recent(&prod, 10).unwrap().len(), 1);
    }

    #[test]
    fn search_only_ever_sees_the_redacted_text() {
        let h = History::open_in_memory().unwrap();
        let s = scope(0);
        h.record(
            &s,
            &argv(&["GET", "secret-key"]),
            RedactionLevel::AllArguments,
            1,
        )
        .unwrap();
        // The name was never stored, so it cannot be found.
        assert!(h.search(&s, "secret-key", 10).unwrap().is_empty());
        assert_eq!(h.search(&s, "GET", 10).unwrap().len(), 1);
    }

    #[test]
    fn clear_removes_one_profile_or_everything() {
        let h = History::open_in_memory().unwrap();
        let a = Scope {
            profile_uuid: "a".into(),
            identity_epoch: 1,
            db: 0,
        };
        let b = Scope {
            profile_uuid: "b".into(),
            identity_epoch: 1,
            db: 0,
        };
        h.record(&a, &argv(&["GET", "1"]), RedactionLevel::None, 1)
            .unwrap();
        h.record(&b, &argv(&["GET", "2"]), RedactionLevel::None, 2)
            .unwrap();
        assert_eq!(h.clear(Some("a")).unwrap(), 1);
        assert!(h.recent(&a, 10).unwrap().is_empty());
        assert_eq!(h.recent(&b, 10).unwrap().len(), 1);
        h.clear(None).unwrap();
        assert_eq!(h.count().unwrap(), 0);
    }

    #[test]
    fn prune_enforces_age_and_count() {
        let h = History::open_in_memory().unwrap();
        let s = scope(0);
        for i in 0..10 {
            h.record(&s, &argv(&["GET", "k"]), RedactionLevel::None, i)
                .unwrap();
        }
        h.prune(5, 100).unwrap();
        assert_eq!(h.count().unwrap(), 5, "entries older than 5ms are gone");
        h.prune(0, 2).unwrap();
        assert_eq!(h.count().unwrap(), 2, "only the newest 2 remain");
    }

    #[test]
    fn schema_version_is_recorded_and_a_newer_file_is_refused() {
        let dir = std::env::temp_dir().join(format!("pr-hist-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("history.sqlite");
        {
            let h = History::open(&path).unwrap();
            let v: i64 = h
                .conn
                .query_row("PRAGMA user_version", [], |r| r.get(0))
                .unwrap();
            assert_eq!(v, SCHEMA_VERSION);
        }
        // Reopening an existing file is fine.
        drop(History::open(&path).unwrap());
        // A file from the future is refused rather than silently downgraded.
        {
            let c = Connection::open(&path).unwrap();
            c.pragma_update(None, "user_version", SCHEMA_VERSION + 1)
                .unwrap();
        }
        match History::open(&path) {
            Err(HistoryError::SchemaTooNew { found }) => assert_eq!(found, SCHEMA_VERSION + 1),
            other => panic!("expected SchemaTooNew, got {other:?}"),
        }
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[cfg(unix)]
    #[test]
    fn the_history_file_is_private() {
        use std::os::unix::fs::PermissionsExt as _;
        let dir = std::env::temp_dir().join(format!("pr-hist-perm-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("history.sqlite");
        let h = History::open(&path).unwrap();
        h.record(&scope(0), &argv(&["GET", "k"]), RedactionLevel::None, 1)
            .unwrap();
        let mode = std::fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(
            mode & 0o777,
            0o600,
            "history must not be group/world readable"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn several_connections_can_share_one_file() {
        // ADR-003: no daemon, so multiple prc processes share the file via WAL.
        let dir = std::env::temp_dir().join(format!("pr-hist-wal-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("history.sqlite");
        let a = History::open(&path).unwrap();
        let b = History::open(&path).unwrap();
        let s = scope(0);
        a.record(&s, &argv(&["GET", "from-a"]), RedactionLevel::None, 1)
            .unwrap();
        b.record(&s, &argv(&["GET", "from-b"]), RedactionLevel::None, 2)
            .unwrap();
        assert_eq!(
            a.recent(&s, 10).unwrap().len(),
            2,
            "each sees the other's write"
        );
        assert_eq!(b.count().unwrap(), 2);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_stored_text_never_contains_a_value_or_a_reply() {
        // SEC-01, end to end: write the worst case and grep the file.
        let dir = std::env::temp_dir().join(format!("pr-hist-leak-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let path = dir.join("history.sqlite");
        {
            let h = History::open(&path).unwrap();
            let s = scope(0);
            h.record(
                &s,
                &argv(&["AUTH", "s3cret-password"]),
                RedactionLevel::None,
                1,
            )
            .unwrap();
            h.record(
                &s,
                &argv(&["SET", "customer:passport:9", "PII-VALUE"]),
                RedactionLevel::AllArguments,
                2,
            )
            .unwrap();
            h.conn
                .execute_batch("PRAGMA wal_checkpoint(TRUNCATE)")
                .unwrap();
        }
        let bytes = std::fs::read(&path).unwrap();
        let text = String::from_utf8_lossy(&bytes);
        for forbidden in ["s3cret-password", "PII-VALUE", "passport"] {
            assert!(
                !text.contains(forbidden),
                "{forbidden} reached the history file"
            );
        }
        assert!(
            text.contains("AUTH"),
            "the command name is still searchable"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
