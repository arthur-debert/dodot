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
mod tests;
