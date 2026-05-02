//! Symlink handler — the most complex handler.
//!
//! Creates double-link chains from source files to user-visible locations.
//! Target resolution priority (highest first):
//!
//! 0. **Custom target** from `[symlink.targets]` config
//! 1. **File-level prefixes** (top-level files only, skip pack namespace):
//!    a. `home.X` → `$HOME/.X`
//!    b. `app.X`  → `<app_support_dir>/X`
//!    c. `xdg.X`  → `$XDG_CONFIG_HOME/X`
//!    d. `lib.X`  → `$HOME/Library/X` (macOS only; warn elsewhere)
//! 2. **Directory prefixes** (per-subtree, skip pack namespace):
//!    a. `_home/<rest>` → `$HOME/.<rest>`
//!    b. `_xdg/<rest>`  → `$XDG_CONFIG_HOME/<rest>`
//!    c. `_app/<rest>`  → `<app_support_dir>/<rest>` (macOS:
//!    `~/Library/Application Support`; non-macOS: collapses to
//!    `xdg_config_home`)
//!    d. `_lib/<rest>`  → `$HOME/Library/<rest>` (macOS only; emits a
//!    warning and skips on other platforms)
//! 3. **`force_home` config list** — canonical `$HOME` tools (ssh, gpg,
//!    bashrc, etc.)
//! 4. **`force_app` config list** — curated GUI-app folders that route
//!    to `<app_support_dir>/<first-segment>/<rest>` without requiring
//!    a `_app/` prefix in the pack tree.
//! 5. **`app_aliases[pack]`** — pack-level rewrite that reroutes the
//!    *default rule* to `<app_support_dir>/<alias>/<rel_path>` so a
//!    natural pack name (`vscode`) can deploy to a GUI-app folder
//!    name (`Code`) without `_app/` prefixes everywhere.
//! 6. **Default**: `$XDG_CONFIG_HOME/<pack>/<rel_path>` for every
//!    pack-root entry (file or directory) and every nested file. The
//!    pack name namespaces config under XDG, matching modern tool
//!    conventions (nvim, helix, ghostty, …) without requiring users
//!    to write `pack/program/` doubled paths.
//!
//! When `[symlink.targets]` declares a destination for a file that
//! *also* carries a filesystem-naming prefix from priorities 1 or 2,
//! resolution refuses with `DodotError::RoutingOverrideConflict` rather
//! than silently letting `targets` win. Two ways to say where one file
//! goes is bug-bait — the user must pick one.
//!
//! See `docs/proposals/macos-paths.lex` for the full rationale behind
//! the third coordinate (`app_support_dir`) and the `_app/` / `_lib/`
//! prefix family.

use std::path::{Path, PathBuf};

use crate::datastore::DataStore;
use crate::fs::Fs;
use crate::handlers::{
    ExecutionPhase, Handler, HandlerConfig, HandlerScope, HandlerStatus, MatchMode, HANDLER_SYMLINK,
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

    fn phase(&self) -> ExecutionPhase {
        ExecutionPhase::Link
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
                check_routing_conflict(&m.pack, &rel_str, config)?;
                match resolve_target_full(&m.pack, &rel_str, config, paths) {
                    Resolution::Path(user_path) => intents.push(HandlerIntent::Link {
                        pack: m.pack.clone(),
                        handler: HANDLER_SYMLINK.into(),
                        source: m.absolute_path.clone(),
                        user_path,
                    }),
                    Resolution::Skip { .. } => {
                        // `_lib/` on non-macOS — silently skipped here;
                        // `warnings_for_matches` produces the user-visible
                        // text in PackStatusResult.warnings.
                    }
                }
            }
        }

        Ok(intents)
    }

    fn warnings_for_matches(
        &self,
        matches: &[RuleMatch],
        config: &HandlerConfig,
        paths: &dyn Pather,
    ) -> Vec<String> {
        if cfg!(target_os = "macos") {
            return Vec::new();
        }
        let mut out = Vec::new();
        for m in matches {
            let rel_str = m.relative_path.to_string_lossy();
            if is_protected(&rel_str, &config.protected_paths) {
                continue;
            }
            // Surface a single warning per macOS-only entry, covering
            // both the per-subtree `_lib/` directory prefix and the
            // top-level `lib.X` file prefix. The resolver returns
            // `Resolution::Skip` for both on every non-macOS host; the
            // warning explains why no symlink got created.
            let is_lib_dir = rel_str == "_lib" || rel_str.starts_with("_lib/");
            let is_lib_file =
                !m.is_dir && matches!(strip_file_prefix(&rel_str), Some((FilePrefix::Lib, _)));
            if is_lib_dir || is_lib_file {
                out.push(format!(
                    "warning: pack `{}` contains `{rel_str}` — \
                     macOS-only path, skipping on this platform",
                    m.pack
                ));
            }
        }
        let _ = paths; // reserved for future per-warning path enrichment
        out
    }

    fn check_status(
        &self,
        file: &Path,
        pack: &str,
        datastore: &dyn DataStore,
    ) -> Result<HandlerStatus> {
        let has_state = datastore.has_handler_state(pack, HANDLER_SYMLINK)?;

        // The trait doesn't carry a `Pather`, so we can't compute the
        // resolved deploy path here. Producing a hand-rolled path string
        // would re-implement (and inevitably drift from) `resolve_target`
        // — the very bug-bomb #48's centralization is meant to prevent.
        // Use a path-free message in the style of `path` and `shell`
        // handlers; callers that need the deploy path call
        // `resolve_target` directly via the `status::status()` flow.
        Ok(HandlerStatus {
            file: file.to_string_lossy().into_owned(),
            handler: HANDLER_SYMLINK.into(),
            deployed: has_state,
            message: if has_state {
                "symlink deployed".into()
            } else {
                "symlink pending".into()
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

    // `_home/`, `_xdg/`, `_app/`, and `_lib/` are per-subtree escape
    // hatches that strip their prefix during file-level resolution.
    // Wholesale-linking the top-level escape dir would bake the prefix
    // into the deploy path (e.g. `~/.config/<pack>/_home`) — clearly
    // not what the user meant. Force per-file mode for these.
    let is_escape_prefix_dir = matches!(rel_str.as_ref(), "_home" | "_xdg" | "_app" | "_lib");

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
        check_routing_conflict(&m.pack, &rel_str, config)?;
        // Use the full Resolution channel so `_lib/` on non-macOS is
        // skipped (no Link intent produced); `warnings_for_matches`
        // surfaces the user-visible warning out-of-band.
        match resolve_target_full(&m.pack, &rel_str, config, paths) {
            Resolution::Path(user_path) => out.push(HandlerIntent::Link {
                pack: m.pack.clone(),
                handler: HANDLER_SYMLINK.into(),
                source: entry.path.clone(),
                user_path,
            }),
            Resolution::Skip { .. } => continue,
        }
    }
    Ok(())
}

/// File-level routing prefixes for top-level files, parallel to the
/// per-subtree directory prefixes (`_home/`, `_xdg/`, `_app/`, `_lib/`).
///
/// Each prefix opts a single file out of the default-rule pack
/// namespacing. The file's name minus the prefix is the deploy-side
/// filename — `home.X` adds the conventional `.` so `home.bashrc`
/// becomes `.bashrc`; the others use the literal remainder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FilePrefix {
    /// `home.X` → `$HOME/.X`
    Home,
    /// `app.X` → `<app_support_dir>/X`
    App,
    /// `xdg.X` → `$XDG_CONFIG_HOME/X`
    Xdg,
    /// `lib.X` → `$HOME/Library/X` (macOS only; Skip elsewhere)
    Lib,
}

/// Recognized file-level prefixes, matched in this order. Mutually
/// exclusive — a filename has at most one such prefix.
const FILE_PREFIXES: &[(&str, FilePrefix)] = &[
    ("home.", FilePrefix::Home),
    ("app.", FilePrefix::App),
    ("xdg.", FilePrefix::Xdg),
    ("lib.", FilePrefix::Lib),
];

/// Recognized subtree directory prefixes (used by both the resolver and
/// the routing-conflict detector). Each entry is `<dir>/` so the same
/// strings catch a top-level directory match (`_app`) and any nested
/// path under it (`_app/Code/...`).
const DIR_PREFIXES: &[&str] = &["_home/", "_xdg/", "_app/", "_lib/"];

/// Top-level dir names that are exactly the routing prefix (no trailing
/// rest). Used for protected-paths-style normalization in
/// [`dir_intents`] and for routing-conflict detection.
const DIR_PREFIX_BARE: &[&str] = &["_home", "_xdg", "_app", "_lib"];

/// Strip a recognized file-level prefix from a top-level filename.
///
/// Returns the matched prefix and the remainder (everything after the
/// dot). Only applies to top-level files (no `/` in path); nested files
/// keep the prefix as a literal name component.
///
/// Empty remainders (e.g. the literal filename `"home."`) return `None`
/// so the file falls through to the default rule rather than targeting
/// a bare directory root (`$HOME/.`).
fn strip_file_prefix(rel_path: &str) -> Option<(FilePrefix, &str)> {
    if rel_path.contains('/') {
        return None;
    }
    for (lit, kind) in FILE_PREFIXES {
        if let Some(rest) = rel_path.strip_prefix(lit) {
            if !rest.is_empty() {
                return Some((*kind, rest));
            }
        }
    }
    None
}

/// True when `rel_path` carries any filesystem-naming routing prefix —
/// either a file-level prefix at the top level, or a subtree directory
/// prefix anywhere from the pack root.
///
/// Used by [`check_routing_conflict`] to detect when a `[symlink.targets]`
/// entry duplicates a routing intent already expressed by the file's
/// name on disk.
fn has_routing_prefix(rel_path: &str) -> bool {
    if strip_file_prefix(rel_path).is_some() {
        return true;
    }
    DIR_PREFIXES.iter().any(|p| rel_path.starts_with(p)) || DIR_PREFIX_BARE.contains(&rel_path)
}

/// Refuse to resolve a file that has both a `[symlink.targets]` entry
/// and a filesystem-naming routing prefix.
///
/// Two ways to say where one file goes is bug-bait — silent precedence
/// would mean the user reads the filename, expects one destination, and
/// gets the config-side one (or vice versa). The right move is to
/// surface both sources and ask the user to pick one.
fn check_routing_conflict(pack: &str, rel_path: &str, config: &HandlerConfig) -> Result<()> {
    let Some(target) = config.targets.get(rel_path) else {
        return Ok(());
    };
    if !has_routing_prefix(rel_path) {
        return Ok(());
    }
    Err(crate::DodotError::RoutingOverrideConflict {
        pack: pack.into(),
        rel_path: rel_path.into(),
        config_target: target.clone(),
    })
}

/// Outcome of resolving a single symlink target.
///
/// Most rules return a concrete path. The `_lib/` prefix on non-macOS
/// platforms returns `Skip` so the handler emits a soft warning and
/// drops the intent — the pack stays valid and other entries deploy
/// normally. See `docs/proposals/macos-paths.lex` §4.2.
#[derive(Debug, Clone)]
pub(crate) enum Resolution {
    /// Deploy at the given path.
    Path(PathBuf),
    /// Skip this entry; the caller surfaces a warning out-of-band via
    /// [`Handler::warnings_for_matches`]. The `reason` field carries a
    /// human-readable explanation for callers (and future diagnostics)
    /// that want it inline.
    Skip {
        #[allow(dead_code)]
        reason: String,
    },
}

/// Resolve the target path for a symlink.
///
/// `pack` is the pack name; it namespaces the default XDG target so
/// `pack vim/vimrc` deploys under `$XDG_CONFIG_HOME/vim/vimrc` rather
/// than `$XDG_CONFIG_HOME/vimrc`.
///
/// See the module-level docs for the full priority ladder. This thin
/// wrapper unwraps the [`Resolution`] back to a `PathBuf` for callers
/// that only ever care about deployable rules; sites that need to
/// honor `Skip` (the `_lib/` non-macOS branch) should call
/// [`resolve_target_full`] directly.
pub(crate) fn resolve_target(
    pack: &str,
    rel_path: &str,
    config: &HandlerConfig,
    paths: &dyn Pather,
) -> PathBuf {
    match resolve_target_full(pack, rel_path, config, paths) {
        Resolution::Path(p) => p,
        Resolution::Skip { .. } => {
            // Skip-routed entries still need *some* PathBuf for the few
            // call sites that ignore the skip channel (mostly tests). A
            // production caller goes through `resolve_target_full` and
            // never sees this branch.
            paths.xdg_config_home().to_path_buf()
        }
    }
}

/// Same as [`resolve_target`] but exposes the full [`Resolution`]
/// outcome, including the `Skip` variant produced by `_lib/` on
/// non-macOS platforms.
pub(crate) fn resolve_target_full(
    pack: &str,
    rel_path: &str,
    config: &HandlerConfig,
    paths: &dyn Pather,
) -> Resolution {
    // Strip any `NNN-` ordering prefix from the pack name before
    // computing the deployed path. The pack's *display name* — not its
    // on-disk directory name — is what the user expects in
    // `~/.config/<pack>/`. A pack `010-nvim/init.lua` deploys to
    // `~/.config/nvim/init.lua` (which is where `nvim` actually reads
    // its config), not `~/.config/010-nvim/init.lua`.
    let pack = crate::packs::display_name_for(pack);
    let home = paths.home_dir();
    let xdg_config = paths.xdg_config_home();
    let app_support = paths.app_support_dir();

    // Priority 0: Custom target override from [symlink.targets]
    if let Some(target) = config.targets.get(rel_path) {
        if target.starts_with('/') {
            // Absolute path — use as-is
            return Resolution::Path(PathBuf::from(target));
        }
        // Relative path — resolve from XDG_CONFIG_HOME
        return Resolution::Path(xdg_config.join(target));
    }

    // Priority 1: file-level prefixes (per-file opt-in, top-level only).
    // Each strips the prefix and routes the remainder under a fixed
    // root, skipping pack namespacing — parallel to the directory
    // prefixes at Priority 2.
    if let Some((kind, rest)) = strip_file_prefix(rel_path) {
        return match kind {
            FilePrefix::Home => Resolution::Path(home.join(format!(".{rest}"))),
            FilePrefix::App => Resolution::Path(app_support.join(rest)),
            FilePrefix::Xdg => Resolution::Path(xdg_config.join(rest)),
            FilePrefix::Lib => {
                if cfg!(target_os = "macos") {
                    Resolution::Path(home.join("Library").join(rest))
                } else {
                    Resolution::Skip {
                        reason: format!("lib.{rest} — macOS-only path, skipping on this platform"),
                    }
                }
            }
        };
    }

    // Priority 2: Explicit directory-prefix escape hatches.
    // _home/<rest> → $HOME/.<rest> (raw, no pack namespace)
    // _xdg/<rest>  → $XDG_CONFIG_HOME/<rest> (raw, no pack namespace)
    // _app/<rest>  → <app_support_dir>/<rest> (raw, no pack namespace)
    // _lib/<rest>  → $HOME/Library/<rest> (macOS only; warn elsewhere)
    if let Some(stripped) = rel_path.strip_prefix("_home/") {
        let parts: Vec<&str> = stripped.split('/').collect();
        if let Some(first) = parts.first() {
            if !first.is_empty() && !first.starts_with('.') {
                let mut new_parts = vec![format!(".{first}")];
                new_parts.extend(parts[1..].iter().map(|s| s.to_string()));
                return Resolution::Path(home.join(new_parts.join("/")));
            }
        }
        return Resolution::Path(home.join(stripped));
    }

    if let Some(stripped) = rel_path.strip_prefix("_xdg/") {
        return Resolution::Path(xdg_config.join(stripped));
    }

    if let Some(stripped) = rel_path.strip_prefix("_app/") {
        return Resolution::Path(app_support.join(stripped));
    }

    if let Some(stripped) = rel_path.strip_prefix("_lib/") {
        if cfg!(target_os = "macos") {
            return Resolution::Path(home.join("Library").join(stripped));
        }
        return Resolution::Skip {
            reason: format!("_lib/{stripped} — macOS-only path, skipping on this platform"),
        };
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
            return Resolution::Path(result);
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
        return Resolution::Path(home.join(dotted));
    }

    // Priority 4: force_app — curated GUI-app folders that route to
    // `<app_support_dir>/<first-segment>/<rest>` without a `_app/`
    // prefix. Mirrors `force_home` semantics: first segment compared
    // case-sensitively (Library folder names *are* case-sensitive).
    if is_force_app(rel_path, &config.force_app) {
        return Resolution::Path(app_support.join(rel_path));
    }

    // Priority 5: app_aliases — pack-level rewrite of the default rule.
    // When the pack name appears in the alias map, route the deploy
    // through `<app_support_dir>/<alias>/<rel_path>` instead of the
    // XDG default. Aliases compose: higher-priority rules above (for
    // `home.X`, `_home/`, `_xdg/`, `_app/`, `force_home`, `force_app`)
    // already returned, so by the time we get here we're in the default
    // bucket and the alias is the right rewrite to apply.
    if let Some(alias) = config.app_aliases.get(pack) {
        return Resolution::Path(app_support.join(alias).join(rel_path));
    }

    // Priority 6: Default — $XDG_CONFIG_HOME/<pack>/<rel_path>
    //
    // The pack name namespaces every entry by default so common modern
    // tools (nvim, helix, ghostty, …) work out of the box without
    // requiring `pack/program/` doubled paths. The escape hatches above
    // cover legacy `$HOME` tools and any user-specified overrides.
    Resolution::Path(xdg_config.join(pack).join(rel_path))
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

/// Check if a path matches any `force_app` entry.
///
/// Matching is *case-sensitive* on the first path segment — Library
/// folder names on macOS are case-sensitive, and `Code` (VS Code) must
/// not also match a hypothetical `code` CLI tool's `~/.config/code/`
/// directory. See `docs/proposals/macos-paths.lex` §3.4.
fn is_force_app(rel_path: &str, force_app: &[String]) -> bool {
    let first_segment = rel_path.split('/').next().unwrap_or(rel_path);
    force_app.iter().any(|entry| entry == first_segment)
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

    // ── Priority 2c: _app/ prefix ───────────────────────────────

    #[test]
    fn app_prefix_routes_to_app_support_root() {
        // _app/<rest> deploys raw under app_support_dir, no pack
        // namespace. The pack-relative path mirrors the on-disk
        // Application Support tree exactly.
        let config = HandlerConfig::default();
        let target = resolve_target(
            "macapps",
            "_app/Code/User/settings.json",
            &config,
            &test_pather(),
        );
        assert_eq!(
            target,
            PathBuf::from("/home/alice/Library/Application Support/Code/User/settings.json")
        );
    }

    #[test]
    fn app_prefix_outranks_default() {
        // A pack literally named `Code` with `_app/Code/x` is covered by
        // Priority 2c — the default rule (Priority 6) never sees it.
        let config = HandlerConfig::default();
        let target = resolve_target("Code", "_app/Code/x", &config, &test_pather());
        assert_eq!(
            target,
            PathBuf::from("/home/alice/Library/Application Support/Code/x")
        );
    }

    // ── Priority 2d: _lib/ prefix (macOS only) ──────────────────

    #[test]
    fn lib_prefix_resolution_full_returns_skip_on_non_macos() {
        // The `_lib/` prefix is macOS-only. On every other host the
        // resolver returns `Resolution::Skip` so the symlink handler
        // can omit the intent and surface a soft warning. The test is
        // gated on `cfg!(target_os = "macos")` for the positive case
        // (the skip branch is the *only* branch on Linux CI).
        let config = HandlerConfig::default();
        let resolution = resolve_target_full(
            "macapps",
            "_lib/LaunchAgents/com.example.foo.plist",
            &config,
            &test_pather(),
        );
        if cfg!(target_os = "macos") {
            match resolution {
                Resolution::Path(p) => assert_eq!(
                    p,
                    PathBuf::from("/home/alice/Library/LaunchAgents/com.example.foo.plist")
                ),
                Resolution::Skip { reason } => {
                    panic!("expected Path on macOS, got Skip({reason})")
                }
            }
        } else {
            assert!(
                matches!(resolution, Resolution::Skip { .. }),
                "_lib/ on non-macOS must skip; got {resolution:?}"
            );
        }
    }

    // ── Priority 4: force_app ───────────────────────────────────

    #[test]
    fn force_app_routes_first_segment_to_app_support() {
        // `force_app = ["Code"]` makes a top-level `Code/...` entry
        // route to <app_support_dir>/Code/... without a `_app/` prefix
        // in the pack tree.
        let config = HandlerConfig {
            force_app: vec!["Code".into()],
            ..HandlerConfig::default()
        };
        let target = resolve_target(
            "macapps",
            "Code/User/settings.json",
            &config,
            &test_pather(),
        );
        assert_eq!(
            target,
            PathBuf::from("/home/alice/Library/Application Support/Code/User/settings.json")
        );
    }

    #[test]
    fn force_app_is_case_sensitive() {
        // `Code` ≠ `code`. Library folder names are case-sensitive on
        // macOS, and conflating `Code` (VS Code) with `code` (a CLI
        // tool's `~/.config/code/`) would route the latter wrong.
        let config = HandlerConfig {
            force_app: vec!["Code".into()],
            ..HandlerConfig::default()
        };
        let target = resolve_target("misc", "code/foo", &config, &test_pather());
        assert_eq!(target, PathBuf::from("/home/alice/.config/misc/code/foo"));
    }

    #[test]
    fn force_app_loses_to_explicit_app_prefix() {
        // Priority 2c (`_app/`) outranks Priority 4 (`force_app`). A
        // pack that mixes both gets the explicit prefix's routing.
        let config = HandlerConfig {
            force_app: vec!["Code".into()],
            ..HandlerConfig::default()
        };
        let target = resolve_target("misc", "_app/Code/x", &config, &test_pather());
        assert_eq!(
            target,
            PathBuf::from("/home/alice/Library/Application Support/Code/x")
        );
    }

    // ── Priority 5: app_aliases ─────────────────────────────────

    #[test]
    fn app_alias_reroutes_default_rule() {
        // Pack `vscode` aliased to `Code` deploys top-level files to
        // <app_support_dir>/Code/... instead of $XDG/vscode/...
        let mut aliases = std::collections::HashMap::new();
        aliases.insert("vscode".into(), "Code".into());
        let config = HandlerConfig {
            app_aliases: aliases,
            ..HandlerConfig::default()
        };
        let target = resolve_target("vscode", "User/settings.json", &config, &test_pather());
        assert_eq!(
            target,
            PathBuf::from("/home/alice/Library/Application Support/Code/User/settings.json")
        );
    }

    #[test]
    fn app_alias_loses_to_explicit_xdg_prefix() {
        // Aliases only modify the default rule (Priority 6). A
        // `_xdg/...` entry is Priority 2b and routes raw under XDG —
        // explicit user intent wins over the alias.
        let mut aliases = std::collections::HashMap::new();
        aliases.insert("vscode".into(), "Code".into());
        let config = HandlerConfig {
            app_aliases: aliases,
            ..HandlerConfig::default()
        };
        let target = resolve_target("vscode", "_xdg/Code/User/foo", &config, &test_pather());
        assert_eq!(target, PathBuf::from("/home/alice/.config/Code/User/foo"));
    }

    #[test]
    fn app_alias_loses_to_home_prefix() {
        // home.X (Priority 1) outranks alias-driven defaults — a
        // `home.foo` file in an aliased pack still routes to ~/.foo.
        let mut aliases = std::collections::HashMap::new();
        aliases.insert("vscode".into(), "Code".into());
        let config = HandlerConfig {
            app_aliases: aliases,
            ..HandlerConfig::default()
        };
        let target = resolve_target("vscode", "home.editorconfig", &config, &test_pather());
        assert_eq!(target, PathBuf::from("/home/alice/.editorconfig"));
    }

    #[test]
    fn app_alias_uses_pack_display_name() {
        // Pack ordering prefix is stripped before alias lookup, just
        // like for the default rule. `010-vscode` aliased as `vscode`
        // → `Code` still routes correctly.
        let mut aliases = std::collections::HashMap::new();
        aliases.insert("vscode".into(), "Code".into());
        let config = HandlerConfig {
            app_aliases: aliases,
            ..HandlerConfig::default()
        };
        let target = resolve_target("010-vscode", "settings.json", &config, &test_pather());
        assert_eq!(
            target,
            PathBuf::from("/home/alice/Library/Application Support/Code/settings.json")
        );
    }

    #[test]
    fn force_app_outranks_app_alias() {
        // `force_app` is Priority 4, `app_aliases` is Priority 5. If a
        // pack has an alias and a top-level entry whose first segment
        // is in force_app, the force_app routing wins.
        let mut aliases = std::collections::HashMap::new();
        aliases.insert("anything".into(), "AliasedFolder".into());
        let config = HandlerConfig {
            force_app: vec!["Cursor".into()],
            app_aliases: aliases,
            ..HandlerConfig::default()
        };
        let target = resolve_target("anything", "Cursor/x", &config, &test_pather());
        assert_eq!(
            target,
            PathBuf::from("/home/alice/Library/Application Support/Cursor/x")
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
    fn strip_file_prefix_unit() {
        assert_eq!(
            strip_file_prefix("home.bashrc"),
            Some((FilePrefix::Home, "bashrc"))
        );
        assert_eq!(
            strip_file_prefix("home.vimrc"),
            Some((FilePrefix::Home, "vimrc"))
        );
        assert_eq!(
            strip_file_prefix("app.config"),
            Some((FilePrefix::App, "config"))
        );
        assert_eq!(
            strip_file_prefix("xdg.mimeapps.list"),
            Some((FilePrefix::Xdg, "mimeapps.list"))
        );
        assert_eq!(
            strip_file_prefix("lib.com.example.plist"),
            Some((FilePrefix::Lib, "com.example.plist"))
        );

        assert_eq!(strip_file_prefix("vimrc"), None);
        assert_eq!(strip_file_prefix(".bashrc"), None);
        // Nested files keep prefixes literal — only top-level files opt in.
        assert_eq!(strip_file_prefix("sub/home.conf"), None);
        assert_eq!(strip_file_prefix("sub/app.json"), None);
        // Empty remainders fall through to the default rule rather than
        // targeting a bare directory root (`$HOME/.`, `<app>/`, …).
        assert_eq!(strip_file_prefix("home."), None);
        assert_eq!(strip_file_prefix("app."), None);
        assert_eq!(strip_file_prefix("xdg."), None);
        assert_eq!(strip_file_prefix("lib."), None);
    }

    /// A file literally named `home.` falls through the priority list to
    /// the pack-namespaced XDG default — never to `$HOME/.`. Regression
    /// for review item #3 on PR #49.
    #[test]
    fn literal_home_dot_filename_does_not_target_home_root() {
        let config = HandlerConfig::default();
        let target = resolve_target("misc", "home.", &config, &test_pather());
        assert_eq!(target, PathBuf::from("/home/alice/.config/misc/home."));
    }

    /// Same regression for the new file prefixes — bare `app.`, `xdg.`,
    /// `lib.` filenames must not target the routing root.
    #[test]
    fn literal_bare_file_prefix_filenames_do_not_target_root() {
        let config = HandlerConfig::default();
        for name in ["app.", "xdg.", "lib."] {
            let target = resolve_target("misc", name, &config, &test_pather());
            assert_eq!(
                target,
                PathBuf::from(format!("/home/alice/.config/misc/{name}")),
                "bare `{name}` should fall through to default rule"
            );
        }
    }

    // ── Priority 1: app./xdg./lib. file prefixes ────────────────

    #[test]
    fn app_prefix_routes_top_level_file_to_app_support_root() {
        // app.X is the per-file counterpart to the `_app/` directory
        // prefix. The rest of the filename deploys raw under
        // app_support_dir, with no pack namespacing.
        let config = HandlerConfig::default();
        let target = resolve_target("vscode", "app.settings.json", &config, &test_pather());
        assert_eq!(
            target,
            PathBuf::from("/home/alice/Library/Application Support/settings.json")
        );
    }

    #[test]
    fn xdg_prefix_routes_top_level_file_to_xdg_root() {
        // xdg.X is the per-file counterpart to the `_xdg/` directory
        // prefix — same skip-pack-namespace semantics.
        let config = HandlerConfig::default();
        let target = resolve_target("desktop", "xdg.mimeapps.list", &config, &test_pather());
        assert_eq!(target, PathBuf::from("/home/alice/.config/mimeapps.list"));
    }

    #[test]
    fn lib_prefix_routes_top_level_file_to_library_on_macos() {
        // lib.X is the per-file counterpart to the `_lib/` directory
        // prefix. macOS-only; non-macOS hosts surface a Skip and a
        // soft warning, parallel to `_lib/`.
        let config = HandlerConfig::default();
        let resolution = resolve_target_full(
            "macapps",
            "lib.com.example.foo.plist",
            &config,
            &test_pather(),
        );
        if cfg!(target_os = "macos") {
            match resolution {
                Resolution::Path(p) => assert_eq!(
                    p,
                    PathBuf::from("/home/alice/Library/com.example.foo.plist")
                ),
                Resolution::Skip { reason } => {
                    panic!("expected Path on macOS, got Skip({reason})")
                }
            }
        } else {
            assert!(
                matches!(resolution, Resolution::Skip { .. }),
                "lib.X on non-macOS must skip; got {resolution:?}"
            );
        }
    }

    #[test]
    fn file_prefixes_only_apply_at_top_level() {
        // Nested `app.X` / `xdg.X` / `lib.X` paths keep the prefix
        // literal — only top-level files opt in (parallel to home.X).
        let config = HandlerConfig::default();
        for name in ["app.settings.json", "xdg.mimeapps.list", "lib.foo.plist"] {
            let nested = format!("subdir/{name}");
            let target = resolve_target("misc", &nested, &config, &test_pather());
            assert_eq!(
                target,
                PathBuf::from(format!("/home/alice/.config/misc/{nested}")),
                "nested `{name}` must keep prefix literal"
            );
        }
    }

    #[test]
    fn lib_file_prefix_emits_warning_on_non_macos() {
        // The handler's `warnings_for_matches` surfaces the same
        // macOS-only soft notice for `lib.X` files as it does for the
        // `_lib/` directory prefix on every non-macOS host.
        let env = crate::testing::TempEnvironment::builder()
            .pack("macapps")
            .file("lib.com.example.foo.plist", "# stub plist")
            .done()
            .build();

        let m = RuleMatch {
            relative_path: PathBuf::from("lib.com.example.foo.plist"),
            absolute_path: env.dotfiles_root.join("macapps/lib.com.example.foo.plist"),
            pack: "macapps".into(),
            handler: HANDLER_SYMLINK.into(),
            is_dir: false,
            options: std::collections::HashMap::new(),
            preprocessor_source: None,
        };
        let handler = SymlinkHandler;
        let config = HandlerConfig::default();
        let warnings =
            handler.warnings_for_matches(std::slice::from_ref(&m), &config, env.paths.as_ref());

        if cfg!(target_os = "macos") {
            assert!(
                warnings.is_empty(),
                "lib.X should not warn on macOS; got {warnings:?}"
            );
            let intents = handler
                .to_intents(&[m], &config, env.paths.as_ref(), env.fs.as_ref())
                .unwrap();
            assert_eq!(intents.len(), 1);
        } else {
            assert_eq!(warnings.len(), 1, "expected one warning, got {warnings:?}");
            assert!(
                warnings[0].contains("macOS-only path"),
                "warning text should mention macOS-only: {warnings:?}"
            );
            let intents = handler
                .to_intents(&[m], &config, env.paths.as_ref(), env.fs.as_ref())
                .unwrap();
            assert!(
                intents.is_empty(),
                "lib.X on non-macOS must not emit Link intents: {intents:?}"
            );
        }
    }

    // ── Routing-override conflicts ──────────────────────────────

    #[test]
    fn targets_plus_home_prefix_is_a_conflict() {
        // `[symlink.targets]` and the `home.` filename prefix both
        // declare where one file goes — refuse to silently let one win.
        let mut config = HandlerConfig::default();
        config
            .targets
            .insert("home.bashrc".into(), "/etc/bashrc".into());
        let env = crate::testing::TempEnvironment::builder()
            .pack("shell")
            .file("home.bashrc", "# bash")
            .done()
            .build();
        let m = RuleMatch {
            relative_path: PathBuf::from("home.bashrc"),
            absolute_path: env.dotfiles_root.join("shell/home.bashrc"),
            pack: "shell".into(),
            handler: HANDLER_SYMLINK.into(),
            is_dir: false,
            options: std::collections::HashMap::new(),
            preprocessor_source: None,
        };
        let err = SymlinkHandler
            .to_intents(&[m], &config, env.paths.as_ref(), env.fs.as_ref())
            .unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("home.bashrc"), "msg: {msg}");
        assert!(msg.contains("/etc/bashrc"), "msg: {msg}");
        assert!(msg.contains("shell"), "msg: {msg}");
    }

    #[test]
    fn targets_plus_directory_prefix_is_a_conflict() {
        // Same conflict for subtree prefixes: `_xdg/foo` + a
        // `[symlink.targets]` entry for the same path errors out.
        let mut config = HandlerConfig::default();
        config
            .targets
            .insert("_xdg/ghostty/config".into(), "/etc/ghostty.conf".into());
        let env = crate::testing::TempEnvironment::builder()
            .pack("term")
            .file("_xdg/ghostty/config", "")
            .done()
            .build();
        let m = RuleMatch {
            relative_path: PathBuf::from("_xdg/ghostty/config"),
            absolute_path: env.dotfiles_root.join("term/_xdg/ghostty/config"),
            pack: "term".into(),
            handler: HANDLER_SYMLINK.into(),
            is_dir: false,
            options: std::collections::HashMap::new(),
            preprocessor_source: None,
        };
        let err = SymlinkHandler
            .to_intents(&[m], &config, env.paths.as_ref(), env.fs.as_ref())
            .unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("_xdg/ghostty/config"), "msg: {msg}");
    }

    #[test]
    fn targets_without_prefix_is_not_a_conflict() {
        // `[symlink.targets]` for a plain (non-prefixed) filename keeps
        // working — that's the canonical use case for the override.
        let mut config = HandlerConfig::default();
        config
            .targets
            .insert("misterious.conf".into(), "/var/etc/misterious.conf".into());
        let env = crate::testing::TempEnvironment::builder()
            .pack("etc")
            .file("misterious.conf", "")
            .done()
            .build();
        let m = RuleMatch {
            relative_path: PathBuf::from("misterious.conf"),
            absolute_path: env.dotfiles_root.join("etc/misterious.conf"),
            pack: "etc".into(),
            handler: HANDLER_SYMLINK.into(),
            is_dir: false,
            options: std::collections::HashMap::new(),
            preprocessor_source: None,
        };
        let intents = SymlinkHandler
            .to_intents(&[m], &config, env.paths.as_ref(), env.fs.as_ref())
            .expect("plain target overrides without prefix should resolve");
        assert_eq!(intents.len(), 1);
        if let HandlerIntent::Link { user_path, .. } = &intents[0] {
            assert_eq!(user_path, &PathBuf::from("/var/etc/misterious.conf"));
        }
    }

    #[test]
    fn has_routing_prefix_unit() {
        // File-level prefixes
        assert!(has_routing_prefix("home.bashrc"));
        assert!(has_routing_prefix("app.settings.json"));
        assert!(has_routing_prefix("xdg.mimeapps.list"));
        assert!(has_routing_prefix("lib.com.example.plist"));
        // Subtree prefixes
        assert!(has_routing_prefix("_home/vimrc"));
        assert!(has_routing_prefix("_xdg/ghostty/config"));
        assert!(has_routing_prefix("_app/Code/User/settings.json"));
        assert!(has_routing_prefix("_lib/LaunchAgents/foo.plist"));
        // Bare prefix dir names too — the catchall scanner can match
        // those at the top level when nothing else inside them does.
        assert!(has_routing_prefix("_home"));
        assert!(has_routing_prefix("_app"));
        // Plain names — no prefix.
        assert!(!has_routing_prefix("vimrc"));
        assert!(!has_routing_prefix("subdir/home.conf"));
        // Empty-rest forms fall through, so they don't carry routing
        // intent for conflict purposes.
        assert!(!has_routing_prefix("home."));
        assert!(!has_routing_prefix("app."));
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

    // ── _lib/ warnings emission ─────────────────────────────────

    #[test]
    fn lib_prefix_emits_warning_on_non_macos() {
        // The symlink handler's `warnings_for_matches` surfaces the
        // soft "_lib/<rest> — macOS-only path, skipping" notice on
        // every non-macOS host. On macOS the rule resolves as a real
        // path, so the warnings list stays empty.
        let env = crate::testing::TempEnvironment::builder()
            .pack("macapps")
            .file("_lib/LaunchAgents/com.example.foo.plist", "# stub plist")
            .done()
            .build();

        let m = RuleMatch {
            relative_path: PathBuf::from("_lib/LaunchAgents/com.example.foo.plist"),
            absolute_path: env
                .dotfiles_root
                .join("macapps/_lib/LaunchAgents/com.example.foo.plist"),
            pack: "macapps".into(),
            handler: HANDLER_SYMLINK.into(),
            is_dir: false,
            options: std::collections::HashMap::new(),
            preprocessor_source: None,
        };
        let handler = SymlinkHandler;
        let config = HandlerConfig::default();
        let warnings =
            handler.warnings_for_matches(std::slice::from_ref(&m), &config, env.paths.as_ref());

        if cfg!(target_os = "macos") {
            assert!(
                warnings.is_empty(),
                "_lib/ should not warn on macOS; got {warnings:?}"
            );
            // And the intent is generated as a real Link.
            let intents = handler
                .to_intents(&[m], &config, env.paths.as_ref(), env.fs.as_ref())
                .unwrap();
            assert_eq!(intents.len(), 1);
        } else {
            assert_eq!(warnings.len(), 1, "expected one warning, got {warnings:?}");
            assert!(
                warnings[0].contains("macOS-only path"),
                "warning text should mention macOS-only: {warnings:?}"
            );
            // And the intent is *omitted* — `to_intents` skips it.
            let intents = handler
                .to_intents(&[m], &config, env.paths.as_ref(), env.fs.as_ref())
                .unwrap();
            assert!(
                intents.is_empty(),
                "_lib/ on non-macOS must not emit Link intents: {intents:?}"
            );
        }
    }

    #[test]
    fn top_level_app_and_lib_dirs_force_per_file_mode() {
        // Regression: a top-level `_app` or `_lib` directory MUST NOT
        // be wholesale-linked — that would bake the prefix into the
        // deploy path (`~/.config/<pack>/_app/...`). Same per-file
        // forcing as `_home` and `_xdg`. Discovered by the bats e2e
        // suite for `_app/`.
        for prefix in ["_app", "_lib"] {
            let env = crate::testing::TempEnvironment::builder()
                .pack("macapps")
                .file(&format!("{prefix}/Code/x.json"), "x")
                .done()
                .build();
            let m = build_dir_match(&env, "macapps", prefix);
            let handler = SymlinkHandler;
            let intents = handler
                .to_intents(
                    &[m],
                    &HandlerConfig::default(),
                    env.paths.as_ref(),
                    env.fs.as_ref(),
                )
                .unwrap();
            // _lib/ on non-macOS produces no intent (skipped). On
            // macOS it produces 1 link. _app/ produces 1 link
            // everywhere (collapses to xdg on Linux but still emits).
            let expected = match prefix {
                "_lib" if !cfg!(target_os = "macos") => 0,
                _ => 1,
            };
            assert_eq!(
                intents.len(),
                expected,
                "prefix={prefix}: expected {expected} intents, got {intents:?}"
            );
            // The user_path should NOT contain the literal prefix —
            // the resolver stripped it. (Skip this check when no
            // intents were produced.)
            if let Some(HandlerIntent::Link { user_path, .. }) = intents.first() {
                assert!(
                    !user_path.to_string_lossy().contains(&format!("/{prefix}/")),
                    "prefix={prefix} leaked into deploy path: {}",
                    user_path.display()
                );
            }
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
