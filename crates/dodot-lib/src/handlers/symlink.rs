//! Symlink handler — the most complex handler.
//!
//! Creates double-link chains from source files to user-visible locations.
//! Implements a 3-layer target path mapping system:
//!
//! 1. **Layer 3** (highest): Explicit overrides (`_home/`, `_xdg/` prefix)
//! 2. **Layer 2**: `force_home` config list
//! 3. **Layer 1** (default): top-level → `$HOME/.{name}`,
//!    subdirectory → `$XDG_CONFIG_HOME/{path}`

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
                let user_path = resolve_target(&rel_str, false, config, paths);
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

    if !has_override {
        let user_path = resolve_target(&rel_str, true, config, paths);
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
        let user_path = resolve_target(&rel_str, false, config, paths);
        out.push(HandlerIntent::Link {
            pack: m.pack.clone(),
            handler: HANDLER_SYMLINK.into(),
            source: entry.path.clone(),
            user_path,
        });
    }
    Ok(())
}

/// Strip the `dot.` prefix from a filename, returning the dotted version.
/// `dot.bashrc` → `.bashrc`, `dot.vimrc` → `.vimrc`.
/// Only applies to top-level files (no `/` in path).
fn strip_dot_prefix(rel_path: &str) -> Option<String> {
    if !rel_path.contains('/') {
        if let Some(rest) = rel_path.strip_prefix("dot.") {
            return Some(format!(".{rest}"));
        }
    }
    None
}

/// Resolve the target path for a symlink using the layered system.
///
/// `is_dir` is true when the match is a top-level directory that will
/// be linked wholesale — in that case the default target is
/// `$XDG_CONFIG_HOME/<name>` (dirs are not dot-prefixed).
///
/// Priority (highest first):
/// 0. Custom target override from config (`[symlink.targets]`)
/// 1. `dot.` prefix convention (top-level only)
/// 2. Layer 3: Explicit `_home/` or `_xdg/` directory prefix
/// 3. Layer 2: `force_home` config list
/// 4. Layer 1: Smart defaults (top-level file → `$HOME/.{name}`,
///    top-level dir → `$XDG_CONFIG_HOME/{name}`, subdirs → `$XDG_CONFIG_HOME/{path}`)
pub(crate) fn resolve_target(
    rel_path: &str,
    is_dir: bool,
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

    // dot. prefix convention: dot.bashrc → .bashrc (top-level only)
    // Applied before all layers so it works with force_home and defaults.
    if let Some(dotted) = strip_dot_prefix(rel_path) {
        // Treat as if the file were already named with a dot prefix.
        // force_home still applies to the stripped name.
        let base = dotted.strip_prefix('.').unwrap_or(&dotted);
        if is_force_home(base, &config.force_home) {
            return home.join(&dotted);
        }
        // Default: top-level dot-prefixed file goes to $HOME
        return home.join(&dotted);
    }

    // Layer 3: Explicit overrides
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

    // Layer 2: force_home
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

    // Layer 1: Smart defaults
    if !rel_path.contains('/') {
        if is_dir {
            // Top-level dir → $XDG_CONFIG_HOME/{name} (dirs are not dot-prefixed).
            return xdg_config.join(rel_path);
        }
        // Top-level file → $HOME/.{name}
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

    // Files already starting with dot go to $HOME directly
    if rel_path.starts_with('.') {
        return home.join(rel_path);
    }

    // Subdirectory files → $XDG_CONFIG_HOME/{path}
    // Strip redundant config/ or .config/ prefix
    let cleaned = rel_path
        .strip_prefix(".config/")
        .or_else(|| rel_path.strip_prefix("config/"))
        .unwrap_or(rel_path);

    xdg_config.join(cleaned)
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

    // ── Layer 1: Smart defaults ─────────────────────────────────

    #[test]
    fn top_level_file_goes_to_home_with_dot() {
        let target = resolve_target("vimrc", false, &default_config(), &test_pather());
        assert_eq!(target, PathBuf::from("/home/alice/.vimrc"));
    }

    #[test]
    fn top_level_dotfile_keeps_dot() {
        let target = resolve_target(".bashrc", false, &default_config(), &test_pather());
        assert_eq!(target, PathBuf::from("/home/alice/.bashrc"));
    }

    #[test]
    fn subdirectory_goes_to_xdg() {
        let config = HandlerConfig::default(); // no force_home
        let target = resolve_target("nvim/init.lua", false, &config, &test_pather());
        assert_eq!(target, PathBuf::from("/home/alice/.config/nvim/init.lua"));
    }

    #[test]
    fn dotted_subdirectory_goes_to_home() {
        let config = HandlerConfig::default();
        let target = resolve_target(".vim/colors/theme.vim", false, &config, &test_pather());
        assert_eq!(target, PathBuf::from("/home/alice/.vim/colors/theme.vim"));
    }

    #[test]
    fn config_prefix_stripped() {
        let config = HandlerConfig::default();
        let target = resolve_target("config/nvim/init.lua", false, &config, &test_pather());
        assert_eq!(target, PathBuf::from("/home/alice/.config/nvim/init.lua"));
    }

    // ── Layer 2: force_home ─────────────────────────────────────

    #[test]
    fn force_home_top_level() {
        let target = resolve_target("bashrc", false, &default_config(), &test_pather());
        assert_eq!(target, PathBuf::from("/home/alice/.bashrc"));
    }

    #[test]
    fn force_home_subdirectory() {
        let target = resolve_target("ssh/config", false, &default_config(), &test_pather());
        assert_eq!(target, PathBuf::from("/home/alice/.ssh/config"));
    }

    // ── Layer 3: Explicit overrides ─────────────────────────────

    #[test]
    fn home_prefix_override() {
        let config = HandlerConfig::default();
        let target = resolve_target("_home/vim/vimrc", false, &config, &test_pather());
        assert_eq!(target, PathBuf::from("/home/alice/.vim/vimrc"));
    }

    #[test]
    fn xdg_prefix_override() {
        let config = HandlerConfig::default();
        let target = resolve_target("_xdg/nvim/init.lua", false, &config, &test_pather());
        assert_eq!(target, PathBuf::from("/home/alice/.config/nvim/init.lua"));
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

    // ── dot. prefix convention ──────────────────────────────────

    #[test]
    fn dot_prefix_stripped_for_top_level() {
        let target = resolve_target("dot.bashrc", false, &default_config(), &test_pather());
        assert_eq!(target, PathBuf::from("/home/alice/.bashrc"));
    }

    #[test]
    fn dot_prefix_with_force_home() {
        let target = resolve_target("dot.zshrc", false, &default_config(), &test_pather());
        assert_eq!(target, PathBuf::from("/home/alice/.zshrc"));
    }

    #[test]
    fn dot_prefix_non_forced_file() {
        let config = HandlerConfig::default(); // no force_home
        let target = resolve_target("dot.vimrc", false, &config, &test_pather());
        assert_eq!(target, PathBuf::from("/home/alice/.vimrc"));
    }

    #[test]
    fn dot_prefix_not_applied_to_subdirs() {
        // dot. prefix only works for top-level files
        let config = HandlerConfig::default();
        let target = resolve_target("subdir/dot.conf", false, &config, &test_pather());
        // Should NOT strip dot. — it's not top-level
        assert_eq!(target, PathBuf::from("/home/alice/.config/subdir/dot.conf"));
    }

    #[test]
    fn strip_dot_prefix_unit() {
        assert_eq!(strip_dot_prefix("dot.bashrc"), Some(".bashrc".into()));
        assert_eq!(strip_dot_prefix("dot.vimrc"), Some(".vimrc".into()));
        assert_eq!(strip_dot_prefix("vimrc"), None);
        assert_eq!(strip_dot_prefix(".bashrc"), None);
        assert_eq!(strip_dot_prefix("sub/dot.conf"), None); // not top-level
    }

    // ── Custom target overrides ─────────────────────────────────

    #[test]
    fn custom_target_absolute_path() {
        let mut config = HandlerConfig::default();
        config
            .targets
            .insert("misterious.conf".into(), "/var/etc/misterious.conf".into());

        let target = resolve_target("misterious.conf", false, &config, &test_pather());
        assert_eq!(target, PathBuf::from("/var/etc/misterious.conf"));
    }

    #[test]
    fn custom_target_relative_path() {
        let mut config = HandlerConfig::default();
        config.targets.insert(
            "home-bound.conf".into(),
            "my-documents/home-bound.conf".into(),
        );

        let target = resolve_target("home-bound.conf", false, &config, &test_pather());
        assert_eq!(
            target,
            PathBuf::from("/home/alice/.config/my-documents/home-bound.conf")
        );
    }

    #[test]
    fn custom_target_overrides_all_layers() {
        // Even a force_home file can be overridden
        let mut config = default_config();
        config
            .targets
            .insert("bashrc".into(), "/custom/bashrc".into());

        let target = resolve_target("bashrc", false, &config, &test_pather());
        assert_eq!(target, PathBuf::from("/custom/bashrc"));
    }

    #[test]
    fn no_custom_target_falls_through() {
        let config = HandlerConfig::default();
        // No targets configured — should use default behavior
        let target = resolve_target("vimrc", false, &config, &test_pather());
        assert_eq!(target, PathBuf::from("/home/alice/.vimrc"));
    }

    // ── Top-level dir resolution ────────────────────────────────

    #[test]
    fn top_level_dir_goes_to_xdg() {
        let config = HandlerConfig::default();
        let target = resolve_target("nvim", true, &config, &test_pather());
        assert_eq!(target, PathBuf::from("/home/alice/.config/nvim"));
    }

    #[test]
    fn top_level_dir_still_respects_force_home() {
        let target = resolve_target("ssh", true, &default_config(), &test_pather());
        assert_eq!(target, PathBuf::from("/home/alice/.ssh"));
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
            assert!(user_path.ends_with(".config/themes"));
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
