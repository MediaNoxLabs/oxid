// SPDX-License-Identifier: Apache-2.0

//! Owner-private atomic file store shared by Oxid's persistence adapters.
//!
//! Every durable store in Oxid owes its callers the same guarantees: a
//! replacement is atomic, the file and its directory are readable only by
//! their owner, a symlink can never redirect a write, and a bounded read
//! refuses an oversized file before allocating for it. Those rules were
//! implemented independently in each adapter and drifted: some copies lost
//! the parent-directory fsync that makes the rename durable, one lost the
//! temporary-file cleanup that leaks on failure, and one lost its symlink
//! rejection entirely.
//!
//! This crate is that ritual, once. It has no dependencies beyond `std` and
//! knows nothing about the documents it stores; each adapter maps
//! [`AtomicStoreError`] onto its own error type at the boundary.
//!
//! Guarantees provided by [`write_owner_private`]:
//!
//! 1. the destination is absent or a regular, owner-only file — never a
//!    symlink, directory, or world/group-readable file;
//! 2. the parent directory exists, is a real directory, is owner-only, and is
//!    created with `0o700` when missing;
//! 3. the payload is written to a fresh `0o600` temporary file in the same
//!    directory, flushed with `sync_all`, then renamed over the destination,
//!    so a reader sees either the previous or the next content and never a
//!    partial write;
//! 4. the parent directory is flushed after the rename, so the replacement
//!    survives a crash;
//! 5. the temporary file is removed if any step after its creation fails.

#![forbid(unsafe_code)]

use std::fs::{self, File, OpenOptions};
use std::io::{Read as _, Write as _};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// Why an owner-private store operation could not be completed.
///
/// The split is deliberate and adapters are expected to preserve it:
/// [`Self::Integrity`] means the filesystem state itself is untrustworthy and
/// the caller must fail closed, while [`Self::Unavailable`] means the
/// operation could not be carried out right now.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AtomicStoreError {
    /// A symlink, wrong file type, or permissive mode was found where an
    /// owner-private regular file or directory was required.
    Integrity,
    /// The operation failed for an environmental reason: the payload exceeded
    /// its bound, or the filesystem refused a step.
    Unavailable,
}

impl core::fmt::Display for AtomicStoreError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        formatter.write_str(match self {
            Self::Integrity => "owner-private store path failed its integrity checks",
            Self::Unavailable => "owner-private store operation is unavailable",
        })
    }
}

impl std::error::Error for AtomicStoreError {}

static TEMPORARY_FILE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// Reads a file that must be owner-private, rejecting anything larger than
/// `max_bytes` before it is allocated.
///
/// Returns `Ok(None)` when the file does not exist, so a first run is not an
/// error. The length is checked twice: once from the metadata, and once after
/// reading `max_bytes + 1` bytes, so a file that grows between the two checks
/// is still rejected.
pub fn read_owner_private_bounded(
    path: &Path,
    max_bytes: usize,
) -> Result<Option<Vec<u8>>, AtomicStoreError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() || !metadata.is_file() {
                return Err(AtomicStoreError::Integrity);
            }
            reject_permissive_mode(&metadata)?;
            if usize::try_from(metadata.len()).unwrap_or(usize::MAX) > max_bytes {
                return Err(AtomicStoreError::Unavailable);
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(_) => return Err(AtomicStoreError::Unavailable),
    }

    let file = File::open(path).map_err(|_| AtomicStoreError::Unavailable)?;
    let mut bytes = Vec::new();
    file.take(
        u64::try_from(max_bytes)
            .unwrap_or(u64::MAX)
            .saturating_add(1),
    )
    .read_to_end(&mut bytes)
    .map_err(|_| AtomicStoreError::Unavailable)?;
    if bytes.len() > max_bytes {
        return Err(AtomicStoreError::Unavailable);
    }
    Ok(Some(bytes))
}

/// Replaces `path` with `bytes` atomically, keeping the file and its parent
/// directory owner-private.
///
/// See the crate documentation for the exact guarantees.
pub fn write_owner_private(path: &Path, bytes: &[u8]) -> Result<(), AtomicStoreError> {
    reject_non_private_file(path)?;
    let parent = path.parent().ok_or(AtomicStoreError::Unavailable)?;
    ensure_private_directory(parent)?;

    let temporary = temporary_path(path);
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    apply_private_mode(&mut options);
    let mut file = options
        .open(&temporary)
        .map_err(|_| AtomicStoreError::Unavailable)?;

    // From here on any failure must not leave the temporary file behind.
    let outcome = file
        .write_all(bytes)
        .and_then(|()| file.sync_all())
        .map_err(|_| AtomicStoreError::Unavailable)
        .and_then(|()| {
            drop(file);
            fs::rename(&temporary, path).map_err(|_| AtomicStoreError::Unavailable)
        });
    if outcome.is_err() {
        let _ = fs::remove_file(&temporary);
        return outcome;
    }

    // The rename is only durable once the directory entry is flushed.
    sync_directory(parent);
    Ok(())
}

/// Rejects a destination that exists but is not an owner-private regular
/// file. A missing destination is accepted: the first write creates it.
pub fn reject_non_private_file(path: &Path) -> Result<(), AtomicStoreError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() || !metadata.is_file() {
                return Err(AtomicStoreError::Integrity);
            }
            reject_permissive_mode(&metadata)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err(AtomicStoreError::Unavailable),
    }
}

/// Rejects a directory that exists but is a symlink, not a directory, or
/// group/world-accessible. A missing directory is accepted, so a read path
/// can use this without creating anything.
pub fn reject_non_private_directory(path: &Path) -> Result<(), AtomicStoreError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() || !metadata.is_dir() {
                return Err(AtomicStoreError::Integrity);
            }
            reject_permissive_mode(&metadata)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err(AtomicStoreError::Unavailable),
    }
}

/// Ensures `path` is an owner-only directory, creating it with `0o700` when
/// it does not exist and rejecting a symlink or permissive mode when it does.
pub fn ensure_private_directory(path: &Path) -> Result<(), AtomicStoreError> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            if metadata.file_type().is_symlink() || !metadata.is_dir() {
                return Err(AtomicStoreError::Integrity);
            }
            return reject_permissive_mode(&metadata);
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => return Err(AtomicStoreError::Unavailable),
    }

    fs::create_dir_all(path).map_err(|_| AtomicStoreError::Unavailable)?;
    let metadata = fs::symlink_metadata(path).map_err(|_| AtomicStoreError::Unavailable)?;
    if metadata.file_type().is_symlink() || !metadata.is_dir() {
        return Err(AtomicStoreError::Integrity);
    }
    apply_private_directory_mode(path)
}

fn temporary_path(path: &Path) -> PathBuf {
    let name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("store");
    let sequence = TEMPORARY_FILE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    path.with_file_name(format!(".{name}.tmp-{}-{sequence}", std::process::id()))
}

#[cfg(unix)]
fn reject_permissive_mode(metadata: &fs::Metadata) -> Result<(), AtomicStoreError> {
    use std::os::unix::fs::PermissionsExt as _;
    if metadata.permissions().mode() & 0o077 == 0 {
        Ok(())
    } else {
        Err(AtomicStoreError::Integrity)
    }
}

#[cfg(not(unix))]
fn reject_permissive_mode(_: &fs::Metadata) -> Result<(), AtomicStoreError> {
    Ok(())
}

#[cfg(unix)]
fn apply_private_mode(options: &mut OpenOptions) {
    use std::os::unix::fs::OpenOptionsExt as _;
    options.mode(0o600);
}

#[cfg(not(unix))]
fn apply_private_mode(_: &mut OpenOptions) {}

#[cfg(unix)]
fn apply_private_directory_mode(path: &Path) -> Result<(), AtomicStoreError> {
    use std::os::unix::fs::PermissionsExt as _;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
        .map_err(|_| AtomicStoreError::Unavailable)
}

#[cfg(not(unix))]
fn apply_private_directory_mode(_: &Path) -> Result<(), AtomicStoreError> {
    Ok(())
}

/// Flushes the directory entry so a completed rename survives a crash.
///
/// A platform that cannot open a directory as a file, or refuses to sync one,
/// leaves the data written and the rename applied; only the durability
/// guarantee is weaker, so this is deliberately not an error.
fn sync_directory(path: &Path) {
    if let Ok(directory) = File::open(path) {
        let _ = directory.sync_all();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(label: &str) -> PathBuf {
        let sequence = TEMPORARY_FILE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let directory = std::env::temp_dir().join(format!(
            "oxid-store-atomic-{}-{label}-{sequence}",
            std::process::id()
        ));
        fs::create_dir_all(&directory).expect("scratch directory");
        // The primitive requires an owner-only parent, so the scratch root is
        // created the way a real adapter creates its private directory rather
        // than inheriting the process umask.
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            fs::set_permissions(&directory, fs::Permissions::from_mode(0o700))
                .expect("owner-only scratch directory");
        }
        directory
    }

    #[test]
    fn write_then_read_round_trips_and_creates_a_private_file() {
        let directory = scratch("round-trip");
        let path = directory.join("store.json");
        write_owner_private(&path, b"{}").expect("write succeeds");
        assert_eq!(
            read_owner_private_bounded(&path, 64).expect("read succeeds"),
            Some(b"{}".to_vec())
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt as _;
            let mode = fs::metadata(&path).expect("metadata").permissions().mode();
            assert_eq!(mode & 0o777, 0o600, "written file must be owner-only");
        }
    }

    #[test]
    fn a_missing_file_reads_as_absent_rather_than_failing() {
        let directory = scratch("absent");
        assert_eq!(
            read_owner_private_bounded(&directory.join("missing.json"), 64).expect("read succeeds"),
            None
        );
    }

    #[test]
    fn replacement_is_atomic_and_leaves_no_temporary_file() {
        let directory = scratch("replace");
        let path = directory.join("store.json");
        write_owner_private(&path, b"first").expect("first write");
        write_owner_private(&path, b"second-and-longer").expect("second write");
        assert_eq!(
            read_owner_private_bounded(&path, 64).expect("read succeeds"),
            Some(b"second-and-longer".to_vec())
        );
        let leftovers = fs::read_dir(&directory)
            .expect("directory listing")
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().contains(".tmp-"))
            .count();
        assert_eq!(leftovers, 0, "no temporary file may survive a write");
    }

    #[test]
    fn an_oversized_payload_is_rejected_before_allocation() {
        let directory = scratch("bounded");
        let path = directory.join("store.json");
        write_owner_private(&path, &[b'x'; 128]).expect("write succeeds");
        assert_eq!(
            read_owner_private_bounded(&path, 64),
            Err(AtomicStoreError::Unavailable)
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_symlinked_destination_is_rejected_instead_of_followed() {
        let directory = scratch("symlink");
        let target = directory.join("outside.json");
        fs::write(&target, b"original").expect("target file");
        let link = directory.join("store.json");
        std::os::unix::fs::symlink(&target, &link).expect("symlink");

        assert_eq!(
            write_owner_private(&link, b"redirected"),
            Err(AtomicStoreError::Integrity)
        );
        assert_eq!(
            read_owner_private_bounded(&link, 64),
            Err(AtomicStoreError::Integrity)
        );
        assert_eq!(
            fs::read(&target).expect("target still readable"),
            b"original".to_vec(),
            "a symlinked write must not reach the target"
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_world_readable_destination_is_rejected() {
        use std::os::unix::fs::PermissionsExt as _;
        let directory = scratch("permissive-file");
        let path = directory.join("store.json");
        fs::write(&path, b"exposed").expect("file");
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).expect("chmod");
        assert_eq!(
            write_owner_private(&path, b"replacement"),
            Err(AtomicStoreError::Integrity)
        );
        assert_eq!(
            read_owner_private_bounded(&path, 64),
            Err(AtomicStoreError::Integrity)
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_permissive_parent_directory_is_rejected() {
        use std::os::unix::fs::PermissionsExt as _;
        let directory = scratch("permissive-parent");
        let nested = directory.join("nested");
        fs::create_dir(&nested).expect("nested directory");
        fs::set_permissions(&nested, fs::Permissions::from_mode(0o755)).expect("chmod");
        assert_eq!(
            write_owner_private(&nested.join("store.json"), b"{}"),
            Err(AtomicStoreError::Integrity)
        );
    }

    #[cfg(unix)]
    #[test]
    fn a_created_parent_directory_is_owner_only() {
        use std::os::unix::fs::PermissionsExt as _;
        let directory = scratch("created-parent");
        let nested = directory.join("private").join("deeper");
        write_owner_private(&nested.join("store.json"), b"{}").expect("write succeeds");
        let mode = fs::metadata(&nested)
            .expect("metadata")
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o700, "created directory must be owner-only");
    }

    #[cfg(unix)]
    #[test]
    fn a_symlinked_parent_directory_is_rejected() {
        let directory = scratch("symlink-parent");
        let real = directory.join("real");
        fs::create_dir(&real).expect("real directory");
        let link = directory.join("linked");
        std::os::unix::fs::symlink(&real, &link).expect("symlink");
        assert_eq!(
            write_owner_private(&link.join("store.json"), b"{}"),
            Err(AtomicStoreError::Integrity)
        );
    }

    #[test]
    fn a_directory_in_place_of_the_destination_is_rejected() {
        let directory = scratch("directory-destination");
        let path = directory.join("store.json");
        fs::create_dir(&path).expect("directory at the destination");
        assert_eq!(
            write_owner_private(&path, b"{}"),
            Err(AtomicStoreError::Integrity)
        );
    }
}
