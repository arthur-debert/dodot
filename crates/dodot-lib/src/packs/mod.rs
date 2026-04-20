//! Pack types, discovery, and orchestration.
//!
//! A pack is a directory of related dotfiles (e.g. `vim/`, `git/`, `zsh/`).
//! It is the unit of organisation, deployment, and removal.

pub mod orchestration;

use std::path::{Path, PathBuf};

use serde::Serialize;

use crate::fs::Fs;
use crate::handlers::HandlerConfig;
use crate::Result;

/// A dotfile pack — a directory of related configuration files.
#[derive(Debug, Clone, Serialize)]
pub struct Pack {
    /// Directory name (e.g. `"vim"`).
    pub name: String,

    /// Absolute path to the pack directory.
    pub path: PathBuf,

    /// Handler-relevant configuration for this pack (merged from
    /// app defaults + root config + pack config).
    pub config: HandlerConfig,
}

/// Discover all packs in the dotfiles root.
///
/// Scans for directories, skipping:
/// - Hidden directories (except `.config`)
/// - Directories matching ignore patterns
/// - Directories containing a `.dodotignore` file
///
/// Packs are returned sorted alphabetically by name.
pub fn discover_packs(
    fs: &dyn Fs,
    dotfiles_root: &Path,
    ignore_patterns: &[String],
) -> Result<Vec<Pack>> {
    let entries = fs.read_dir(dotfiles_root)?;
    let mut packs = Vec::new();

    for entry in entries {
        if !entry.is_dir {
            continue;
        }

        let name = &entry.name;

        // Skip hidden directories (except .config)
        if name.starts_with('.') && name != ".config" {
            continue;
        }

        // Skip ignored patterns
        if is_ignored(name, ignore_patterns) {
            continue;
        }

        // Skip packs with .dodotignore
        if fs.exists(&entry.path.join(".dodotignore")) {
            continue;
        }

        // Validate pack name (alphanumeric, underscore, dash)
        if !is_valid_pack_name(name) {
            continue;
        }

        packs.push(Pack {
            name: name.clone(),
            path: entry.path.clone(),
            config: HandlerConfig::default(),
        });
    }

    // Already sorted by read_dir (OsFs sorts), but ensure it
    packs.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(packs)
}

/// Discover pack directories that are ignored via a `.dodotignore` file.
///
/// Returns names (sorted alphabetically) of directories that would otherwise
/// be valid packs but carry a `.dodotignore` marker. Used by the `status`
/// command to surface these directories so users aren't surprised by their
/// absence from the main listing.
///
/// Applies the same filters as `discover_packs` (hidden dirs, ignore
/// patterns, valid names) so the two lists together cover every
/// pack-shaped directory the user might expect to see.
pub fn discover_ignored_packs(
    fs: &dyn Fs,
    dotfiles_root: &Path,
    ignore_patterns: &[String],
) -> Result<Vec<String>> {
    let entries = fs.read_dir(dotfiles_root)?;
    let mut names = Vec::new();

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

        if fs.exists(&entry.path.join(".dodotignore")) {
            names.push(name.clone());
        }
    }

    names.sort();
    Ok(names)
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
    fn discover_ignored_returns_dodotignore_dirs() {
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

        let ignored = discover_ignored_packs(env.fs.as_ref(), &env.dotfiles_root, &[]).unwrap();
        assert_eq!(ignored, vec!["disabled".to_string(), "old".to_string()]);
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
}
