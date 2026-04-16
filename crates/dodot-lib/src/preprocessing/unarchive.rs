//! Unarchive preprocessor — extracts tar.gz archives.
//!
//! Matches files with `.tar.gz` extension and extracts their contents.
//! Each file in the archive becomes an [`ExpandedFile`].
//!
//! This is an Opaque transformation: there is no reverse path
//! (you cannot re-archive deployed files back into the source).

use std::io::Read;
use std::path::Path;

use crate::fs::Fs;
use crate::preprocessing::{ExpandedFile, Preprocessor, TransformType};
use crate::{DodotError, Result};

/// A preprocessor that extracts `.tar.gz` archives.
pub struct UnarchivePreprocessor;

impl UnarchivePreprocessor {
    pub fn new() -> Self {
        Self
    }
}

impl Default for UnarchivePreprocessor {
    fn default() -> Self {
        Self::new()
    }
}

impl Preprocessor for UnarchivePreprocessor {
    fn name(&self) -> &str {
        "unarchive"
    }

    fn transform_type(&self) -> TransformType {
        TransformType::Opaque
    }

    fn matches_extension(&self, filename: &str) -> bool {
        filename.ends_with(".tar.gz")
    }

    fn stripped_name(&self, filename: &str) -> String {
        filename
            .strip_suffix(".tar.gz")
            .unwrap_or(filename)
            .to_string()
    }

    fn expand(&self, source: &Path, fs: &dyn Fs) -> Result<Vec<ExpandedFile>> {
        let reader = fs.open_read(source)?;
        let gz = flate2::read::GzDecoder::new(reader);
        let mut archive = tar::Archive::new(gz);

        let mut expanded = Vec::new();

        let entries = archive
            .entries()
            .map_err(|e| DodotError::PreprocessorError {
                preprocessor: "unarchive".into(),
                source_file: source.to_path_buf(),
                message: format!("failed to read archive entries: {e}"),
            })?;

        for entry_result in entries {
            let mut entry = entry_result.map_err(|e| DodotError::PreprocessorError {
                preprocessor: "unarchive".into(),
                source_file: source.to_path_buf(),
                message: format!("failed to read archive entry: {e}"),
            })?;

            let entry_path = entry
                .path()
                .map_err(|e| DodotError::PreprocessorError {
                    preprocessor: "unarchive".into(),
                    source_file: source.to_path_buf(),
                    message: format!("invalid path in archive: {e}"),
                })?
                .into_owned();

            let is_dir = entry.header().entry_type().is_dir();

            if is_dir {
                expanded.push(ExpandedFile {
                    relative_path: entry_path,
                    content: Vec::new(),
                    is_dir: true,
                });
            } else {
                let mut content = Vec::new();
                entry
                    .read_to_end(&mut content)
                    .map_err(|e| DodotError::PreprocessorError {
                        preprocessor: "unarchive".into(),
                        source_file: source.to_path_buf(),
                        message: format!("failed to read entry content: {e}"),
                    })?;

                expanded.push(ExpandedFile {
                    relative_path: entry_path,
                    content,
                    is_dir: false,
                });
            }
        }

        Ok(expanded)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_tar_gz_extension() {
        let pp = UnarchivePreprocessor::new();
        assert!(pp.matches_extension("bin.tar.gz"));
        assert!(pp.matches_extension("tools.tar.gz"));
        assert!(!pp.matches_extension("file.tar"));
        assert!(!pp.matches_extension("file.gz"));
        assert!(!pp.matches_extension("file.zip"));
        assert!(!pp.matches_extension("tar.gz")); // no base name before extension? still matches
    }

    #[test]
    fn stripped_name_removes_extension() {
        let pp = UnarchivePreprocessor::new();
        assert_eq!(pp.stripped_name("bin.tar.gz"), "bin");
        assert_eq!(pp.stripped_name("my-tools.tar.gz"), "my-tools");
        assert_eq!(pp.stripped_name("nested.dir.tar.gz"), "nested.dir");
    }

    #[test]
    fn trait_properties() {
        let pp = UnarchivePreprocessor::new();
        assert_eq!(pp.name(), "unarchive");
        assert_eq!(pp.transform_type(), TransformType::Opaque);
    }

    #[test]
    fn expand_extracts_tar_gz() {
        use flate2::write::GzEncoder;
        use flate2::Compression;

        let env = crate::testing::TempEnvironment::builder()
            .pack("tools")
            .file("placeholder", "")
            .done()
            .build();

        // Create a tar.gz archive programmatically
        let archive_path = env.dotfiles_root.join("tools/bin.tar.gz");
        let file = std::fs::File::create(&archive_path).unwrap();
        let enc = GzEncoder::new(file, Compression::default());
        let mut builder = tar::Builder::new(enc);

        // Add a file to the archive
        let content = b"#!/bin/sh\necho hello";
        let mut header = tar::Header::new_gnu();
        header.set_path("mytool").unwrap();
        header.set_size(content.len() as u64);
        header.set_mode(0o755);
        header.set_cksum();
        builder.append(&header, &content[..]).unwrap();

        // Add another file
        let content2 = b"#!/bin/sh\necho world";
        let mut header2 = tar::Header::new_gnu();
        header2.set_path("other-tool").unwrap();
        header2.set_size(content2.len() as u64);
        header2.set_mode(0o755);
        header2.set_cksum();
        builder.append(&header2, &content2[..]).unwrap();

        let enc = builder.into_inner().unwrap();
        enc.finish().unwrap();

        // Now expand it
        let pp = UnarchivePreprocessor::new();
        let result = pp.expand(&archive_path, env.fs.as_ref()).unwrap();

        assert_eq!(result.len(), 2);

        let names: Vec<String> = result
            .iter()
            .map(|f| f.relative_path.to_string_lossy().to_string())
            .collect();
        assert!(names.contains(&"mytool".to_string()));
        assert!(names.contains(&"other-tool".to_string()));

        let mytool = result
            .iter()
            .find(|f| f.relative_path.to_str() == Some("mytool"))
            .unwrap();
        assert_eq!(
            String::from_utf8_lossy(&mytool.content),
            "#!/bin/sh\necho hello"
        );
        assert!(!mytool.is_dir);
    }

    #[test]
    fn expand_tar_gz_with_directory() {
        use flate2::write::GzEncoder;
        use flate2::Compression;

        let env = crate::testing::TempEnvironment::builder()
            .pack("tools")
            .file("placeholder", "")
            .done()
            .build();

        let archive_path = env.dotfiles_root.join("tools/stuff.tar.gz");
        let file = std::fs::File::create(&archive_path).unwrap();
        let enc = GzEncoder::new(file, Compression::default());
        let mut builder = tar::Builder::new(enc);

        // Add a directory entry
        let mut dir_header = tar::Header::new_gnu();
        dir_header.set_path("subdir/").unwrap();
        dir_header.set_size(0);
        dir_header.set_entry_type(tar::EntryType::Directory);
        dir_header.set_mode(0o755);
        dir_header.set_cksum();
        builder.append(&dir_header, &[][..]).unwrap();

        // Add a file inside the directory
        let content = b"nested file";
        let mut file_header = tar::Header::new_gnu();
        file_header.set_path("subdir/nested.txt").unwrap();
        file_header.set_size(content.len() as u64);
        file_header.set_mode(0o644);
        file_header.set_cksum();
        builder.append(&file_header, &content[..]).unwrap();

        let enc = builder.into_inner().unwrap();
        enc.finish().unwrap();

        let pp = UnarchivePreprocessor::new();
        let result = pp.expand(&archive_path, env.fs.as_ref()).unwrap();

        assert_eq!(result.len(), 2);

        let dir_entry = result
            .iter()
            .find(|f| f.relative_path.to_str() == Some("subdir/"))
            .expect("should have directory entry");
        assert!(dir_entry.is_dir);

        let file_entry = result
            .iter()
            .find(|f| f.relative_path.to_str() == Some("subdir/nested.txt"))
            .expect("should have nested file");
        assert!(!file_entry.is_dir);
        assert_eq!(String::from_utf8_lossy(&file_entry.content), "nested file");
    }

    #[test]
    fn expand_empty_tar_gz() {
        use flate2::write::GzEncoder;
        use flate2::Compression;

        let env = crate::testing::TempEnvironment::builder()
            .pack("tools")
            .file("placeholder", "")
            .done()
            .build();

        let archive_path = env.dotfiles_root.join("tools/empty.tar.gz");
        let file = std::fs::File::create(&archive_path).unwrap();
        let enc = GzEncoder::new(file, Compression::default());
        let builder = tar::Builder::new(enc);
        let enc = builder.into_inner().unwrap();
        enc.finish().unwrap();

        let pp = UnarchivePreprocessor::new();
        let result = pp.expand(&archive_path, env.fs.as_ref()).unwrap();

        assert!(result.is_empty(), "empty archive should expand to no files");
    }

    #[test]
    fn expand_single_file_tar_gz() {
        use flate2::write::GzEncoder;
        use flate2::Compression;

        let env = crate::testing::TempEnvironment::builder()
            .pack("tools")
            .file("placeholder", "")
            .done()
            .build();

        let archive_path = env.dotfiles_root.join("tools/one.tar.gz");
        let file = std::fs::File::create(&archive_path).unwrap();
        let enc = GzEncoder::new(file, Compression::default());
        let mut builder = tar::Builder::new(enc);

        let content = b"single file";
        let mut header = tar::Header::new_gnu();
        header.set_path("only.txt").unwrap();
        header.set_size(content.len() as u64);
        header.set_mode(0o644);
        header.set_cksum();
        builder.append(&header, &content[..]).unwrap();

        let enc = builder.into_inner().unwrap();
        enc.finish().unwrap();

        let pp = UnarchivePreprocessor::new();
        let result = pp.expand(&archive_path, env.fs.as_ref()).unwrap();

        assert_eq!(result.len(), 1);
        assert_eq!(result[0].relative_path.to_str(), Some("only.txt"));
    }

    #[test]
    fn expand_corrupted_archive_returns_error() {
        let env = crate::testing::TempEnvironment::builder()
            .pack("tools")
            .file("bad.tar.gz", "this is not a valid gzip stream")
            .done()
            .build();

        let pp = UnarchivePreprocessor::new();
        let source = env.dotfiles_root.join("tools/bad.tar.gz");
        let err = pp.expand(&source, env.fs.as_ref());

        assert!(err.is_err(), "corrupted archive should produce an error");
    }

    #[test]
    fn expand_missing_file_returns_error() {
        let env = crate::testing::TempEnvironment::builder().build();

        let pp = UnarchivePreprocessor::new();
        let source = env.dotfiles_root.join("nonexistent.tar.gz");
        let err = pp.expand(&source, env.fs.as_ref());

        assert!(err.is_err(), "missing archive should produce an error");
    }
}
