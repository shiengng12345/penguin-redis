//! Stamp and verify pinned catalog snapshots (V-F01).
//!
//! The digest is computed here, in Rust, with the same blake3 implementation that hashes
//! approvals (ADR-009), so the project has exactly one hashing path. The capture script
//! writes the JSON; this writes the fingerprint a reviewer's signature effectively covers.

use std::path::Path;

/// Compute and write `<path>.blake3`. Refuses a file that does not parse or is not canonical,
/// because stamping an unreadable snapshot would certify nothing.
///
/// # Errors
/// Returns a message describing why the file could not be stamped.
pub fn stamp(path: &Path) -> Result<String, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    pr_catalog::snapshot::parse_unverified(&bytes)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    let d = pr_catalog::snapshot::digest(&bytes);
    let side = sidecar(path);
    std::fs::write(&side, format!("{d}\n")).map_err(|e| format!("{}: {e}", side.display()))?;
    Ok(d)
}

/// Verify a snapshot against its sidecar and report what it contains.
///
/// # Errors
/// Returns a message if the sidecar is missing or the snapshot does not load.
pub fn verify(path: &Path) -> Result<String, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let side = sidecar(path);
    let expected = std::fs::read_to_string(&side)
        .map_err(|e| format!("{} (run `xtask catalog-stamp`): {e}", side.display()))?;
    let snap = pr_catalog::snapshot::load(&bytes, &expected)
        .map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(format!(
        "{} {} ({}) {} commands, captured {} from {}",
        snap.provenance.family,
        snap.provenance.line,
        snap.provenance.server_version,
        snap.commands.len(),
        snap.provenance.captured_utc,
        snap.provenance.image,
    ))
}

fn sidecar(path: &Path) -> std::path::PathBuf {
    let mut s = path.as_os_str().to_os_string();
    s.push(".blake3");
    std::path::PathBuf::from(s)
}
