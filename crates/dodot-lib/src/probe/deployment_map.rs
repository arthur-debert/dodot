//! The deployment map.
//!
//! The deployment map is a plain-text TSV under `<data_dir>/deployment-map.tsv`
//! with a two-line `#`-comment preamble followed by one TSV row per
//! datastore entry. An example file:
//!
//! ```text
//! # dodot deployment map v1
//! # columns: pack\thandler\tkind\tsource\tdatastore
//! vim\tshell\tsymlink\t/home/alice/dotfiles/vim/aliases.sh\t/home/alice/.local/share/dodot/packs/vim/shell/aliases.sh
//! git\tsymlink\tsymlink\t/home/alice/dotfiles/git/gitconfig\t/home/alice/.local/share/dodot/packs/git/symlink/gitconfig
//! ```
//!
//! The file is overwritten on every `dodot up` / `dodot down` so it
//! always matches the current datastore state. Its primary consumers
//! are:
//!
//! - `dodot refresh` (see `docs/proposals/magic.lex`), which needs the
//!   source→deployed mapping to decide which source templates to
//!   mtime-touch when a deployed file diverges.
//! - `dodot probe deployment-map`, the human-facing reader.
//!
//! # Sources of truth
//!
//! The map is derived *from the datastore alone* — we never re-run the
//! handlers to regenerate it. This keeps the writer trivial and keeps
//! the map honest: if the datastore has drifted from what the handlers
//! would produce today, the map reflects the datastore (which is what
//! the init script reads), not a hypothetical re-derivation.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::fs::Fs;
use crate::paths::Pather;
use crate::Result;

/// How a single datastore entry is materialised on disk.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DeploymentKind {
    /// The entry is a symlink in the datastore pointing at a source file
    /// inside the dotfiles repo. This covers `symlink`, `shell`, and
    /// `path` handlers.
    Symlink,
    /// The entry is a regular file written by dodot (a sentinel for
    /// `install` / `homebrew`, or a preprocessor's rendered output).
    File,
    /// The entry is a directory written by a preprocessor
    /// (e.g. expanded archive contents).
    Directory,
}

impl DeploymentKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Symlink => "symlink",
            Self::File => "file",
            Self::Directory => "directory",
        }
    }
}

/// One row in the deployment map.
///
/// `source` is the absolute path in the dotfiles repo (for symlink
/// entries — empty for file / directory entries, which are not backed by
/// a source file). `datastore` is the absolute path inside `<data_dir>`
/// where the entry lives.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeploymentMapEntry {
    pub pack: String,
    pub handler: String,
    pub kind: DeploymentKind,
    #[serde(default)]
    pub source: PathBuf,
    pub datastore: PathBuf,
}

/// Walk the datastore and collect one [`DeploymentMapEntry`] per
/// visible entry under `<data_dir>/packs/<pack>/<handler>/`.
///
/// The walk is non-recursive within a handler directory: dodot's
/// data layout is `packs/<pack>/<handler>/<entry>`, and any deeper
/// structure (preprocessor-expanded subtrees under `rendered` handler
/// directories, for instance) is represented as a single [`Directory`]
/// row. Consumers that care about subtree contents can walk from the
/// `datastore` path themselves.
///
/// [`Directory`]: DeploymentKind::Directory
pub fn collect_deployment_map(fs: &dyn Fs, paths: &dyn Pather) -> Result<Vec<DeploymentMapEntry>> {
    let packs_dir = paths.data_dir().join("packs");
    if !fs.is_dir(&packs_dir) {
        return Ok(Vec::new());
    }

    let mut entries = Vec::new();

    let mut pack_entries = fs.read_dir(&packs_dir)?;
    pack_entries.sort_by(|a, b| a.name.cmp(&b.name));

    for pack_dir in pack_entries {
        if !pack_dir.is_dir {
            continue;
        }
        let pack_name = pack_dir.name.clone();

        let mut handler_dirs = fs.read_dir(&pack_dir.path)?;
        handler_dirs.sort_by(|a, b| a.name.cmp(&b.name));

        for handler_dir in handler_dirs {
            if !handler_dir.is_dir {
                continue;
            }
            let handler_name = handler_dir.name.clone();

            let mut items = fs.read_dir(&handler_dir.path)?;
            items.sort_by(|a, b| a.name.cmp(&b.name));

            for item in items {
                let kind = classify_entry(fs, &item);
                let source = if kind == DeploymentKind::Symlink {
                    // readlink may fail on a broken symlink — we still
                    // want to record the entry so the user can see it.
                    fs.readlink(&item.path).unwrap_or_default()
                } else {
                    PathBuf::new()
                };

                entries.push(DeploymentMapEntry {
                    pack: pack_name.clone(),
                    handler: handler_name.clone(),
                    kind,
                    source,
                    datastore: item.path.clone(),
                });
            }
        }
    }

    Ok(entries)
}

fn classify_entry(fs: &dyn Fs, entry: &crate::fs::DirEntry) -> DeploymentKind {
    if entry.is_symlink {
        DeploymentKind::Symlink
    } else if entry.is_dir {
        DeploymentKind::Directory
    } else if entry.is_file {
        DeploymentKind::File
    } else {
        // Fallback: query the fs directly. Shouldn't be reachable in
        // practice since read_dir populates all three flags.
        match fs.lstat(&entry.path) {
            Ok(m) if m.is_symlink => DeploymentKind::Symlink,
            Ok(m) if m.is_dir => DeploymentKind::Directory,
            _ => DeploymentKind::File,
        }
    }
}

/// Format the deployment map as TSV.
///
/// The output is deterministic (entries are emitted in the order
/// returned by [`collect_deployment_map`], which sorts by pack, then
/// handler, then entry name).
pub fn format_deployment_map(entries: &[DeploymentMapEntry]) -> String {
    let mut out = String::new();
    out.push_str("# dodot deployment map v1\n");
    out.push_str("# columns: pack\thandler\tkind\tsource\tdatastore\n");
    for e in entries {
        out.push_str(&format_row(e));
        out.push('\n');
    }
    out
}

fn format_row(e: &DeploymentMapEntry) -> String {
    format!(
        "{}\t{}\t{}\t{}\t{}",
        e.pack,
        e.handler,
        e.kind.as_str(),
        e.source.display(),
        e.datastore.display(),
    )
}

/// Collect, format, and write the deployment map to
/// `<data_dir>/deployment-map.tsv`. Returns the written path.
pub fn write_deployment_map(fs: &dyn Fs, paths: &dyn Pather) -> Result<PathBuf> {
    let entries = collect_deployment_map(fs, paths)?;
    let content = format_deployment_map(&entries);
    let map_path = paths.deployment_map_path();
    fs.mkdir_all(paths.data_dir())?;
    fs.write_file(&map_path, content.as_bytes())?;
    Ok(map_path)
}

/// Read and parse a deployment-map TSV file.
///
/// Blank lines and `#`-prefixed comments are ignored. Rows with the
/// wrong column count are skipped silently — the map is best-effort
/// and a truncated file should not crash the reader.
pub fn read_deployment_map(fs: &dyn Fs, path: &Path) -> Result<Vec<DeploymentMapEntry>> {
    if !fs.exists(path) {
        return Ok(Vec::new());
    }
    let content = fs.read_to_string(path)?;
    Ok(parse_deployment_map(&content))
}

fn parse_deployment_map(content: &str) -> Vec<DeploymentMapEntry> {
    content.lines().filter_map(parse_row).collect()
}

fn parse_row(line: &str) -> Option<DeploymentMapEntry> {
    let trimmed = line.trim_end_matches('\r');
    if trimmed.is_empty() || trimmed.starts_with('#') {
        return None;
    }
    let mut parts = trimmed.splitn(5, '\t');
    let pack = parts.next()?;
    let handler = parts.next()?;
    let kind_str = parts.next()?;
    let source = parts.next()?;
    let datastore = parts.next()?;
    let kind = match kind_str {
        "symlink" => DeploymentKind::Symlink,
        "file" => DeploymentKind::File,
        "directory" => DeploymentKind::Directory,
        _ => return None,
    };
    Some(DeploymentMapEntry {
        pack: pack.to_string(),
        handler: handler.to_string(),
        kind,
        source: PathBuf::from(source),
        datastore: PathBuf::from(datastore),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::datastore::{CommandOutput, CommandRunner, DataStore, FilesystemDataStore};
    use crate::testing::TempEnvironment;
    use std::sync::Arc;

    struct NoopRunner;
    impl CommandRunner for NoopRunner {
        fn run(&self, _: &str, _: &[String]) -> Result<CommandOutput> {
            Ok(CommandOutput {
                exit_code: 0,
                stdout: String::new(),
                stderr: String::new(),
            })
        }
    }

    fn make_datastore(env: &TempEnvironment) -> FilesystemDataStore {
        FilesystemDataStore::new(env.fs.clone(), env.paths.clone(), Arc::new(NoopRunner))
    }

    #[test]
    fn empty_datastore_yields_empty_map() {
        let env = TempEnvironment::builder().build();
        let entries = collect_deployment_map(env.fs.as_ref(), env.paths.as_ref()).unwrap();
        assert!(entries.is_empty());
    }

    #[test]
    fn symlink_entries_capture_source_and_datastore() {
        let env = TempEnvironment::builder()
            .pack("vim")
            .file("aliases.sh", "alias vi=vim")
            .done()
            .build();

        let ds = make_datastore(&env);
        let source = env.dotfiles_root.join("vim/aliases.sh");
        ds.create_data_link("vim", "shell", &source).unwrap();

        let entries = collect_deployment_map(env.fs.as_ref(), env.paths.as_ref()).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].pack, "vim");
        assert_eq!(entries[0].handler, "shell");
        assert_eq!(entries[0].kind, DeploymentKind::Symlink);
        assert_eq!(entries[0].source, source);
        assert_eq!(
            entries[0].datastore,
            env.paths
                .handler_data_dir("vim", "shell")
                .join("aliases.sh")
        );
    }

    #[test]
    fn entries_sort_by_pack_then_handler_then_name() {
        let env = TempEnvironment::builder()
            .pack("vim")
            .file("aliases.sh", "")
            .file("bin/tool", "#!/bin/sh")
            .done()
            .pack("git")
            .file("gitconfig", "")
            .done()
            .build();

        let ds = make_datastore(&env);
        ds.create_data_link("vim", "shell", &env.dotfiles_root.join("vim/aliases.sh"))
            .unwrap();
        ds.create_data_link("vim", "path", &env.dotfiles_root.join("vim/bin"))
            .unwrap();
        ds.create_data_link("git", "symlink", &env.dotfiles_root.join("git/gitconfig"))
            .unwrap();

        let entries = collect_deployment_map(env.fs.as_ref(), env.paths.as_ref()).unwrap();

        let keys: Vec<(String, String)> = entries
            .iter()
            .map(|e| (e.pack.clone(), e.handler.clone()))
            .collect();
        // git/symlink comes before vim/{path,shell} (sorted by pack),
        // and vim/path comes before vim/shell (sorted by handler).
        assert_eq!(
            keys,
            vec![
                ("git".into(), "symlink".into()),
                ("vim".into(), "path".into()),
                ("vim".into(), "shell".into()),
            ]
        );
    }

    #[test]
    fn sentinel_file_classified_as_file_with_no_source() {
        let env = TempEnvironment::builder().build();

        // Simulate an install handler sentinel: a plain file in the
        // handler dir, not a symlink.
        let handler_dir = env.paths.handler_data_dir("nvim", "install");
        env.fs.mkdir_all(&handler_dir).unwrap();
        env.fs
            .write_file(
                &handler_dir.join("install.sh-abc123"),
                b"completed|2026-01-01",
            )
            .unwrap();

        let entries = collect_deployment_map(env.fs.as_ref(), env.paths.as_ref()).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].kind, DeploymentKind::File);
        assert!(
            entries[0].source.as_os_str().is_empty(),
            "sentinels have no source file; got {:?}",
            entries[0].source
        );
    }

    #[test]
    fn broken_symlink_still_recorded_with_empty_source() {
        let env = TempEnvironment::builder().build();

        let handler_dir = env.paths.handler_data_dir("nvim", "shell");
        env.fs.mkdir_all(&handler_dir).unwrap();
        // Point at a path that doesn't exist. readlink still works on
        // broken symlinks, so in this case source is captured. To test
        // the *unreadable* branch we'd need to simulate a failure;
        // broken-but-readable is the realistic case.
        let broken_target = env.dotfiles_root.join("nvim/gone.sh");
        env.fs
            .symlink(&broken_target, &handler_dir.join("gone.sh"))
            .unwrap();

        let entries = collect_deployment_map(env.fs.as_ref(), env.paths.as_ref()).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].kind, DeploymentKind::Symlink);
        assert_eq!(entries[0].source, broken_target);
    }

    #[test]
    fn write_and_reread_roundtrip() {
        let env = TempEnvironment::builder()
            .pack("vim")
            .file("aliases.sh", "")
            .done()
            .build();

        let ds = make_datastore(&env);
        let source = env.dotfiles_root.join("vim/aliases.sh");
        ds.create_data_link("vim", "shell", &source).unwrap();

        let path = write_deployment_map(env.fs.as_ref(), env.paths.as_ref()).unwrap();
        assert_eq!(path, env.paths.deployment_map_path());
        env.assert_exists(&path);

        let content = env.fs.read_to_string(&path).unwrap();
        assert!(content.starts_with("# dodot deployment map v1"));
        assert!(content.contains("vim\tshell\tsymlink\t"));

        let parsed = read_deployment_map(env.fs.as_ref(), &path).unwrap();
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].pack, "vim");
        assert_eq!(parsed[0].source, source);
    }

    #[test]
    fn read_returns_empty_when_file_missing() {
        let env = TempEnvironment::builder().build();
        let parsed =
            read_deployment_map(env.fs.as_ref(), &env.paths.deployment_map_path()).unwrap();
        assert!(parsed.is_empty());
    }

    #[test]
    fn parser_ignores_comments_and_blank_lines() {
        let content = "\
# a comment
\n\
vim\tshell\tsymlink\t/src/a\t/ds/a
# another

git\tsymlink\tsymlink\t/src/b\t/ds/b
";
        let parsed = parse_deployment_map(content);
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].pack, "vim");
        assert_eq!(parsed[1].pack, "git");
    }

    #[test]
    fn parser_skips_malformed_rows() {
        // Too few columns and an unknown kind should both be dropped,
        // not crash.
        let content = "\
only-two-cols\tvalue
vim\tshell\tweird-kind\t/a\t/b
vim\tshell\tsymlink\t/a\t/b
";
        let parsed = parse_deployment_map(content);
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].kind, DeploymentKind::Symlink);
    }

    #[test]
    fn format_has_header_and_one_row_per_entry() {
        let entries = vec![
            DeploymentMapEntry {
                pack: "vim".into(),
                handler: "shell".into(),
                kind: DeploymentKind::Symlink,
                source: PathBuf::from("/src/a"),
                datastore: PathBuf::from("/ds/a"),
            },
            DeploymentMapEntry {
                pack: "vim".into(),
                handler: "install".into(),
                kind: DeploymentKind::File,
                source: PathBuf::new(),
                datastore: PathBuf::from("/ds/sentinel"),
            },
        ];
        let s = format_deployment_map(&entries);
        let lines: Vec<&str> = s.lines().collect();
        assert_eq!(lines.len(), 4); // 2 comments + 2 data rows
        assert!(lines[0].starts_with('#'));
        assert!(lines[1].starts_with('#'));
        assert_eq!(lines[2], "vim\tshell\tsymlink\t/src/a\t/ds/a");
        assert_eq!(lines[3], "vim\tinstall\tfile\t\t/ds/sentinel");
    }

    #[test]
    fn empty_input_produces_header_only() {
        let s = format_deployment_map(&[]);
        let lines: Vec<&str> = s.lines().collect();
        assert_eq!(lines.len(), 2);
        assert!(lines[0].starts_with("# dodot"));
        assert!(lines[1].starts_with("# columns"));
    }

    #[test]
    fn paths_with_tabs_would_break_tsv_but_are_not_produced_by_dodot() {
        // Documenting an invariant rather than testing a path: dodot
        // never creates paths containing literal tab characters, so we
        // don't escape them in the TSV. A pack dir named "foo\tbar"
        // would produce a malformed row — but dodot's pack discovery
        // rejects such names upstream (ignore list + XDG conventions).
        //
        // This test just locks in the current format so a future change
        // that wants tab-containing paths has to explicitly revisit
        // escaping.
        let entry = DeploymentMapEntry {
            pack: "p".into(),
            handler: "h".into(),
            kind: DeploymentKind::Symlink,
            source: PathBuf::from("/a"),
            datastore: PathBuf::from("/b"),
        };
        let row = format_row(&entry);
        assert_eq!(row.matches('\t').count(), 4);
    }
}
