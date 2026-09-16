//! The pinned catalog snapshot: schema, integrity and loading (v2.1 §11.4, ADR-030, V-F01).
//!
//! A snapshot is captured once, from a **digest-pinned** server image, by
//! `ci/catalog-snapshot.sh`. It is a *build input*: a human reviews the diff, `xtask
//! catalog-stamp` fingerprints the exact file bytes, and the result is committed.
//!
//! This is not a hole in ADR-030. ADR-030 forbids a *live* server from influencing effects,
//! because a live server is the thing being guarded against. A reviewed, fingerprinted file
//! from a known image is the opposite situation, and the two are kept apart by construction:
//! nothing here ever talks to a socket, and [`crate::precedence`] — which does see live
//! replies — cannot widen effects.
//!
//! Integrity is over the file bytes rather than a canonical re-serialisation, so there is no
//! second serialiser to disagree with the first.
//!
//! Every struct here is `deny_unknown_fields`. Silently ignoring a field the capture wrote is
//! how a snapshot and the compiler drift apart without anyone noticing — this crate found
//! exactly that during V-F01, when a leftover integrity field survived a schema change and
//! was read by nothing.

use serde::{Deserialize, Serialize};

/// Schema version this build understands.
pub const SCHEMA: u32 = 1;

/// Where a snapshot came from.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Provenance {
    /// `redis` or `valkey`.
    pub family: String,
    /// Version line, e.g. `8.0`.
    pub line: String,
    /// Reported server version, e.g. `8.0.6`.
    pub server_version: String,
    /// The pinned image the capture ran against, including its digest.
    pub image: String,
    /// When it was captured, RFC 3339 UTC.
    pub captured_utc: String,
    /// Which introspection commands were read.
    pub source: Vec<String>,
}

/// One argument node exactly as `COMMAND DOCS` describes it, minus fields we never read.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RawArg {
    /// Argument name, e.g. `key`, `seconds`, `condition`.
    pub name: String,
    /// `key`, `string`, `integer`, `double`, `unix-time`, `pattern`, `pure-token`, `oneof`,
    /// `block`.
    #[serde(rename = "type")]
    pub kind: String,
    /// The literal keyword that introduces this argument, e.g. `EX`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
    /// Display form from the server docs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_text: Option<String>,
    /// First server version that accepted it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub since: Option<String>,
    /// Index into the command's key specs, present only on real keys.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key_spec_index: Option<u32>,
    /// May be omitted.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub optional: bool,
    /// May repeat.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub multiple: bool,
    /// The token repeats with each occurrence.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub multiple_token: bool,
    /// Children, for `oneof` and `block`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arguments: Option<Vec<RawArg>>,
}

/// One command as captured.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct RawCommand {
    /// Uppercase name; a subcommand is `CLIENT|LIST`.
    pub name: String,
    /// Uppercase container name for a subcommand.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub container: Option<String>,
    /// One-line summary.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    /// First version.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub since: Option<String>,
    /// Documentation group, e.g. `string`, `stream`, `bf`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub group: Option<String>,
    /// Complexity note.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub complexity: Option<String>,
    /// Arity as reported by `COMMAND`; negative means "at least".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arity: Option<i64>,
    /// Command flags, e.g. `write`, `readonly`, `admin`, `blocking`.
    #[serde(default)]
    pub flags: Vec<String>,
    /// ACL categories, e.g. `@dangerous`.
    #[serde(default)]
    pub acl: Vec<String>,
    /// First key position, or 0.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub first_key: Option<i64>,
    /// Last key position.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_key: Option<i64>,
    /// Step between keys.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key_step: Option<i64>,
    /// Argument grammar.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arguments: Option<Vec<RawArg>>,
}

/// A whole snapshot file.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Snapshot {
    /// Schema version.
    pub schema: u32,
    /// Capture provenance.
    pub provenance: Provenance,
    /// Commands, sorted by name.
    pub commands: Vec<RawCommand>,
}

/// Why a snapshot could not be loaded.
#[derive(Debug, thiserror::Error)]
pub enum LoadError {
    /// The file is not valid JSON, or does not match the schema.
    #[error("snapshot is not readable: {0}")]
    Parse(#[from] serde_json::Error),
    /// The schema version is not the one this build understands. Refusing is deliberate:
    /// silently ignoring unknown fields is how a new argument kind becomes a wrong role.
    #[error("snapshot schema {found} is not supported (this build reads {SCHEMA})")]
    Schema {
        /// What the file declared.
        found: u32,
    },
    /// The file bytes do not match the stamped digest.
    #[error("snapshot digest mismatch: expected {expected}, file hashes to {actual}")]
    Digest {
        /// The digest recorded beside the file.
        expected: String,
        /// What the file actually hashes to.
        actual: String,
    },
    /// Commands are not sorted, or a name repeats. Both break reviewable diffs.
    #[error("snapshot is not canonical: {0}")]
    NotCanonical(String),
}

/// The blake3 digest of a snapshot's exact bytes, lowercase hex.
#[must_use]
pub fn digest(bytes: &[u8]) -> String {
    blake3::hash(bytes).to_hex().to_string()
}

/// Parse a snapshot, checking the digest and canonical form.
///
/// `expected_digest` is the content of the `.blake3` sidecar. It is required rather than
/// optional: an unstamped snapshot is one nobody has reviewed.
///
/// # Errors
/// Returns [`LoadError`] if the digest, schema or ordering does not hold.
pub fn load(bytes: &[u8], expected_digest: &str) -> Result<Snapshot, LoadError> {
    let actual = digest(bytes);
    let expected = expected_digest.trim();
    if actual != expected {
        return Err(LoadError::Digest {
            expected: expected.to_owned(),
            actual,
        });
    }
    let snap: Snapshot = serde_json::from_slice(bytes)?;
    if snap.schema != SCHEMA {
        return Err(LoadError::Schema { found: snap.schema });
    }
    check_canonical(&snap)?;
    Ok(snap)
}

/// Parse without verifying the digest. For the stamping tool only.
///
/// # Errors
/// Returns [`LoadError`] if the file does not parse or is not canonical.
pub fn parse_unverified(bytes: &[u8]) -> Result<Snapshot, LoadError> {
    let snap: Snapshot = serde_json::from_slice(bytes)?;
    if snap.schema != SCHEMA {
        return Err(LoadError::Schema { found: snap.schema });
    }
    check_canonical(&snap)?;
    Ok(snap)
}

fn check_canonical(snap: &Snapshot) -> Result<(), LoadError> {
    let mut prev: Option<&str> = None;
    for c in &snap.commands {
        if let Some(p) = prev
            && c.name.as_str() <= p
        {
            return Err(LoadError::NotCanonical(format!(
                "commands must be sorted and unique; {p:?} is followed by {:?}",
                c.name
            )));
        }
        if c.name != c.name.to_uppercase() {
            return Err(LoadError::NotCanonical(format!(
                "command name {:?} is not uppercase",
                c.name
            )));
        }
        prev = Some(&c.name);
    }
    Ok(())
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;

    fn minimal(commands: &str) -> String {
        format!(
            r#"{{"schema":1,"provenance":{{"family":"redis","line":"8.0","server_version":"8.0.6","image":"redis@sha256:aa","captured_utc":"2026-01-01T00:00:00Z","source":["COMMAND DOCS"]}},"commands":[{commands}]}}"#
        )
    }

    #[test]
    fn a_correctly_stamped_snapshot_loads() {
        let body = minimal(r#"{"name":"GET","flags":["readonly"],"acl":[]}"#);
        let d = digest(body.as_bytes());
        let s = load(body.as_bytes(), &d).unwrap();
        assert_eq!(s.commands.len(), 1);
        assert_eq!(s.provenance.family, "redis");
        assert!(
            s.provenance.image.contains("sha256:"),
            "provenance must record the pinned digest, not a floating tag"
        );
    }

    #[test]
    fn one_edited_byte_is_refused() {
        // The whole point: hand-patching `write` to `readonly` must not load.
        let body = minimal(r#"{"name":"SET","flags":["write"],"acl":[]}"#);
        let d = digest(body.as_bytes());
        let tampered = body.replace(r#""write""#, r#""readonly""#);
        assert_ne!(tampered, body);
        let err = load(tampered.as_bytes(), &d).unwrap_err();
        assert!(matches!(err, LoadError::Digest { .. }), "got {err:?}");
    }

    #[test]
    fn a_trailing_newline_in_the_sidecar_is_tolerated() {
        let body = minimal(r#"{"name":"GET","flags":[],"acl":[]}"#);
        let d = format!("{}\n", digest(body.as_bytes()));
        assert!(load(body.as_bytes(), &d).is_ok());
    }

    #[test]
    fn an_unknown_schema_is_refused_rather_than_partially_read() {
        let body = minimal(r#"{"name":"GET","flags":[],"acl":[]}"#)
            .replace(r#""schema":1"#, r#""schema":2"#);
        let d = digest(body.as_bytes());
        assert!(matches!(
            load(body.as_bytes(), &d).unwrap_err(),
            LoadError::Schema { found: 2 }
        ));
    }

    #[test]
    fn unsorted_or_duplicated_commands_are_refused() {
        for bad in [
            r#"{"name":"SET","flags":[],"acl":[]},{"name":"GET","flags":[],"acl":[]}"#,
            r#"{"name":"GET","flags":[],"acl":[]},{"name":"GET","flags":[],"acl":[]}"#,
        ] {
            let body = minimal(bad);
            let d = digest(body.as_bytes());
            assert!(
                matches!(
                    load(body.as_bytes(), &d).unwrap_err(),
                    LoadError::NotCanonical(_)
                ),
                "should have rejected {bad}"
            );
        }
    }

    #[test]
    fn a_lowercase_command_name_is_refused() {
        let body = minimal(r#"{"name":"get","flags":[],"acl":[]}"#);
        let d = digest(body.as_bytes());
        assert!(matches!(
            load(body.as_bytes(), &d).unwrap_err(),
            LoadError::NotCanonical(_)
        ));
    }

    #[test]
    fn the_digest_is_over_bytes_so_reformatting_changes_it() {
        // Deliberate: a reformatted file is a file nobody reviewed in that form.
        let a = minimal(r#"{"name":"GET","flags":[],"acl":[]}"#);
        let b = a.replace(",\"commands\"", ", \"commands\"");
        assert_ne!(digest(a.as_bytes()), digest(b.as_bytes()));
    }
}
