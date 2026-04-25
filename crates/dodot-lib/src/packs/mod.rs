//! Pack types, discovery, and orchestration.
//!
//! A pack is a directory of related dotfiles (e.g. `vim/`, `git/`, `zsh/`).
//! It is the unit of organisation, deployment, and removal.
//!
//! # Pack ordering
//!
//! Packs are processed in lexicographic order of their on-disk directory
//! names. That order determines every cross-pack effect: shell init
//! source order, `$PATH` entry order, install/homebrew execution order.
//! See [`docs/reference/handlers.lex`](../../../../docs/reference/handlers.lex)
//! "Cross-Pack Ordering" for the user-facing contract.
//!
//! For the small minority of packs where ordering actually matters,
//! dodot recognises a numeric prefix on the directory name as ordering
//! metadata: `010-brew`, `020_zsh`, `100-starship`. The grammar is
//! `^(\d+)[-_](.+)$`. The portion after the separator is the pack's
//! *display name* — what every user-facing surface shows
//! (`dodot status`, `dodot list`, error messages, shell-init comments,
//! log lines). The full directory name is the pack's *sort key* and
//! the identity used for every internal surface (datastore subtree,
//! sentinel keys, paths).
//!
//! Three classes of collision are rejected at scan time, with both
//! offending paths reported:
//!
//! - **Logical-name collision** — `nvim` and `010-nvim` both exist;
//!   the display name `nvim` is ambiguous.
//! - **Multi-prefix collision** — `010-nvim` and `020-nvim` both exist;
//!   the display name `nvim` resolves to two packs.
//! - **Empty stem** — `010-` or `010_` with no name after the separator.
//!
//! The non-collision case where two packs share a prefix but differ
//! on the stem (`010-brew` and `010-zsh`) is permitted; lex order on
//! the stem decides between them. The 10/20/30 gap convention is
//! documented but not enforced.
//!
//! ## Why no formal dependency graph?
//!
//! A `priority = 30` field, a `requires:` / `after:` declaration, or a
//! phase-bucket directory layout were all considered and explicitly
//! rejected for this iteration (see `docs/proposals/pack-ordering.lex`
//! §6). The systemd lesson — that conflating ordering with dependency
//! is the documented failure mode of every system that has tried — is
//! the reason. The prefix is purely an ordering primitive; it says
//! "A applies before B", not "A is required for B to make sense".
//! A pack with a missing dependency is the user's problem, not the
//! framework's.

pub mod orchestration;

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::fs::Fs;
use crate::handlers::HandlerConfig;
use crate::{DodotError, Result};

/// A dotfile pack — a directory of related configuration files.
#[derive(Debug, Clone, Serialize)]
pub struct Pack {
    /// On-disk directory name (e.g. `"vim"`, `"010-nvim"`). This is the
    /// sort key that drives cross-pack ordering, and the identity used
    /// by every internal surface (datastore subtree, sentinel keys,
    /// path resolution).
    pub name: String,

    /// User-facing pack name. For unprefixed packs this equals
    /// [`name`](Self::name); for packs whose directory matches the
    /// `^(\d+)[-_](.+)$` ordering grammar, this is the portion after
    /// the separator (e.g. `010-nvim` → `nvim`). Used by every
    /// user-facing surface: `dodot status`, `dodot list`, error
    /// messages, generated shell-init comments, log lines, CLI
    /// argument resolution.
    pub display_name: String,

    /// Absolute path to the pack directory.
    pub path: PathBuf,

    /// Handler-relevant configuration for this pack (merged from
    /// app defaults + root config + pack config).
    pub config: HandlerConfig,
}

impl Pack {
    /// Construct a `Pack` from its on-disk directory name. Derives
    /// [`display_name`](Self::display_name) by stripping a recognised
    /// numeric prefix (`010-foo` → `foo`); for names without a prefix,
    /// `display_name == name`.
    ///
    /// Pack name validation (alphanumerics, `_`, `-`, `.`) is the
    /// caller's responsibility — typically [`scan_packs`].
    pub fn new(name: String, path: PathBuf, config: HandlerConfig) -> Self {
        let display_name = match parse_prefix(&name) {
            Ok(Some(stem)) => stem.to_string(),
            // Empty-stem (`010-`) is rejected upstream by scan_packs;
            // if a caller bypasses that, fall back to the raw name
            // rather than silently producing an empty display name.
            Ok(None) | Err(_) => name.clone(),
        };
        Pack {
            name,
            display_name,
            path,
            config,
        }
    }
}

/// Compute the user-facing pack name for an on-disk directory name.
/// Strips a recognised numeric prefix (`010-foo` → `foo`); for
/// directories without a prefix, returns the full name.
///
/// For empty-stem inputs (`010-`, `010_`), returns the full name —
/// scan-time validation rejects those before they reach this function,
/// but the fallback keeps the helper total.
pub fn display_name_for(dir_name: &str) -> &str {
    match parse_prefix(dir_name) {
        Ok(Some(stem)) => stem,
        _ => dir_name,
    }
}

/// Outcome of parsing a pack directory name against the ordering
/// prefix grammar `^(\d+)[-_](.+)$`.
///
/// - `Ok(Some(stem))` — the name has a recognised prefix; the returned
///   `&str` is the user-facing portion (e.g. `"010-nvim"` → `"nvim"`).
/// - `Ok(None)` — the name does not match the grammar; treat it as
///   unprefixed (display name equals the directory name).
/// - `Err(())` — the name looks like a prefix (`010-` / `010_`) but
///   nothing follows the separator. A scan-time error: a pack must
///   have a name.
fn parse_prefix(name: &str) -> std::result::Result<Option<&str>, ()> {
    let bytes = name.as_bytes();
    let digits_len = bytes.iter().take_while(|b| b.is_ascii_digit()).count();
    if digits_len == 0 {
        return Ok(None);
    }
    match bytes.get(digits_len) {
        Some(b'-') | Some(b'_') => {}
        _ => return Ok(None),
    }
    let stem = &name[digits_len + 1..];
    if stem.is_empty() {
        return Err(());
    }
    Ok(Some(stem))
}

/// Detect collisions between recognised display names. Reports the
/// three classes covered in the module docs; returns the first
/// collision encountered (deterministic, since `packs` is sorted by
/// directory name on entry).
fn detect_display_collisions(packs: &[Pack]) -> Result<()> {
    let mut by_display: HashMap<&str, Vec<&Pack>> = HashMap::new();
    for pack in packs {
        by_display.entry(&pack.display_name).or_default().push(pack);
    }

    // Walk in sort order so the error is deterministic across runs.
    for pack in packs {
        if let Some(group) = by_display.get(pack.display_name.as_str()) {
            if group.len() > 1 {
                let paths: Vec<PathBuf> = group.iter().map(|p| p.path.clone()).collect();
                return Err(DodotError::PackOrderingCollision {
                    display_name: pack.display_name.clone(),
                    paths,
                });
            }
        }
    }
    Ok(())
}

/// Result of scanning the dotfiles root: active packs + names of
/// pack-shaped directories skipped via `.dodotignore`.
pub struct DiscoveredPacks {
    pub packs: Vec<Pack>,
    pub ignored: Vec<String>,
}

/// Scan the dotfiles root once, partitioning pack-shaped directories into
/// active packs and those skipped via `.dodotignore`.
///
/// Directories filtered out entirely (hidden, matching `ignore_patterns`,
/// invalid names) appear in neither list — they aren't pack-shaped.
///
/// Both lists are returned sorted lexicographically by on-disk
/// directory name. That sort order is the contract that drives every
/// cross-pack effect (shell init source order, `$PATH` order,
/// install/homebrew execution order); see the module docs.
///
/// Errors:
///
/// - [`DodotError::PackInvalid`] for an empty-stem prefix
///   (e.g. `010-` or `010_`) — a pack must have a name.
/// - [`DodotError::PackOrderingCollision`] when two or more packs
///   resolve to the same display name (`nvim` + `010-nvim`, or
///   `010-nvim` + `020-nvim`).
pub fn scan_packs(
    fs: &dyn Fs,
    dotfiles_root: &Path,
    ignore_patterns: &[String],
) -> Result<DiscoveredPacks> {
    let entries = fs.read_dir(dotfiles_root)?;
    let mut packs = Vec::new();
    let mut ignored = Vec::new();

    for entry in entries {
        if !entry.is_dir {
            continue;
        }

        let name = &entry.name;

        if name.starts_with('.') && name != ".config" {
            continue;
        }

        if is_ignored(name, ignore_patterns) {
            continue;
        }

        if !is_valid_pack_name(name) {
            continue;
        }

        // Reject pack directories that look like an ordering prefix
        // but have no name after the separator (e.g. `010-`, `010_`).
        // Done here so the error carries the offending path.
        if parse_prefix(name).is_err() {
            return Err(DodotError::PackInvalid {
                name: name.clone(),
                reason:
                    "directory looks like an ordering prefix but has no name after the separator"
                        .into(),
            });
        }

        if fs.exists(&entry.path.join(".dodotignore")) {
            ignored.push(name.clone());
            continue;
        }

        packs.push(Pack::new(
            name.clone(),
            entry.path.clone(),
            HandlerConfig::default(),
        ));
    }

    packs.sort_by(|a, b| a.name.cmp(&b.name));
    ignored.sort();

    detect_display_collisions(&packs)?;

    Ok(DiscoveredPacks { packs, ignored })
}

/// Discover all active packs in the dotfiles root.
///
/// Skips hidden directories (except `.config`), directories matching
/// ignore patterns, directories carrying a `.dodotignore` file, and
/// directories with invalid names. Returns sorted alphabetically.
///
/// Prefer [`scan_packs`] when you also need the ignored list —
/// this is a convenience wrapper over the same single-pass scan.
pub fn discover_packs(
    fs: &dyn Fs,
    dotfiles_root: &Path,
    ignore_patterns: &[String],
) -> Result<Vec<Pack>> {
    Ok(scan_packs(fs, dotfiles_root, ignore_patterns)?.packs)
}

/// Check if a name matches any ignore pattern.
fn is_ignored(name: &str, patterns: &[String]) -> bool {
    for pattern in patterns {
        if let Ok(glob) = glob::Pattern::new(pattern) {
            if glob.matches(name) {
                return true;
            }
        }
        if name == pattern {
            return true;
        }
    }
    false
}

/// Validate that a pack name contains only safe characters.
fn is_valid_pack_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '-' || c == '.')
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TempEnvironment;

    #[test]
    fn discover_finds_pack_directories() {
        let env = TempEnvironment::builder()
            .pack("git")
            .file("gitconfig", "x")
            .done()
            .pack("vim")
            .file("vimrc", "x")
            .done()
            .pack("zsh")
            .file("zshrc", "x")
            .done()
            .build();

        let packs = discover_packs(env.fs.as_ref(), &env.dotfiles_root, &[]).unwrap();
        let names: Vec<&str> = packs.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, vec!["git", "vim", "zsh"]);
    }

    #[test]
    fn discover_skips_hidden_dirs() {
        let env = TempEnvironment::builder()
            .pack("vim")
            .file("vimrc", "x")
            .done()
            .build();

        // Manually create a hidden dir
        env.fs
            .mkdir_all(&env.dotfiles_root.join(".hidden-pack"))
            .unwrap();
        env.fs
            .write_file(&env.dotfiles_root.join(".hidden-pack/file"), b"x")
            .unwrap();

        let packs = discover_packs(env.fs.as_ref(), &env.dotfiles_root, &[]).unwrap();
        let names: Vec<&str> = packs.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, vec!["vim"]);
    }

    #[test]
    fn discover_skips_ignored_patterns() {
        let env = TempEnvironment::builder()
            .pack("vim")
            .file("vimrc", "x")
            .done()
            .pack("scratch")
            .file("notes", "x")
            .done()
            .build();

        let packs =
            discover_packs(env.fs.as_ref(), &env.dotfiles_root, &["scratch".into()]).unwrap();
        let names: Vec<&str> = packs.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, vec!["vim"]);
    }

    #[test]
    fn discover_skips_dodotignore() {
        let env = TempEnvironment::builder()
            .pack("vim")
            .file("vimrc", "x")
            .done()
            .pack("disabled")
            .file("stuff", "x")
            .ignored()
            .done()
            .build();

        let packs = discover_packs(env.fs.as_ref(), &env.dotfiles_root, &[]).unwrap();
        let names: Vec<&str> = packs.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, vec!["vim"]);
    }

    #[test]
    fn scan_partitions_active_and_ignored_packs() {
        let env = TempEnvironment::builder()
            .pack("vim")
            .file("vimrc", "x")
            .done()
            .pack("disabled")
            .file("stuff", "x")
            .ignored()
            .done()
            .pack("old")
            .file("thing", "x")
            .ignored()
            .done()
            .build();

        let result = scan_packs(env.fs.as_ref(), &env.dotfiles_root, &[]).unwrap();
        let names: Vec<&str> = result.packs.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, vec!["vim"]);
        assert_eq!(
            result.ignored,
            vec!["disabled".to_string(), "old".to_string()]
        );
    }

    #[test]
    fn discover_sorts_alphabetically() {
        let env = TempEnvironment::builder()
            .pack("zsh")
            .file("z", "x")
            .done()
            .pack("alacritty")
            .file("a", "x")
            .done()
            .pack("git")
            .file("g", "x")
            .done()
            .build();

        let packs = discover_packs(env.fs.as_ref(), &env.dotfiles_root, &[]).unwrap();
        let names: Vec<&str> = packs.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(names, vec!["alacritty", "git", "zsh"]);
    }

    #[test]
    fn discover_skips_files_at_root() {
        let env = TempEnvironment::builder()
            .pack("vim")
            .file("vimrc", "x")
            .done()
            .build();

        // Create a file at dotfiles root (not a pack)
        env.fs
            .write_file(&env.dotfiles_root.join("README.md"), b"# my dotfiles")
            .unwrap();

        let packs = discover_packs(env.fs.as_ref(), &env.dotfiles_root, &[]).unwrap();
        assert_eq!(packs.len(), 1);
        assert_eq!(packs[0].name, "vim");
    }

    #[test]
    fn valid_pack_names() {
        assert!(is_valid_pack_name("vim"));
        assert!(is_valid_pack_name("my-pack"));
        assert!(is_valid_pack_name("pack_name"));
        assert!(is_valid_pack_name("nvim.bak"));
        assert!(!is_valid_pack_name(""));
        assert!(!is_valid_pack_name("has space"));
        assert!(!is_valid_pack_name("path/traversal"));
    }

    // ── Pack ordering: prefix grammar ──────────────────────────────

    #[test]
    fn parse_prefix_recognises_dash_separator() {
        assert_eq!(parse_prefix("010-nvim"), Ok(Some("nvim")));
        assert_eq!(parse_prefix("1-a"), Ok(Some("a")));
        assert_eq!(parse_prefix("100-fzf-tab"), Ok(Some("fzf-tab")));
    }

    #[test]
    fn parse_prefix_recognises_underscore_separator() {
        assert_eq!(parse_prefix("020_zsh"), Ok(Some("zsh")));
        assert_eq!(parse_prefix("99_late"), Ok(Some("late")));
    }

    #[test]
    fn parse_prefix_passes_through_unprefixed_names() {
        assert_eq!(parse_prefix("vim"), Ok(None));
        assert_eq!(parse_prefix("my-pack"), Ok(None));
        // Digits without a separator → not a prefix.
        assert_eq!(parse_prefix("vim2"), Ok(None));
        // Non-digit prefix → not a prefix.
        assert_eq!(parse_prefix("a01-foo"), Ok(None));
        // Separator at position 0 (no digits) → not a prefix.
        assert_eq!(parse_prefix("-foo"), Ok(None));
        assert_eq!(parse_prefix("_foo"), Ok(None));
    }

    #[test]
    fn parse_prefix_rejects_empty_stem() {
        assert_eq!(parse_prefix("010-"), Err(()));
        assert_eq!(parse_prefix("010_"), Err(()));
        assert_eq!(parse_prefix("1-"), Err(()));
    }

    #[test]
    fn pack_new_strips_prefix_for_display_name() {
        let p = Pack::new(
            "010-nvim".into(),
            PathBuf::from("/x/010-nvim"),
            HandlerConfig::default(),
        );
        assert_eq!(p.name, "010-nvim");
        assert_eq!(p.display_name, "nvim");
    }

    #[test]
    fn pack_new_keeps_unprefixed_name_for_display_name() {
        let p = Pack::new(
            "vim".into(),
            PathBuf::from("/x/vim"),
            HandlerConfig::default(),
        );
        assert_eq!(p.name, "vim");
        assert_eq!(p.display_name, "vim");
    }

    #[test]
    fn display_name_for_helper_handles_both_forms() {
        assert_eq!(display_name_for("010-nvim"), "nvim");
        assert_eq!(display_name_for("020_zsh"), "zsh");
        assert_eq!(display_name_for("vim"), "vim");
        // Empty-stem inputs fall back to the raw name (callers should
        // have rejected them at scan time).
        assert_eq!(display_name_for("010-"), "010-");
    }

    #[test]
    fn scan_sorts_prefixed_packs_numerically_via_lex_when_zero_padded() {
        let env = TempEnvironment::builder()
            .pack("100-zsh")
            .file("zshrc", "x")
            .done()
            .pack("010-brew")
            .file("Brewfile", "x")
            .done()
            .pack("020-git")
            .file("gitconfig", "x")
            .done()
            .build();

        let packs = discover_packs(env.fs.as_ref(), &env.dotfiles_root, &[]).unwrap();
        let dirs: Vec<&str> = packs.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(dirs, vec!["010-brew", "020-git", "100-zsh"]);
        let displays: Vec<&str> = packs.iter().map(|p| p.display_name.as_str()).collect();
        assert_eq!(displays, vec!["brew", "git", "zsh"]);
    }

    #[test]
    fn scan_interleaves_prefixed_and_unprefixed_via_lex() {
        // `010-brew` < `020-zsh` < `nvim` < `starship`.
        let env = TempEnvironment::builder()
            .pack("nvim")
            .file("init.lua", "x")
            .done()
            .pack("starship")
            .file("starship.toml", "x")
            .done()
            .pack("010-brew")
            .file("Brewfile", "x")
            .done()
            .pack("020-zsh")
            .file("zshrc", "x")
            .done()
            .build();

        let packs = discover_packs(env.fs.as_ref(), &env.dotfiles_root, &[]).unwrap();
        let dirs: Vec<&str> = packs.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(dirs, vec!["010-brew", "020-zsh", "nvim", "starship"]);
    }

    #[test]
    fn scan_rejects_logical_name_collision_between_prefixed_and_unprefixed() {
        let env = TempEnvironment::builder()
            .pack("nvim")
            .file("init.lua", "x")
            .done()
            .pack("010-nvim")
            .file("init.lua", "x")
            .done()
            .build();

        let err = discover_packs(env.fs.as_ref(), &env.dotfiles_root, &[]).unwrap_err();
        match err {
            DodotError::PackOrderingCollision {
                display_name,
                paths,
            } => {
                assert_eq!(display_name, "nvim");
                assert_eq!(paths.len(), 2);
                let path_strs: Vec<String> =
                    paths.iter().map(|p| p.display().to_string()).collect();
                assert!(path_strs.iter().any(|s| s.ends_with("nvim")));
                assert!(path_strs.iter().any(|s| s.ends_with("010-nvim")));
            }
            other => panic!("expected PackOrderingCollision, got: {other:?}"),
        }
    }

    #[test]
    fn scan_rejects_multi_prefix_collision() {
        let env = TempEnvironment::builder()
            .pack("010-nvim")
            .file("init.lua", "x")
            .done()
            .pack("020-nvim")
            .file("init.lua", "x")
            .done()
            .build();

        let err = discover_packs(env.fs.as_ref(), &env.dotfiles_root, &[]).unwrap_err();
        assert!(matches!(
            err,
            DodotError::PackOrderingCollision { ref display_name, .. } if display_name == "nvim"
        ));
    }

    #[test]
    fn scan_allows_same_prefix_with_different_stems() {
        // `010-brew` and `010-zsh` both legal — display names differ
        // (`brew`, `zsh`), and lex order on the stem decides between
        // them.
        let env = TempEnvironment::builder()
            .pack("010-brew")
            .file("Brewfile", "x")
            .done()
            .pack("010-zsh")
            .file("zshrc", "x")
            .done()
            .build();

        let packs = discover_packs(env.fs.as_ref(), &env.dotfiles_root, &[]).unwrap();
        let dirs: Vec<&str> = packs.iter().map(|p| p.name.as_str()).collect();
        assert_eq!(dirs, vec!["010-brew", "010-zsh"]);
        let displays: Vec<&str> = packs.iter().map(|p| p.display_name.as_str()).collect();
        assert_eq!(displays, vec!["brew", "zsh"]);
    }

    #[test]
    fn scan_rejects_empty_stem_directory() {
        let env = TempEnvironment::builder()
            .pack("010-")
            .file("placeholder", "x")
            .done()
            .build();

        let err = discover_packs(env.fs.as_ref(), &env.dotfiles_root, &[]).unwrap_err();
        match err {
            DodotError::PackInvalid { name, reason } => {
                assert_eq!(name, "010-");
                assert!(reason.contains("ordering prefix"));
            }
            other => panic!("expected PackInvalid, got: {other:?}"),
        }
    }
}
