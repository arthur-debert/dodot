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

/// Extract all entries from an archive into memory, keyed by their
/// relative path. Skips directory entries (they're recreated on write).
///
/// Paths are validated: absolute paths and `..` components are
/// rejected with [`ArchiveError::UnsafePath`] so a malicious archive
/// can never escape the datastore subdir.
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
pub fn read_member(
    bytes: &[u8],
    format: ArchiveFormat,
    member: &str,
) -> Result<ArchiveEntry, ArchiveError> {
    let target = std::path::Path::new(member);
    let all = read_all(bytes, format)?;
    all.into_iter()
        .find_map(|(p, e)| (p == target).then_some(e))
        .ok_or_else(|| ArchiveError::MissingMember(member.to_string()))
}

fn read_tar_gz(bytes: &[u8]) -> Result<HashMap<PathBuf, ArchiveEntry>, ArchiveError> {
    let gz = flate2::read::GzDecoder::new(Cursor::new(bytes));
    let mut archive = tar::Archive::new(gz);
    let mut out: HashMap<PathBuf, ArchiveEntry> = HashMap::new();
    for entry in archive
        .entries()
        .map_err(|e| ArchiveError::Read(e.to_string()))?
    {
        let mut entry = entry.map_err(|e| ArchiveError::Read(e.to_string()))?;
        let raw_path = entry
            .path()
            .map_err(|e| ArchiveError::Read(e.to_string()))?
            .into_owned();
        let safe = validate_safe_archive_path(&raw_path)?;
        let header = entry.header();
        let mode = header.mode().ok();
        let is_dir = header.entry_type().is_dir();
        let mut buf = Vec::new();
        if !is_dir {
            entry
                .read_to_end(&mut buf)
                .map_err(|e| ArchiveError::Read(e.to_string()))?;
        }
        out.insert(
            safe.clone(),
            ArchiveEntry {
                path: safe,
                is_dir,
                bytes: buf,
                mode,
            },
        );
    }
    Ok(out)
}

fn read_zip(bytes: &[u8]) -> Result<HashMap<PathBuf, ArchiveEntry>, ArchiveError> {
    let mut archive =
        zip::ZipArchive::new(Cursor::new(bytes)).map_err(|e| ArchiveError::Read(e.to_string()))?;
    let mut out: HashMap<PathBuf, ArchiveEntry> = HashMap::new();
    for i in 0..archive.len() {
        let mut file = archive
            .by_index(i)
            .map_err(|e| ArchiveError::Read(e.to_string()))?;
        // ZipArchive's `enclosed_name()` rejects paths that would
        // escape the extraction root (absolute, `..` segments). It
        // returns `None` in those cases — we treat that as an unsafe
        // entry rather than silently dropping.
        let raw_path = file
            .enclosed_name()
            .ok_or_else(|| ArchiveError::UnsafePath(file.name().to_string()))?;
        let safe = validate_safe_archive_path(&raw_path)?;
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
        out.insert(
            safe.clone(),
            ArchiveEntry {
                path: safe,
                is_dir,
                bytes: buf,
                mode,
            },
        );
    }
    Ok(out)
}

/// Reject archive paths that would escape the extraction root.
///
/// Returns the normalized relative path on success. Mirrors the
/// `validate_safe_relative` helper in `datastore::filesystem` but
/// scoped to archive contents (which can carry weirder shapes).
fn validate_safe_archive_path(raw: &std::path::Path) -> Result<PathBuf, ArchiveError> {
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
        // An archive entry that resolves to an empty path (e.g. just
        // `./`) is the archive root; skip it cleanly.
        return Err(ArchiveError::UnsafePath(raw.display().to_string()));
    }
    Ok(cleaned)
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
