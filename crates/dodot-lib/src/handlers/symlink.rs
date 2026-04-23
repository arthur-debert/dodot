//! Symlink handler — the most complex handler.
//!
//! Creates double-link chains from source files to user-visible locations.
//! Target resolution priority (highest first):
//!
//! 0. **Custom target** from `[symlink.targets]` config
//! 1. **`home.X` prefix** (top-level files only) — routes to `$HOME/.X`
//! 2. **`_home/` or `_xdg/` directory prefix** — raw `$HOME/.<rest>` or
//!    `$XDG_CONFIG_HOME/<rest>` (escape hatches that skip pack
//!    namespacing entirely)
//! 3. **`force_home` config list** — canonical `$HOME` tools (ssh, gpg,
//!    bashrc, etc.)
//! 4. **Default**: `$XDG_CONFIG_HOME/<pack>/<rel_path>` for every
//!    pack-root entry (file or directory) and every nested file. The
//!    pack name namespaces config under XDG, matching modern tool
//!    conventions (nvim, helix, ghostty, …) without requiring users
//!    to write `pack/program/` doubled paths.

use std::path::{Path, PathBuf};

use crate::datastore::DataStore;
use crate::fs::Fs;
use crate::handlers::{
    Handler, HandlerCategory, HandlerConfig, HandlerScope, HandlerStatus, MatchMode,
    HANDLER_SYMLINK,
};
use crate::operations::HandlerIntent;
use crate::paths::Pather;
use crate::rules::RuleMatch;
use crate::Result;

pub struct SymlinkHandler;

impl Handler for SymlinkHandler {
    fn name(&self) -> &str {
        HANDLER_SYMLINK
    }

    fn category(&self) -> HandlerCategory {
        HandlerCategory::Configuration
    }

    fn match_mode(&self) -> MatchMode {
        MatchMode::Catchall
    }

    fn scope(&self) -> HandlerScope {
        HandlerScope::Exclusive
    }

    fn to_intents(
        &self,
        matches: &[RuleMatch],
        config: &HandlerConfig,
        paths: &dyn Pather,
        fs: &dyn Fs,
    ) -> Result<Vec<HandlerIntent>> {
        let mut intents = Vec::new();

        for m in matches {
            let rel_str = m.relative_path.to_string_lossy();

            // Check protected paths
            if is_protected(&rel_str, &config.protected_paths) {
                continue;
            }

            if m.is_dir {
                intents.extend(dir_intents(m, config, paths, fs)?);
            } else {
                let user_path = resolve_target(&m.pack, &rel_str, config, paths);
                intents.push(HandlerIntent::Link {
                    pack: m.pack.clone(),
                    handler: HANDLER_SYMLINK.into(),
                    source: m.absolute_path.clone(),
                    user_path,
                });
            }
        }

        Ok(intents)
    }

    fn check_status(
        &self,
        file: &Path,
        pack: &str,
        datastore: &dyn DataStore,
    ) -> Result<HandlerStatus> {
        let filename = file.file_name().unwrap_or_default().to_string_lossy();
        let has_state = datastore.has_handler_state(pack, HANDLER_SYMLINK)?;

        Ok(HandlerStatus {
            file: file.to_string_lossy().into_owned(),
            handler: HANDLER_SYMLINK.into(),
            deployed: has_state,
            message: if has_state {
                format!("linked to $HOME/.{filename}")
            } else {
                format!("will be linked to $HOME/.{filename}")
            },
        })
    }
}

/// Produce symlink intents for a directory match.
///
/// Wholesale mode (one symlink for the whole directory) is the default.
/// Per-file mode is triggered when the directory contains any file whose
/// relative path matches a `protected_paths` entry or appears as a key
/// in `symlink.targets`. In per-file mode we recurse and emit one Link
/// intent per non-protected file, each resolved independently.
fn dir_intents(
    m: &RuleMatch,
    config: &HandlerConfig,
    paths: &dyn Pather,
    fs: &dyn Fs,
) -> Result<Vec<HandlerIntent>> {
    let rel_str = m.relative_path.to_string_lossy();
    let dir_prefix = format!("{rel_str}/");

    let has_override = config.protected_paths.iter().any(|p| {
        let normalized = p.strip_prefix('.').unwrap_or(p);
        normalized.starts_with(&dir_prefix)
            || p.starts_with(&dir_prefix)
            || normalized == rel_str
            || p == rel_str.as_ref()
    }) || config
        .targets
        .keys()
        .any(|k| k.starts_with(&dir_prefix) || k == rel_str.as_ref());

    // `_home/` and `_xdg/` are per-subtree escape hatches that strip
    // their prefix during file-level resolution. Wholesale-linking the
    // top-level `_home` or `_xdg` dir would bake the prefix into the
    // deploy path (e.g. `~/.config/<pack>/_home`) — clearly not what
    // the user meant. Force per-file mode for these.
    let is_escape_prefix_dir = matches!(rel_str.as_ref(), "_home" | "_xdg");

    if !has_override && !is_escape_prefix_dir {
        let user_path = resolve_target(&m.pack, &rel_str, config, paths);
        return Ok(vec![HandlerIntent::Link {
            pack: m.pack.clone(),
            handler: HANDLER_SYMLINK.into(),
            source: m.absolute_path.clone(),
            user_path,
        }]);
    }

    // Per-file mode: recurse the directory and emit one intent per file.
    let mut intents = Vec::new();
    collect_per_file_intents(m, &m.absolute_path, config, paths, fs, &mut intents)?;
    Ok(intents)
}

fn collect_per_file_intents(
    m: &RuleMatch,
    dir: &Path,
    config: &HandlerConfig,
    paths: &dyn Pather,
    fs: &dyn Fs,
    out: &mut Vec<HandlerIntent>,
) -> Result<()> {
    let entries = fs.read_dir(dir)?;
    for entry in entries {
        // Skip dodot's own files and anything matching the pack's
        // ignore patterns — same filter the scanner applies at walk
        // time, so per-file fallback doesn't pick up `.DS_Store`,
        // `.dodot.toml`, `*.swp`, etc.
        if crate::rules::should_skip_entry(&entry.name, &config.pack_ignore) {
            continue;
        }
        if entry.is_dir {
            collect_per_file_intents(m, &entry.path, config, paths, fs, out)?;
            continue;
        }
        let rel = entry
            .path
            .strip_prefix(&m.absolute_path)
            .ok()
            .map(|r| m.relative_path.join(r))
            .unwrap_or_else(|| PathBuf::from(&entry.name));
        let rel_str = rel.to_string_lossy();
        if is_protected(&rel_str, &config.protected_paths) {
            continue;
        }
        let user_path = resolve_target(&m.pack, &rel_str, config, paths);
        out.push(HandlerIntent::Link {
            pack: m.pack.clone(),
            handler: HANDLER_SYMLINK.into(),
            source: entry.path.clone(),
            user_path,
        });
    }
    Ok(())
}

/// Strip the `home.` prefix from a filename, returning the `$HOME`-bound
/// dotted version. `home.bashrc` → `.bashrc`, `home.vimrc` → `.vimrc`.
/// Only applies to top-level files (no `/` in path).
///
/// This is the per-file opt-in for "deploy to `$HOME/.<rest>` instead of
/// the default `$XDG_CONFIG_HOME/<pack>/<rest>`". For per-subtree
/// opt-out, use the `_home/` directory prefix.
fn strip_home_prefix(rel_path: &str) -> Option<String> {
    if !rel_path.contains('/') {
        if let Some(rest) = rel_path.strip_prefix("home.") {
            return Some(format!(".{rest}"));
        }
    }
    None
}

/// Resolve the target path for a symlink.
///
/// `pack` is the pack name; it namespaces the default XDG target so
/// `pack vim/vimrc` deploys under `$XDG_CONFIG_HOME/vim/vimrc` rather
/// than `$XDG_CONFIG_HOME/vimrc`.
///
/// Priority (highest first):
/// 0. Custom target override from config (`[symlink.targets]`)
/// 1. `home.X` prefix convention (top-level files only) → `$HOME/.X`
/// 2. Explicit `_home/` or `_xdg/` directory prefix (escape hatches —
///    skip pack namespacing entirely)
/// 3. `force_home` config list (ssh, gpg, bashrc, …)
/// 4. Default: `$XDG_CONFIG_HOME/<pack>/<rel_path>`
pub(crate) fn resolve_target(
    pack: &str,
    rel_path: &str,
    config: &HandlerConfig,
    paths: &dyn Pather,
) -> PathBuf {
    let home = paths.home_dir();
    let xdg_config = paths.xdg_config_home();

    // Priority 0: Custom target override from [symlink.targets]
    if let Some(target) = config.targets.get(rel_path) {
        if target.starts_with('/') {
            // Absolute path — use as-is
            return PathBuf::from(target);
        }
        // Relative path — resolve from XDG_CONFIG_HOME
        return xdg_config.join(target);
    }

    // Priority 1: home. prefix convention (per-file opt-in for $HOME placement)
    // home.bashrc → ~/.bashrc, home.vimrc → ~/.vimrc (top-level files only).
    if let Some(dotted) = strip_home_prefix(rel_path) {
        return home.join(&dotted);
    }

    // Priority 2: Explicit directory-prefix escape hatches.
    // _home/<rest> → $HOME/.<rest> (raw, no pack namespace)
    // _xdg/<rest>  → $XDG_CONFIG_HOME/<rest> (raw, no pack namespace)
    if let Some(stripped) = rel_path.strip_prefix("_home/") {
        let parts: Vec<&str> = stripped.split('/').collect();
        if let Some(first) = parts.first() {
            if !first.is_empty() && !first.starts_with('.') {
                let mut new_parts = vec![format!(".{first}")];
                new_parts.extend(parts[1..].iter().map(|s| s.to_string()));
                return home.join(new_parts.join("/"));
            }
        }
        return home.join(stripped);
    }

    if let Some(stripped) = rel_path.strip_prefix("_xdg/") {
        return xdg_config.join(stripped);
    }

    // Priority 3: force_home blacklist (ssh, gpg, bashrc, …)
    if is_force_home(rel_path, &config.force_home) {
        if rel_path.contains('/') {
            // Subdirectory file forced to home (e.g. ssh/config -> .ssh/config)
            let parts: Vec<&str> = rel_path.split('/').collect();
            let first = parts[0];
            let dotted = if first.starts_with('.') {
                first.to_string()
            } else {
                format!(".{first}")
            };
            let rest: Vec<&str> = parts[1..].to_vec();
            let mut result = home.join(dotted);
            for part in rest {
                result = result.join(part);
            }
            return result;
        }

        let filename = Path::new(rel_path)
            .file_name()
            .unwrap_or_default()
            .to_string_lossy();
        let dotted = if filename.starts_with('.') {
            filename.to_string()
        } else {
            format!(".{filename}")
        };
        return home.join(dotted);
    }

    // Priority 4: Default — $XDG_CONFIG_HOME/<pack>/<rel_path>
    //
    // The pack name namespaces every entry by default so common modern
    // tools (nvim, helix, ghostty, …) work out of the box without
    // requiring `pack/program/` doubled paths. The escape hatches above
    // cover legacy `$HOME` tools and any user-specified overrides.
    xdg_config.join(pack).join(rel_path)
}

/// Check if a path matches any force_home entry.
fn is_force_home(rel_path: &str, force_home: &[String]) -> bool {
    let first_segment = rel_path.split('/').next().unwrap_or(rel_path);
    let without_dot = first_segment.strip_prefix('.').unwrap_or(first_segment);

    force_home.iter().any(|entry| {
        let entry_without_dot = entry.strip_prefix('.').unwrap_or(entry);
        entry_without_dot == without_dot
    })
}

/// Check if a path is in the protected paths list.
fn is_protected(rel_path: &str, protected_paths: &[String]) -> bool {
    let normalized = rel_path.strip_prefix("./").unwrap_or(rel_path);
    let with_dot = if !normalized.starts_with('.') {
        format!(".{normalized}")
    } else {
        normalized.to_string()
    };

    for protected in protected_paths {
        // Exact match
        if protected == normalized || protected == &with_dot {
            return true;
        }
        // Parent directory match
        if normalized.starts_with(&format!("{protected}/"))
            || with_dot.starts_with(&format!("{protected}/"))
        {
            return true;
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::paths::XdgPather;

    fn test_pather() -> XdgPather {
        XdgPather::builder()
            .home("/home/alice")
            .dotfiles_root("/home/alice/dotfiles")
            .xdg_config_home("/home/alice/.config")
            .build()
            .unwrap()
    }

    fn default_config() -> HandlerConfig {
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

    // ── Priority 3: force_home ──────────────────────────────────

    #[test]
    fn force_home_top_level_file() {
        let target = resolve_target("shell", "bashrc", &default_config(), &test_pather());
        assert_eq!(target, PathBuf::from("/home/alice/.bashrc"));
    }

    #[test]
    fn force_home_subdirectory_file() {
        let target = resolve_target("net", "ssh/config", &default_config(), &test_pather());
        assert_eq!(target, PathBuf::from("/home/alice/.ssh/config"));
    }

    #[test]
    fn force_home_top_level_dir_wholesale() {
        let target = resolve_target("net", "ssh", &default_config(), &test_pather());
        assert_eq!(target, PathBuf::from("/home/alice/.ssh"));
    }

    // ── Priority 2: _home/ and _xdg/ escape hatches ─────────────

    #[test]
    fn home_prefix_dir_escapes_pack_namespace() {
        // _home/<rest> deploys raw to $HOME/.<rest>, regardless of pack
        // name. Useful when a single pack groups files that belong in
        // $HOME without being in force_home.
        let config = HandlerConfig::default();
        let target = resolve_target("misc", "_home/vim/vimrc", &config, &test_pather());
        assert_eq!(target, PathBuf::from("/home/alice/.vim/vimrc"));
    }

    #[test]
    fn xdg_prefix_dir_escapes_pack_namespace() {
        // _xdg/<rest> deploys raw to $XDG_CONFIG_HOME/<rest>, NOT under
        // the pack name. Useful for packs whose name doesn't match the
        // target program (e.g. a `term-config` pack containing
        // `_xdg/ghostty/config`).
        let config = HandlerConfig::default();
        let target = resolve_target(
            "term-config",
            "_xdg/ghostty/config",
            &config,
            &test_pather(),
        );
        assert_eq!(target, PathBuf::from("/home/alice/.config/ghostty/config"));
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

    // ── Priority 1: home. prefix convention ─────────────────────

    #[test]
    fn home_prefix_routes_top_level_file_to_home() {
        // home.X is the per-file opt-in for $HOME/.X placement, replacing
        // the older `dot.X` prefix in #48.
        let config = HandlerConfig::default();
        let target = resolve_target("git", "home.gitconfig", &config, &test_pather());
        assert_eq!(target, PathBuf::from("/home/alice/.gitconfig"));
    }

    #[test]
    fn home_prefix_works_even_when_pack_not_force_home() {
        let config = HandlerConfig::default();
        let target = resolve_target("misc", "home.vimrc", &config, &test_pather());
        assert_eq!(target, PathBuf::from("/home/alice/.vimrc"));
    }

    #[test]
    fn home_prefix_not_applied_to_subdirs() {
        // home. prefix only works for top-level files; nested
        // home.<x> files keep the literal name under the pack's XDG dir.
        let config = HandlerConfig::default();
        let target = resolve_target("misc", "subdir/home.conf", &config, &test_pather());
        assert_eq!(
            target,
            PathBuf::from("/home/alice/.config/misc/subdir/home.conf")
        );
    }

    #[test]
    fn strip_home_prefix_unit() {
        assert_eq!(strip_home_prefix("home.bashrc"), Some(".bashrc".into()));
        assert_eq!(strip_home_prefix("home.vimrc"), Some(".vimrc".into()));
        assert_eq!(strip_home_prefix("vimrc"), None);
        assert_eq!(strip_home_prefix(".bashrc"), None);
        assert_eq!(strip_home_prefix("sub/home.conf"), None);
    }

    // ── Custom target overrides ─────────────────────────────────

    #[test]
    fn custom_target_absolute_path() {
        let mut config = HandlerConfig::default();
        config
            .targets
            .insert("misterious.conf".into(), "/var/etc/misterious.conf".into());

        let target = resolve_target("pack", "misterious.conf", &config, &test_pather());
        assert_eq!(target, PathBuf::from("/var/etc/misterious.conf"));
    }

    #[test]
    fn custom_target_relative_path() {
        let mut config = HandlerConfig::default();
        config.targets.insert(
            "home-bound.conf".into(),
            "my-documents/home-bound.conf".into(),
        );

        let target = resolve_target("pack", "home-bound.conf", &config, &test_pather());
        assert_eq!(
            target,
            PathBuf::from("/home/alice/.config/my-documents/home-bound.conf")
        );
    }

    #[test]
    fn custom_target_overrides_all_layers() {
        // Even a force_home file can be overridden.
        let mut config = default_config();
        config
            .targets
            .insert("bashrc".into(), "/custom/bashrc".into());

        let target = resolve_target("shell", "bashrc", &config, &test_pather());
        assert_eq!(target, PathBuf::from("/custom/bashrc"));
    }

    #[test]
    fn no_custom_target_falls_through_to_pack_namespaced_default() {
        let config = HandlerConfig::default();
        let target = resolve_target("vim", "vimrc", &config, &test_pather());
        assert_eq!(target, PathBuf::from("/home/alice/.config/vim/vimrc"));
    }

    // ── Wholesale vs per-file dir behavior ──────────────────────

    fn build_dir_match(env: &crate::testing::TempEnvironment, pack: &str, dir: &str) -> RuleMatch {
        RuleMatch {
            relative_path: PathBuf::from(dir),
            absolute_path: env.dotfiles_root.join(pack).join(dir),
            pack: pack.into(),
            handler: HANDLER_SYMLINK.into(),
            is_dir: true,
            options: std::collections::HashMap::new(),
            preprocessor_source: None,
        }
    }

    #[test]
    fn plain_top_level_dir_produces_single_wholesale_intent() {
        let env = crate::testing::TempEnvironment::builder()
            .pack("warp")
            .file("themes/nord.yaml", "a")
            .file("themes/vs_code.yaml", "b")
            .done()
            .build();
        let m = build_dir_match(&env, "warp", "themes");
        let handler = SymlinkHandler;
        let paths = crate::paths::XdgPather::builder()
            .home(&env.home)
            .dotfiles_root(&env.dotfiles_root)
            .build()
            .unwrap();
        let intents = handler
            .to_intents(&[m], &HandlerConfig::default(), &paths, env.fs.as_ref())
            .unwrap();
        assert_eq!(intents.len(), 1, "plain dir -> single wholesale intent");
        if let HandlerIntent::Link {
            source, user_path, ..
        } = &intents[0]
        {
            assert!(source.ends_with("warp/themes"));
            // Under #48, top-level dirs deploy under the pack's XDG dir.
            assert!(
                user_path.ends_with(".config/warp/themes"),
                "user_path={}",
                user_path.display()
            );
        } else {
            panic!("expected Link intent");
        }
    }

    #[test]
    fn dir_with_protected_path_falls_back_to_per_file_and_skips_protected() {
        let env = crate::testing::TempEnvironment::builder()
            .pack("secret")
            .file("ssh/config", "Host *")
            .file("ssh/id_rsa", "DO NOT LINK")
            .done()
            .build();
        let m = build_dir_match(&env, "secret", "ssh");
        let handler = SymlinkHandler;
        let config = HandlerConfig {
            protected_paths: vec!["ssh/id_rsa".into()],
            force_home: vec!["ssh".into()],
            ..HandlerConfig::default()
        };
        let paths = crate::paths::XdgPather::builder()
            .home(&env.home)
            .dotfiles_root(&env.dotfiles_root)
            .build()
            .unwrap();
        let intents = handler
            .to_intents(&[m], &config, &paths, env.fs.as_ref())
            .unwrap();
        assert_eq!(
            intents.len(),
            1,
            "only ssh/config should be linked; id_rsa skipped. Got: {intents:?}"
        );
        if let HandlerIntent::Link {
            source, user_path, ..
        } = &intents[0]
        {
            assert!(source.ends_with("ssh/config"));
            // force_home=["ssh"] routes subdir config to $HOME/.ssh/config
            assert!(user_path.ends_with(".ssh/config"));
        } else {
            panic!("expected Link intent");
        }
    }

    #[test]
    fn per_file_fallback_skips_special_and_pack_ignored_files() {
        // When per-file mode kicks in (because of a protected_path),
        // the recursion must apply the same filters the scanner uses:
        // dodot's own files and pack-ignore globs like `.DS_Store`.
        let env = crate::testing::TempEnvironment::builder()
            .pack("cfg")
            .file("ssh/config", "Host *")
            .file("ssh/id_rsa", "secret")
            .file("ssh/.DS_Store", "garbage")
            .file("ssh/.dodot.toml", "# pack config")
            .done()
            .build();
        let m = build_dir_match(&env, "cfg", "ssh");
        let handler = SymlinkHandler;
        let config = HandlerConfig {
            protected_paths: vec!["ssh/id_rsa".into()],
            pack_ignore: vec![".DS_Store".into()],
            ..HandlerConfig::default()
        };
        let paths = crate::paths::XdgPather::builder()
            .home(&env.home)
            .dotfiles_root(&env.dotfiles_root)
            .build()
            .unwrap();
        let intents = handler
            .to_intents(&[m], &config, &paths, env.fs.as_ref())
            .unwrap();
        assert_eq!(
            intents.len(),
            1,
            "only ssh/config should be linked. Got: {intents:?}"
        );
        if let HandlerIntent::Link { source, .. } = &intents[0] {
            assert!(source.ends_with("ssh/config"));
        }
    }

    #[test]
    fn dir_with_targets_override_falls_back_to_per_file() {
        let env = crate::testing::TempEnvironment::builder()
            .pack("app")
            .file("config/main.toml", "x")
            .file("config/aux.toml", "y")
            .done()
            .build();
        let m = build_dir_match(&env, "app", "config");
        let handler = SymlinkHandler;
        let mut targets = std::collections::HashMap::new();
        targets.insert("config/main.toml".into(), "/etc/main.toml".into());
        let config = HandlerConfig {
            targets,
            ..HandlerConfig::default()
        };
        let paths = crate::paths::XdgPather::builder()
            .home(&env.home)
            .dotfiles_root(&env.dotfiles_root)
            .build()
            .unwrap();
        let intents = handler
            .to_intents(&[m], &config, &paths, env.fs.as_ref())
            .unwrap();
        // Both files should get per-file intents — targets override forces
        // per-file mode so main.toml gets the explicit path.
        assert_eq!(intents.len(), 2, "intents: {intents:?}");
        let main = intents
            .iter()
            .find(|i| matches!(i, HandlerIntent::Link { source, .. } if source.ends_with("config/main.toml")))
            .expect("main.toml intent");
        if let HandlerIntent::Link { user_path, .. } = main {
            assert_eq!(user_path, &PathBuf::from("/etc/main.toml"));
        }
    }
}
