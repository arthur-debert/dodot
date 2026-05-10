//! Tests for the symlink handler.
//!
//! Shared fixtures (`test_pather`, `default_config`) live here so the
//! per-tier sub-modules can `use super::{...}` without re-defining each
//! one. Small sections — default rule, pack-prefix interaction,
//! protected paths, force_home matching — stay inline. The big topical
//! suites live in sibling files: cascade priority resolution and the
//! integration tests covering routing conflicts, custom targets,
//! wholesale-vs-per-file behaviour, and `_lib/` warnings.

#![allow(unused_imports)]

mod cascade;
mod integration;

use std::collections::HashMap;
use std::path::PathBuf;

use super::*;
use crate::handlers::{Handler, HandlerConfig, HandlerScope, HANDLER_SYMLINK};
use crate::operations::HandlerIntent;
use crate::packs::Pack;
use crate::paths::{Pather, XdgPather};
use crate::rules::RuleMatch;

pub(super) fn test_pather() -> XdgPather {
    // Pin app_support_dir explicitly so resolver tests behave
    // identically on Linux and macOS hosts. Production builds let
    // the platform default kick in; unit tests need determinism.
    XdgPather::builder()
        .home("/home/alice")
        .dotfiles_root("/home/alice/dotfiles")
        .xdg_config_home("/home/alice/.config")
        .app_support_dir("/home/alice/Library/Application Support")
        .build()
        .unwrap()
}

pub(super) fn default_config() -> HandlerConfig {
    HandlerConfig {
        force_home: vec![
            "ssh".into(),
            "bashrc".into(),
            "zshrc".into(),
            "profile".into(),
        ],
        protected_paths: vec![
            ".ssh/id_rsa".into(),
            ".ssh/id_ed25519".into(),
            ".gnupg".into(),
        ],
        targets: std::collections::HashMap::new(),
        ..HandlerConfig::default()
    }
}

// ── Default rule: $XDG_CONFIG_HOME/<pack>/<rel_path> ─────────

#[test]
fn top_level_file_goes_to_pack_xdg_dir() {
    // Under #48: top-level files in a pack default to
    // $XDG_CONFIG_HOME/<pack>/<file>, not $HOME/.<file>.
    let config = HandlerConfig::default();
    let target = resolve_target("vim", "vimrc", &config, &test_pather());
    assert_eq!(target, PathBuf::from("/home/alice/.config/vim/vimrc"));
}

#[test]
fn top_level_dir_goes_to_pack_xdg_dir() {
    let config = HandlerConfig::default();
    // Top-level dir wholesale-linked: `nvim/lua` directory →
    // ~/.config/nvim/lua (the dir itself, not its files).
    let target = resolve_target("nvim", "lua", &config, &test_pather());
    assert_eq!(target, PathBuf::from("/home/alice/.config/nvim/lua"));
}

#[test]
fn top_level_dir_wholesale_goes_to_pack_xdg_dir() {
    let config = HandlerConfig::default();
    let target = resolve_target("warp", "themes", &config, &test_pather());
    assert_eq!(target, PathBuf::from("/home/alice/.config/warp/themes"));
}

// ── Pack ordering: prefix is stripped before path computation ─

#[test]
fn prefixed_pack_deploys_under_display_name_dir() {
    // `010-nvim/init.lua` deploys to `~/.config/nvim/init.lua` —
    // where neovim actually reads its config — not under
    // `~/.config/010-nvim/`. The ordering prefix lives on disk
    // and on the sort axis; it must not leak into the user's
    // filesystem.
    let config = HandlerConfig::default();
    let target = resolve_target("010-nvim", "init.lua", &config, &test_pather());
    assert_eq!(target, PathBuf::from("/home/alice/.config/nvim/init.lua"));
}

#[test]
fn prefixed_pack_works_with_underscore_separator() {
    let config = HandlerConfig::default();
    let target = resolve_target("020_zsh", "zshrc", &config, &test_pather());
    // `force_home` defaults are off here; default-rule path holds.
    assert_eq!(target, PathBuf::from("/home/alice/.config/zsh/zshrc"));
}

#[test]
fn prefixed_pack_with_force_home_still_strips_prefix() {
    // The `force_home` matching is keyed on the pack-relative
    // path (`ssh/config`), not the pack name, so this test mostly
    // confirms that prefix stripping doesn't perturb that path —
    // and that nested resolution still lands at `~/.ssh/config`
    // regardless of how the pack directory is named.
    let target = resolve_target("030-net", "ssh/config", &default_config(), &test_pather());
    assert_eq!(target, PathBuf::from("/home/alice/.ssh/config"));
}

/// Regression for the 0.16.0 pilot: pack `ghostty` with top-level
/// file `config` used to resolve to `$HOME/.config` (collision with
/// XDG_CONFIG_HOME directory). Under #48 it goes under the pack.
#[test]
fn top_level_file_named_config_goes_under_pack_no_xdg_collision() {
    let config = HandlerConfig::default();
    let target = resolve_target("ghostty", "config", &config, &test_pather());
    assert_eq!(target, PathBuf::from("/home/alice/.config/ghostty/config"));
}

#[test]
fn nested_file_namespaced_under_pack() {
    let config = HandlerConfig::default();
    let target = resolve_target("nvim", "lua/options.lua", &config, &test_pather());
    assert_eq!(
        target,
        PathBuf::from("/home/alice/.config/nvim/lua/options.lua")
    );
}

// ── Protected paths ─────────────────────────────────────────

#[test]
fn protected_exact_match() {
    assert!(is_protected("ssh/id_rsa", &[".ssh/id_rsa".into()]));
    assert!(is_protected(".ssh/id_rsa", &[".ssh/id_rsa".into()]));
}

#[test]
fn protected_parent_directory() {
    assert!(is_protected(
        "gnupg/private-keys-v1.d/key",
        &[".gnupg".into()]
    ));
}

#[test]
fn not_protected() {
    assert!(!is_protected("vimrc", &[".ssh/id_rsa".into()]));
}

// ── force_home matching ─────────────────────────────────────

#[test]
fn force_home_matches_without_dot() {
    assert!(is_force_home("ssh/config", &["ssh".into()]));
    assert!(is_force_home("bashrc", &["bashrc".into()]));
}

#[test]
fn force_home_does_not_match_unrelated() {
    assert!(!is_force_home("vimrc", &["ssh".into(), "bashrc".into()]));
}
