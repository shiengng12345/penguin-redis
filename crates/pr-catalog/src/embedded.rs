//! The catalog that ships in the binary (v2.1 §11.4, §24.6, ADR-030, V-F01/V-H01).
//!
//! The pinned snapshots are embedded rather than read from disk: a catalog that can be
//! replaced by editing a file next to the executable is a catalog an attacker can edit, and
//! ADR-030 makes it the sole authority for what a command does.
//!
//! Compilation is **lazy**, and that is a budget decision, not an optimisation. §24.6 gives
//! `prc --help` 100 ms; parsing and compiling ~950 commands does not fit in that and does not
//! need to, because `--help` needs none of it. [`is_loaded`] exists so a test can prove the
//! fast paths really have not touched it (V-H01).

use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::compile::{Merged, compile, merge};
use crate::snapshot::{self, Snapshot};
use crate::spec::{CommandSpec, Family};

const REDIS_JSON: &[u8] = include_bytes!("../snapshots/redis-8.0.json");
const REDIS_SUM: &str = include_str!("../snapshots/redis-8.0.json.blake3");
const VALKEY_JSON: &[u8] = include_bytes!("../snapshots/valkey-8.1.json");
const VALKEY_SUM: &str = include_str!("../snapshots/valkey-8.1.json.blake3");

static LOADED: AtomicBool = AtomicBool::new(false);
static REDIS: OnceLock<Vec<CommandSpec>> = OnceLock::new();
static VALKEY: OnceLock<Vec<CommandSpec>> = OnceLock::new();
static MERGED: OnceLock<Merged> = OnceLock::new();

/// Whether anything has forced the catalog to compile yet.
///
/// A test asserts this is still false after `--help`, so making compilation eager becomes a
/// failing test rather than a startup-time regression nobody notices.
#[must_use]
pub fn is_loaded() -> bool {
    LOADED.load(Ordering::Acquire)
}

/// Parse an embedded snapshot, verifying its fingerprint.
///
/// # Panics
/// If an embedded snapshot does not match its digest. That is not a recoverable condition:
/// the binary was built from a catalog nobody signed off, and every policy decision made
/// from it would be untrustworthy. The build-time gate (`ci/check-catalog-snapshots.sh`)
/// makes reaching this impossible in a released build.
fn parse(bytes: &'static [u8], sum: &'static str) -> Snapshot {
    LOADED.store(true, Ordering::Release);
    match snapshot::load(bytes, sum) {
        Ok(s) => s,
        Err(e) => panic!("the embedded catalog snapshot is not the one that was reviewed: {e}"),
    }
}

/// The compiled Redis catalog.
#[must_use]
pub fn redis() -> &'static [CommandSpec] {
    REDIS.get_or_init(|| compile(&parse(REDIS_JSON, REDIS_SUM), Family::Redis))
}

/// The compiled Valkey catalog.
#[must_use]
pub fn valkey() -> &'static [CommandSpec] {
    VALKEY.get_or_init(|| compile(&parse(VALKEY_JSON, VALKEY_SUM), Family::Valkey))
}

/// Both families merged, with the divergence recorded.
#[must_use]
pub fn merged() -> &'static Merged {
    MERGED.get_or_init(|| merge(redis().to_vec(), valkey().to_vec()))
}

/// Look a command up by canonical name, e.g. `HGET` or `CLIENT|LIST`.
///
/// A miss is not an error: an unknown command is classified `unknown`, which the policy layer
/// treats as maximum risk (§20.2).
#[must_use]
pub fn lookup(name: &str) -> Option<&'static CommandSpec> {
    let upper = name.to_uppercase();
    merged().commands.iter().find(|c| c.name == upper)
}

/// Provenance of what is embedded, for `prc --version` and bug reports.
#[must_use]
pub fn provenance() -> [(&'static str, String); 2] {
    let r = parse(REDIS_JSON, REDIS_SUM).provenance;
    let v = parse(VALKEY_JSON, VALKEY_SUM).provenance;
    [
        ("redis", format!("{} from {}", r.server_version, r.image)),
        ("valkey", format!("{} from {}", v.server_version, v.image)),
    ]
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    #[test]
    fn the_embedded_snapshots_verify_and_compile() {
        assert!(redis().len() > 500, "{}", redis().len());
        assert!(valkey().len() > 300, "{}", valkey().len());
        assert!(is_loaded());
    }

    #[test]
    fn lookup_finds_commands_and_subcommands() {
        assert_eq!(lookup("hget").map(|c| c.name.as_str()), Some("HGET"));
        assert_eq!(
            lookup("client|list").map(|c| c.name.as_str()),
            Some("CLIENT|LIST")
        );
        assert!(lookup("ACME.NOSUCH").is_none());
    }

    #[test]
    fn the_merged_catalog_keeps_both_families() {
        let m = merged();
        assert!(m.redis_only.iter().any(|n| n == "HEXPIRE"));
        assert!(m.valkey_only.iter().any(|n| n == "COMMANDLOG"));
        assert!(m.effect_conflicts.is_empty(), "{:?}", m.effect_conflicts);
    }

    #[test]
    fn provenance_names_the_pinned_images() {
        let p = provenance();
        assert!(p[0].1.contains("@sha256:"));
        assert!(p[1].1.contains("@sha256:"));
    }

    #[test]
    fn compiling_twice_returns_the_same_catalog() {
        // `OnceLock` means the cost is paid once; a second caller must not re-parse 600 KB.
        let a = redis().as_ptr();
        let b = redis().as_ptr();
        assert_eq!(a, b);
    }
}
