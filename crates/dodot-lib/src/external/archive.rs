//! Archive extraction helpers for `type = "archive"` and
//! `type = "archive-file"` externals.
//!
//! Two formats are supported: gzipped tar (the GitHub-release default)
//! and zip. The format is either inferred from the URL filename or
//! declared explicitly in the TOML.

use std::collections::HashMap;
use std::io::{Cursor, Read};
use std::path::PathBuf;

use crate::external::ArchiveFormat;

/// Error category returned by extraction.
#[derive(Debug, thiserror::Error)]
pub enum ArchiveError {
    #[error("archive format could not be inferred from URL {0}; set `format` explicitly")]
    FormatUnknown(String),
    #[error("archive read failed: {0}")]
    Read(String),
    #[error("archive contains unsafe path: {0}")]
    UnsafePath(String),
    #[error("archive does not contain member: {0}")]
    MissingMember(String),
}

/// In-memory representation of a single entry pulled from an archive.
///
/// `path` is the entry path relative to the archive root, validated
/// safe (no absolute paths, no `..` components). `is_dir` is true for
/// directory entries (no body).
#[derive(Debug, Clone)]
pub struct ArchiveEntry {
    pub path: PathBuf,
    pub is_dir: bool,
    pub bytes: Vec<u8>,
    /// Unix permission mode. None for entries that don't carry one
    /// (zip entries without external attrs). Callers default to 0o644
    /// for files and 0o755 for directories.
    pub mode: Option<u32>,
}

/// Extract every entry from an archive into memory, keyed by relative
/// path. Both regular files and directory markers are returned (callers
/// distinguish via [`ArchiveEntry::is_dir`]) so explicit empty
/// directories survive the round-trip.
///
/// Paths are validated: absolute paths and `..` components are
/// rejected with [`ArchiveError::UnsafePath`] so a malicious archive
/// can never escape the datastore subdir. Tar entries that are
/// neither regular files nor directories (symlinks, hardlinks,
/// devices, FIFOs, …) are rejected explicitly — dodot does not yet
/// have a defensible posture for materialising them, so silently
/// turning them into empty files would be a footgun.
pub fn read_all(
    bytes: &[u8],
    format: ArchiveFormat,
) -> Result<HashMap<PathBuf, ArchiveEntry>, ArchiveError> {
    match format {
        ArchiveFormat::TarGz => read_tar_gz(bytes),
        ArchiveFormat::Zip => read_zip(bytes),
    }
}

/// Extract a single named entry from an archive into memory.
///
/// Streams through the archive and returns the first matching entry
/// without buffering the rest — handy for `type = "archive-file"`
/// against multi-GB release tarballs.
pub fn read_member(
    bytes: &[u8],
    format: ArchiveFormat,
    member: &str,
) -> Result<ArchiveEntry, ArchiveError> {
    let target = std::path::Path::new(member);
    match format {
        ArchiveFormat::TarGz => read_tar_gz_one(bytes, target),
        ArchiveFormat::Zip => read_zip_one(bytes, target),
    }
}

fn read_tar_gz(bytes: &[u8]) -> Result<HashMap<PathBuf, ArchiveEntry>, ArchiveError> {
    let gz = flate2::read::GzDecoder::new(Cursor::new(bytes));
    let mut archive = tar::Archive::new(gz);
    let mut out: HashMap<PathBuf, ArchiveEntry> = HashMap::new();
    for entry in archive
        .entries()
        .map_err(|e| ArchiveError::Read(e.to_string()))?
    {
        let entry = entry.map_err(|e| ArchiveError::Read(e.to_string()))?;
        if let Some(parsed) = read_tar_entry(entry)? {
            out.insert(parsed.path.clone(), parsed);
        }
    }
    Ok(out)
}

fn read_tar_gz_one(bytes: &[u8], member: &std::path::Path) -> Result<ArchiveEntry, ArchiveError> {
    let gz = flate2::read::GzDecoder::new(Cursor::new(bytes));
    let mut archive = tar::Archive::new(gz);
    for entry in archive
        .entries()
        .map_err(|e| ArchiveError::Read(e.to_string()))?
    {
        let entry = entry.map_err(|e| ArchiveError::Read(e.to_string()))?;
        if let Some(parsed) = read_tar_entry(entry)? {
            if parsed.path == member {
                return Ok(parsed);
            }
        }
    }
    Err(ArchiveError::MissingMember(member.display().to_string()))
}

/// Parse one tar entry into an [`ArchiveEntry`].
///
/// Returns `Ok(None)` when the entry should be silently skipped (root
/// `./` placeholder). Returns `Err(UnsafePath)` for entries whose
/// type is not supported (symlinks, hardlinks, device nodes, etc.) —
/// we don't have a defensible materialisation story for those and
/// silently dropping the body would mislead callers.
fn read_tar_entry<R: std::io::Read>(
    mut entry: tar::Entry<'_, R>,
) -> Result<Option<ArchiveEntry>, ArchiveError> {
    let raw_path = entry
        .path()
        .map_err(|e| ArchiveError::Read(e.to_string()))?
        .into_owned();
    let safe = match validate_safe_archive_path(&raw_path)? {
        Some(p) => p,
        None => return Ok(None), // pure `./` root placeholder
    };
    let header_entry_type = entry.header().entry_type();
    let mode = entry.header().mode().ok();
    let is_dir = header_entry_type.is_dir();
    let is_regular = header_entry_type.is_file();
    if !is_dir && !is_regular {
        // Symlinks, hardlinks, fifos, char/block devices, sparse, etc.
        // Reject explicitly so the caller can surface a clear error
        // rather than silently materialising an empty file.
        return Err(ArchiveError::UnsafePath(format!(
            "unsupported tar entry type {:?} at {}",
            header_entry_type,
            raw_path.display()
        )));
    }
    let mut buf = Vec::new();
    if !is_dir {
        entry
            .read_to_end(&mut buf)
            .map_err(|e| ArchiveError::Read(e.to_string()))?;
    }
    Ok(Some(ArchiveEntry {
        path: safe,
        is_dir,
        bytes: buf,
        mode,
    }))
}

fn read_zip(bytes: &[u8]) -> Result<HashMap<PathBuf, ArchiveEntry>, ArchiveError> {
    let mut archive =
        zip::ZipArchive::new(Cursor::new(bytes)).map_err(|e| ArchiveError::Read(e.to_string()))?;
    let mut out: HashMap<PathBuf, ArchiveEntry> = HashMap::new();
    for i in 0..archive.len() {
        if let Some(parsed) = read_zip_index(&mut archive, i)? {
            out.insert(parsed.path.clone(), parsed);
        }
    }
    Ok(out)
}

fn read_zip_one(bytes: &[u8], member: &std::path::Path) -> Result<ArchiveEntry, ArchiveError> {
    let mut archive =
        zip::ZipArchive::new(Cursor::new(bytes)).map_err(|e| ArchiveError::Read(e.to_string()))?;
    for i in 0..archive.len() {
        if let Some(parsed) = read_zip_index(&mut archive, i)? {
            if parsed.path == member {
                return Ok(parsed);
            }
        }
    }
    Err(ArchiveError::MissingMember(member.display().to_string()))
}

fn read_zip_index(
    archive: &mut zip::ZipArchive<Cursor<&[u8]>>,
    index: usize,
) -> Result<Option<ArchiveEntry>, ArchiveError> {
    let mut file = archive
        .by_index(index)
        .map_err(|e| ArchiveError::Read(e.to_string()))?;
    // ZipArchive's `enclosed_name()` rejects paths that would escape
    // the extraction root (absolute, `..` segments). It returns
    // `None` in those cases — we treat that as an unsafe entry
    // rather than silently dropping.
    let raw_path = file
        .enclosed_name()
        .ok_or_else(|| ArchiveError::UnsafePath(file.name().to_string()))?;
    let safe = match validate_safe_archive_path(&raw_path)? {
        Some(p) => p,
        None => return Ok(None),
    };
    let is_dir = file.is_dir();
    // `unix_mode()` returns the full st_mode with file-type bits;
    // mask to the permission portion so callers see a familiar
    // 0o755 / 0o644 rather than 0o100644.
    let mode = file.unix_mode().map(|m| m & 0o7777);
    let mut buf = Vec::new();
    if !is_dir {
        file.read_to_end(&mut buf)
            .map_err(|e| ArchiveError::Read(e.to_string()))?;
    }
    Ok(Some(ArchiveEntry {
        path: safe,
        is_dir,
        bytes: buf,
        mode,
    }))
}

/// Validate an archive-entry path against the rules that keep
/// extraction inside the datastore subdir.
///
/// - `Ok(Some(path))` — safe relative path, materialise this entry.
/// - `Ok(None)` — entry resolves to the archive root (e.g. a bare
///   `./` placeholder). Skip it silently; tarballs produced by GNU
///   `tar` often start with such a row.
/// - `Err(UnsafePath)` — the entry has an absolute path, a `..`
///   segment, or a Windows-style prefix. Refuse the whole archive.
fn validate_safe_archive_path(raw: &std::path::Path) -> Result<Option<PathBuf>, ArchiveError> {
    use std::path::Component;
    let mut cleaned = PathBuf::new();
    for component in raw.components() {
        match component {
            Component::Normal(n) => cleaned.push(n),
            // Skip pure `./` segments rather than fail (tar archives
            // produced by `tar` itself frequently start with `./`).
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                return Err(ArchiveError::UnsafePath(raw.display().to_string()));
            }
        }
    }
    if cleaned.as_os_str().is_empty() {
        Ok(None)
    } else {
        Ok(Some(cleaned))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use flate2::write::GzEncoder;
    use flate2::Compression;
    use std::io::Write;

    /// Build a tiny tar.gz with two entries for tests.
    fn fake_tar_gz() -> Vec<u8> {
        let mut tar_buf: Vec<u8> = Vec::new();
        {
            let mut builder = tar::Builder::new(&mut tar_buf);
            // Plain file.
            let mut header = tar::Header::new_gnu();
            let body = b"hello tar\n";
            header.set_path("themes/alpha.zsh-theme").unwrap();
            header.set_size(body.len() as u64);
            header.set_mode(0o644);
            header.set_cksum();
            builder.append(&header, &body[..]).unwrap();

            // Nested file.
            let mut header = tar::Header::new_gnu();
            let body = b"#!/bin/sh\necho hi\n";
            header.set_path("themes/scripts/setup.sh").unwrap();
            header.set_size(body.len() as u64);
            header.set_mode(0o755);
            header.set_cksum();
            builder.append(&header, &body[..]).unwrap();
            builder.finish().unwrap();
        }
        let mut gz = GzEncoder::new(Vec::new(), Compression::default());
        gz.write_all(&tar_buf).unwrap();
        gz.finish().unwrap()
    }

    /// Build a tiny zip with one file for tests.
    fn fake_zip() -> Vec<u8> {
        let mut buf: Vec<u8> = Vec::new();
        {
            let mut writer = zip::ZipWriter::new(Cursor::new(&mut buf));
            let opts: zip::write::FileOptions<'_, ()> = zip::write::FileOptions::default()
                .compression_method(zip::CompressionMethod::Deflated)
                .unix_permissions(0o644);
            writer.start_file("hello.txt", opts).unwrap();
            writer.write_all(b"zipped hello").unwrap();
            writer.finish().unwrap();
        }
        buf
    }

    #[test]
    fn read_all_tar_gz_returns_entries() {
        let entries = read_all(&fake_tar_gz(), ArchiveFormat::TarGz).unwrap();
        assert!(entries.contains_key(&PathBuf::from("themes/alpha.zsh-theme")));
        assert!(entries.contains_key(&PathBuf::from("themes/scripts/setup.sh")));
        let e = entries
            .get(&PathBuf::from("themes/scripts/setup.sh"))
            .unwrap();
        assert_eq!(e.bytes, b"#!/bin/sh\necho hi\n");
        assert_eq!(e.mode, Some(0o755));
    }

    #[test]
    fn read_member_tar_gz_finds_named_entry() {
        let e = read_member(
            &fake_tar_gz(),
            ArchiveFormat::TarGz,
            "themes/alpha.zsh-theme",
        )
        .unwrap();
        assert_eq!(e.bytes, b"hello tar\n");
    }

    #[test]
    fn read_member_tar_gz_missing_errors_clearly() {
        let err = read_member(&fake_tar_gz(), ArchiveFormat::TarGz, "no/such.txt").unwrap_err();
        assert!(matches!(err, ArchiveError::MissingMember(_)), "{err:?}");
    }

    #[test]
    fn read_all_zip_returns_entries() {
        let entries = read_all(&fake_zip(), ArchiveFormat::Zip).unwrap();
        let e = entries.get(&PathBuf::from("hello.txt")).unwrap();
        assert_eq!(e.bytes, b"zipped hello");
        assert_eq!(e.mode, Some(0o644));
    }

    #[test]
    fn unsafe_paths_rejected() {
        // The high-level `tar::Builder` refuses to write `..` paths,
        // and `zip::ZipArchive::enclosed_name` filters them out on
        // read — so directly exercise our validator on a synthetic
        // path that would otherwise reach an extraction site.
        let cases = ["../escape", "subdir/../../escape", "/absolute/escape"];
        for p in cases {
            let err = validate_safe_archive_path(std::path::Path::new(p)).unwrap_err();
            assert!(
                matches!(err, ArchiveError::UnsafePath(_)),
                "expected UnsafePath for {p:?}, got {err:?}"
            );
        }
    }

    #[test]
    fn root_placeholder_returns_none_not_err() {
        // A bare `./` entry (common in tarballs produced by GNU tar)
        // resolves to an empty cleaned path. The validator returns
        // `Ok(None)` so the reader can skip it silently — earlier
        // behaviour was `Err(UnsafePath)`, which would have failed
        // every otherwise-valid tarball with a root placeholder.
        let result = validate_safe_archive_path(std::path::Path::new("./")).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn tar_symlink_entries_are_rejected() {
        // Build a tar that contains a symlink entry — should be
        // refused so we don't silently extract an empty file.
        let mut tar_buf: Vec<u8> = Vec::new();
        {
            let mut builder = tar::Builder::new(&mut tar_buf);
            let mut header = tar::Header::new_gnu();
            header.set_path("link").unwrap();
            header.set_size(0);
            header.set_mode(0o644);
            header.set_entry_type(tar::EntryType::Symlink);
            header.set_link_name("target").unwrap();
            header.set_cksum();
            builder.append(&header, std::io::empty()).unwrap();
            builder.finish().unwrap();
        }
        let mut gz = GzEncoder::new(Vec::new(), Compression::default());
        gz.write_all(&tar_buf).unwrap();
        let bytes = gz.finish().unwrap();
        let err = read_all(&bytes, ArchiveFormat::TarGz).unwrap_err();
        assert!(
            matches!(err, ArchiveError::UnsafePath(ref m) if m.contains("Symlink")),
            "expected UnsafePath about Symlink, got {err:?}"
        );
    }

    #[test]
    fn infer_format_from_url() {
        assert_eq!(
            ArchiveFormat::infer_from_url("https://x/foo.tar.gz"),
            Some(ArchiveFormat::TarGz)
        );
        assert_eq!(
            ArchiveFormat::infer_from_url("https://x/foo.tgz?v=1"),
            Some(ArchiveFormat::TarGz)
        );
        assert_eq!(
            ArchiveFormat::infer_from_url("https://x/Foo.ZIP"),
            Some(ArchiveFormat::Zip)
        );
        assert_eq!(ArchiveFormat::infer_from_url("https://x/foo.unknown"), None);
    }
}
