//! Verification evidence.
//!
//! One record per claim, written only by a verifier run, binding the result to
//! the exact source state it observed. Replaces the provider-specific
//! `.claude/l*-accept-sha` stamps with something provider-neutral and
//! machine-readable: a Codex, Gemini, or CI run produces the same record a
//! Claude run does, and a product surface can read it without knowing which
//! agent was at the keyboard.
//!
//! Records are local, reviewable evidence, not signed attestations. Digests
//! detect stale inputs; anyone able to edit the record and source can fabricate
//! a passing result. Independent CI and review remain separate proof boundaries.

use std::fs::{self, File, OpenOptions};
use std::io::{ErrorKind, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use serde::{Deserialize, Serialize};

use crate::ClaimError;

/// Verifier output is kept for diagnosis, not for archive. Bounded because a
/// claim record is committed: an unbounded tail would put build logs, and
/// eventually whatever a build log happens to print, into git history.
const MAX_DETAIL: usize = 2000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum VerifyResult {
    Pass,
    Fail,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Environment {
    pub os: String,
    pub arch: String,
}

impl Environment {
    pub fn current() -> Self {
        Self {
            os: std::env::consts::OS.to_string(),
            arch: std::env::consts::ARCH.to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvidenceRecord {
    pub claim_id: String,
    /// Older records without requirement binding must be reverified.
    #[serde(default)]
    pub claim_digest: String,
    pub verifier_id: String,
    /// Bound so that changing a verifier's meaning degrades every claim resting
    /// on the old meaning, instead of leaving stale proof looking current.
    pub verifier_version: String,
    pub result: VerifyResult,
    /// Commit the verifier observed. Ancestry against HEAD is checked on read.
    pub commit: String,
    /// Digest of the claim's scope at `commit`.
    pub scope_digest: String,
    /// Unix seconds. Only consulted when the claim declares an expiry.
    pub verified_at: i64,
    pub environment: Environment,
    #[serde(default)]
    pub detail: String,
}

impl EvidenceRecord {
    pub fn truncate_detail(mut detail: String) -> String {
        if detail.len() > MAX_DETAIL {
            // Cut on a char boundary; verifier output is not guaranteed ASCII.
            let mut end = MAX_DETAIL;
            while end > 0 && !detail.is_char_boundary(end) {
                end -= 1;
            }
            detail.truncate(end);
            detail.push_str("\n… truncated");
        }
        detail
    }
}

pub fn path_for(repo_root: &Path, claim_id: &str) -> Result<PathBuf, ClaimError> {
    crate::manifest::validate_claim_id(claim_id)?;
    Ok(repo_root
        .join("claims")
        .join("evidence")
        .join(format!("{claim_id}.json")))
}

/// Read standing evidence, if any.
///
/// A record that will not parse is treated as absent rather than as an error:
/// the registry's job is to report an unproven claim, and refusing to run
/// because one file is corrupt would hide every other claim's state too. A
/// record reached through a link reads as absent for the same reason, and
/// because a link could present a file nobody reviewed, from outside the
/// repository, as this claim's proof.
pub fn load(repo_root: &Path, claim_id: &str) -> Option<EvidenceRecord> {
    let path = path_for(repo_root, claim_id).ok()?;
    for dir in evidence_dirs(repo_root) {
        if inspect_dir(&dir).ok()? != Presence::Present {
            return None;
        }
    }
    let inspected = inspect_record(&path).ok()??;
    let mut file = File::open(&path).ok()?;
    // What was inspected must be what was opened: a link swapped in between
    // the two calls would otherwise be followed.
    if !same_object(&inspected, &file.metadata().ok()?) {
        return None;
    }
    let mut text = String::new();
    file.read_to_string(&mut text).ok()?;
    serde_json::from_str(&text).ok()
}

/// Record evidence so that it can only land in a regular file inside this
/// repository's own `claims/evidence` directory.
///
/// A committed link at `claims`, `claims/evidence`, or the record is refused,
/// never followed: otherwise recording a proof would overwrite a file outside
/// the tree. The bytes go to a sibling this call creates exclusively, then
/// `rename` replaces the record. Rename swaps directory entries instead of
/// following a link, so a record replaced by a link after the checks still
/// cannot redirect the write, and no reader sees a half-written record.
pub fn store(repo_root: &Path, record: &EvidenceRecord) -> Result<PathBuf, ClaimError> {
    let path = path_for(repo_root, &record.claim_id)?;
    let [claims, dir] = evidence_dirs(repo_root);
    ensure_dir(&claims)?;
    ensure_dir(&dir)?;
    inspect_record(&path)?;

    let mut text = serde_json::to_string_pretty(record)
        .map_err(|e| ClaimError::Io(format!("serialize evidence: {e}")))?;
    text.push('\n');

    let (sibling, mut file) = create_sibling(&dir, &record.claim_id)?;
    let written = file
        .write_all(text.as_bytes())
        .and_then(|()| file.sync_all());
    drop(file);
    if let Err(e) = written.and_then(|()| fs::rename(&sibling, &path)) {
        // Best effort: the record itself was never opened for writing.
        let _ = fs::remove_file(&sibling);
        return Err(ClaimError::Io(format!("{}: {e}", path.display())));
    }
    Ok(path)
}

/// `claims` and `claims/evidence`, outermost first.
fn evidence_dirs(repo_root: &Path) -> [PathBuf; 2] {
    let claims = repo_root.join("claims");
    let evidence = claims.join("evidence");
    [claims, evidence]
}

#[derive(Debug, PartialEq, Eq)]
enum Presence {
    Missing,
    Present,
}

/// `symlink_metadata` never follows a link, so a link is judged as a link.
fn inspect_dir(path: &Path) -> Result<Presence, ClaimError> {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.file_type().is_dir() => Ok(Presence::Present),
        Ok(_) => Err(not_real(path, "directory")),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(Presence::Missing),
        Err(e) => Err(ClaimError::Io(format!("{}: {e}", path.display()))),
    }
}

/// Create one missing level. `create_dir` fails on any existing entry, a link
/// included; when a concurrent writer created it first, judge what it left.
fn ensure_dir(path: &Path) -> Result<(), ClaimError> {
    if inspect_dir(path)? == Presence::Present {
        return Ok(());
    }
    match fs::create_dir(path) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == ErrorKind::AlreadyExists => match inspect_dir(path)? {
            Presence::Present => Ok(()),
            Presence::Missing => Err(ClaimError::Io(format!("{}: {e}", path.display()))),
        },
        Err(e) => Err(ClaimError::Io(format!("{}: {e}", path.display()))),
    }
}

/// A record is absent or a regular file; a link, directory, or device is
/// refused.
fn inspect_record(path: &Path) -> Result<Option<fs::Metadata>, ClaimError> {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.file_type().is_file() => Ok(Some(meta)),
        Ok(_) => Err(not_real(path, "regular file")),
        Err(e) if e.kind() == ErrorKind::NotFound => Ok(None),
        Err(e) => Err(ClaimError::Io(format!("{}: {e}", path.display()))),
    }
}

fn not_real(path: &Path, kind: &str) -> ClaimError {
    ClaimError::Io(format!(
        "{} must be a real {kind}, not a link or another file type",
        path.display()
    ))
}

/// Temporary names are unique per process and per call, so concurrent writers
/// never share a sibling and the last complete rename wins.
fn create_sibling(dir: &Path, claim_id: &str) -> Result<(PathBuf, File), ClaimError> {
    static NEXT: AtomicU64 = AtomicU64::new(0);
    for _ in 0..64 {
        let sibling = dir.join(format!(
            ".{claim_id}.{}.{}.tmp",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        // `create_new` is O_CREAT|O_EXCL: it refuses any existing name, a link
        // included, so the bytes only ever land in a file this call created.
        match OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&sibling)
        {
            Ok(file) => return Ok((sibling, file)),
            Err(e) if e.kind() == ErrorKind::AlreadyExists => continue,
            Err(e) => return Err(ClaimError::Io(format!("{}: {e}", sibling.display()))),
        }
    }
    Err(ClaimError::Io(format!(
        "{}: no free temporary name for evidence",
        dir.display()
    )))
}

#[cfg(unix)]
fn same_object(inspected: &fs::Metadata, opened: &fs::Metadata) -> bool {
    use std::os::unix::fs::MetadataExt;
    opened.is_file() && inspected.dev() == opened.dev() && inspected.ino() == opened.ino()
}

#[cfg(not(unix))]
fn same_object(_inspected: &fs::Metadata, opened: &fs::Metadata) -> bool {
    opened.is_file()
}
