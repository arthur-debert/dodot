//! Source-path inference for `dodot adopt`.
//!
//! `adopt` accepts a deployed path (e.g. `~/.config/nvim/init.lua`,
//! `~/.vimrc`, `~/.weechat/`) and figures out:
//!
//!   1. **Which pack** the source belongs in (e.g. `nvim`, `vim`, `weechat`),
//!      so a freshly-adopted file lands in the right pack subtree without
//!      the user spelling the name out.
//!   2. **Where inside the pack** the file should live so that re-deploying
//!      via `dodot up` lands the symlink back at the original source path.
//!      This is the inverse of `handlers::symlink::resolve_target`'s
//!      priority ladder; getting it wrong breaks the round-trip property.
//!
//! The inference is *not* the resolver's inverse in the strict sense —
//! the resolver maps `(pack, in_pack_path) → deployed_path`, and there
//! are typically multiple pack-relative paths that round-trip to the
//! same deployed path (e.g. `home.vimrc` in any pack and `_home/vimrc`
//! in any pack both deploy to `~/.vimrc`). Inference picks the *most
//! ergonomic* option per source root:
//!
//! - `$XDG_CONFIG_HOME/<X>/<rest>` — pack `<X>`, in-pack `<rest>` (round-trips
//!   via the resolver's default rule, which namespaces under XDG by pack).
//! - `$HOME/.<X>` (file) — in-pack `home.<X>` (round-trips via Priority 1).
//! - `$HOME/.<X>/...` (dir) — in-pack `_home/<X>/...` (round-trips via
//!   Priority 2's `_home/` directory prefix).
//! - `~/Library/Application Support/<X>/<rest>` (macOS, future) — pack
//!   `<X>`, in-pack `_app/<X>/<rest>` (round-trips via Priority 2's
//!   `_app/` prefix per `docs/proposals/macos-paths.lex`).
//!
//! ## Override semantics (`--into <pack>`)
//!
//! When the user supplies `--into <Y>` and `<Y>` differs from the
//! naturally-inferred pack name, the in-pack path must change too — the
//! default rule (`$XDG/<pack>/<rest>`) would otherwise route the file to
//! the wrong place. Inference returns *both* the natural in-pack path
//! and the override-aware variant, so the caller can pick based on
//! whether `--into` was supplied:
//!
//! - XDG source, override differs: use `_xdg/<X>/<rest>` so the
//!   `_xdg/` prefix (Priority 2) bypasses pack-namespacing.
//! - HOME source: the `home.X` and `_home/X/` prefixes are pack-name
//!   independent already, so the override-aware path equals the natural
//!   path. Override changes only the *pack* the file lands in.
//! - AppSupport source, override differs (future): use `_app/<X>/<rest>`.
//!
//! ## Why HOME sources don't infer a pack name
//!
//! `~/.config/<X>/...` carries pack structure in the path: the first
//! segment under XDG *is* the natural pack name. `~/.<X>` does not — a
//! file like `~/.bashrc` could plausibly belong in a `shell`, `bash`, or
//! `dotfiles` pack depending on the user's organization. Rather than
//! guess (and saddle the user with pack names like `bashrc` or
//! `gitconfig`), inference declines and the caller requires `--into`.
//!
//! The only HOME-source ergonomic concession is the `force_home` list:
//! entries there get the bare pack-relative name (no `home.` or `_home/`
//! prefix) because Priority 3 routes them back without one.

use std::path::{Component, Path, PathBuf};

use crate::paths::Pather;

/// Outcome of inferring how to adopt a single source path.
///
/// The `_natural` and `_override` fields encode the same target two ways:
/// once for the case where the pack name matches the source-root's
/// natural inference (no `_xdg/`/`_app/` prefix needed), and once for the
/// case where the user supplied a different pack via `--into`.
#[derive(Debug, Clone)]
pub(crate) struct InferredTarget {
    /// Pack name derived from the source path itself, if the source
    /// root carries pack structure. `None` when the caller must supply
    /// `--into <pack>` (HOME root) or when the source is at the very
    /// top of XDG with no app subdirectory.
    pub natural_pack: Option<String>,
    /// In-pack path to use when the chosen pack name equals
    /// `natural_pack` (or when override-aware encoding doesn't differ
    /// for this root, e.g. HOME).
    pub in_pack_natural: PathBuf,
    /// In-pack path to use when the chosen pack name *differs* from
    /// the source's naturally-inferred name. For XDG this prepends
    /// `_xdg/<original-pack-segment>/` to keep round-trip; for HOME this
    /// is identical to `in_pack_natural` (already prefix-encoded).
    pub in_pack_override: PathBuf,
    /// Which root the source was matched against. Drives error
    /// messages and the small set of root-specific behaviors (e.g.
    /// directory-source expansion under XDG/AppSupport).
    pub source_root: SourceRoot,
    /// `true` when the source is the *pack-root directory itself* under
    /// XDG/AppSupport (e.g. `~/.config/nvim/`) — the caller should
    /// enumerate its children and adopt each one as its own plan, so
    /// each child becomes a top-level pack entry rather than the dir
    /// becoming one big symlink-to-pack-root.
    pub expand_children: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SourceRoot {
    /// `$HOME` (e.g. `/Users/alice` or `/home/alice`).
    Home,
    /// `$XDG_CONFIG_HOME` (e.g. `~/.config`, or whatever `XDG_CONFIG_HOME` points at).
    XdgConfig,
    /// `~/Library/Application Support` on macOS, or whatever
    /// `Pather::app_support_dir()` returns. On Linux this collapses to
    /// `xdg_config_home`, so the AppSupport arm is unreachable in
    /// practice on non-macOS hosts (XDG matches first via
    /// longest-prefix order).
    AppSupport,
    /// `$HOME/Library` on macOS — the parent of
    /// `Application Support`, covering `Preferences/`, `LaunchAgents/`,
    /// `Fonts/`, `Services/`, etc. Adopt sources here round-trip via
    /// the `_lib/<rest>` priority-2d prefix. Match order in
    /// [`infer_target`] places this AFTER `AppSupport` so the more
    /// specific prefix wins for `Application Support/...` paths.
    Library,
}

/// Why inference declined to produce a target.
#[derive(Debug)]
pub(crate) enum InferenceError {
    /// Source is not under any recognized adopt-source root.
    UnrecognizedRoot { hint_roots: Vec<PathBuf> },
    /// Source is a direct child of `$HOME` but isn't a dotfile, so
    /// there's no automatic round-trip path. The user can rename to
    /// dotted form or use `[symlink.targets]` for an explicit override.
    NonDottedHome { stripped: String },
    /// Source is a top-level entry directly under `$XDG_CONFIG_HOME`
    /// (no app subdirectory). XDG-root files have no natural pack
    /// structure to mine; require `--into` and use `_xdg/` manually.
    LooseXdgFile,
    /// Source is the XDG root itself (e.g. `~/.config/`). Too broad
    /// to adopt as a single unit.
    XdgRootItself,
    /// Source is the HOME root itself. Refused.
    HomeRootItself,
    /// macOS sandboxed-app container. Containers/<bundle>/Data/Library/...
    /// is system-managed; adoption is refused per
    /// `docs/proposals/macos-paths.lex` §7.3.
    SandboxedContainer,
    /// Source is `$HOME/Library` itself. Too broad to adopt as one
    /// unit — caller should pick a subdirectory.
    LibraryRootItself,
}

impl std::fmt::Display for InferenceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InferenceError::UnrecognizedRoot { hint_roots } => {
                let roots: Vec<String> =
                    hint_roots.iter().map(|p| p.display().to_string()).collect();
                write!(
                    f,
                    "source is outside any recognized adopt root (expected under one of: {}). \
                     Move the file under one of these locations first, or copy it into a \
                     pack manually and use `[symlink.targets]` for an absolute deploy path.",
                    roots.join(", ")
                )
            }
            InferenceError::NonDottedHome { stripped } => write!(
                f,
                "a non-dotted entry in $HOME has no automatic round-trip path \
                 under the post-#48 XDG default. Either rename to a dotted name \
                 (e.g. .{stripped}) before adopting, or copy into the pack \
                 manually and add a [symlink.targets] override pinning the \
                 deploy path."
            ),
            InferenceError::LooseXdgFile => write!(
                f,
                "loose file directly under $XDG_CONFIG_HOME has no pack structure \
                 to infer. Move it into a subdirectory (recommended) or pass \
                 --into <pack> and place the file manually as `_xdg/<name>` in \
                 that pack."
            ),
            InferenceError::XdgRootItself => write!(
                f,
                "$XDG_CONFIG_HOME itself is too broad to adopt — adopt individual \
                 application subdirectories (e.g. `~/.config/nvim/`) instead."
            ),
            InferenceError::HomeRootItself => {
                write!(f, "$HOME itself is too broad to adopt.")
            }
            InferenceError::SandboxedContainer => write!(
                f,
                "this is a sandboxed app's container; its config is not \
                 intended to be edited externally. dodot does not support \
                 adopting from ~/Library/Containers/."
            ),
            InferenceError::LibraryRootItself => write!(
                f,
                "$HOME/Library itself is too broad to adopt — pick a \
                 subdirectory like ~/Library/Preferences/ or \
                 ~/Library/LaunchAgents/."
            ),
        }
    }
}

/// Infer pack name and pack-relative path for a single adopt source.
///
/// `abs_source` must be an absolute, lexically-normalized path (no `..`,
/// no `.`). The caller is responsible for canonicalizing where needed —
/// inference works on the *path string* primarily so that `--no-follow`
/// can preserve a symlink source as the link itself rather than resolve
/// through to its target.
///
/// `is_dir` is the post-`--no-follow` directory flag: a symlink-to-dir
/// adopted with `--no-follow` is `is_dir=false` because we'll move the
/// link, not the target.
///
/// `force_home` is the merged `[symlink] force_home` list (root config +
/// pack config) — entries here use the bare pack-relative name, no
/// `home.X` / `_home/X/` prefix, to match the resolver's Priority 3
/// routing.
pub(crate) fn infer_target(
    abs_source: &Path,
    is_dir: bool,
    pather: &dyn Pather,
    force_home: &[String],
) -> Result<InferredTarget, InferenceError> {
    // Canonicalize roots and source for prefix matching. We canonicalize
    // because on macOS `/var` and `/private/var` are equivalent symlinks,
    // and `~` may itself be a symlinked path (e.g. `/home/alice` →
    // `/var/home/alice` on some distros). Mismatching real and symbolic
    // forms would fail the strip_prefix checks below.
    //
    // For the source we canonicalize the *parent* (if any) and re-join
    // the basename. Canonicalizing the source itself would follow a
    // symlink source — breaking `--no-follow` semantics — and fails
    // outright on a dangling symlink. The parent is always a real
    // directory: even for a dangling source link, the link itself lives
    // *somewhere*, and that somewhere has a canonical form.
    let canon_source = canonicalize_parent_keep_basename(abs_source);
    let canon_home = canonicalize_for_match(pather.home_dir());
    let canon_xdg = canonicalize_for_match(pather.xdg_config_home());
    let canon_app = canonicalize_for_match(pather.app_support_dir());

    // ── Sandboxed-container refusal ──────────────────────────────────
    //
    // Check this *before* any other root match. On macOS the canonical
    // `~/Library/Containers/` prefix lives under HOME, so without an
    // early exit we'd potentially treat it as a HOME-rooted source. The
    // refusal applies regardless of platform: `dodot adopt` shouldn't
    // touch container data on any OS, and on Linux the path simply
    // won't exist (no false positives).
    let containers_root = canon_home.join("Library").join("Containers");
    if canon_source.starts_with(&containers_root) {
        return Err(InferenceError::SandboxedContainer);
    }

    // ── AppSupport root (longest-prefix wins) ────────────────────────
    //
    // On macOS `app_support_dir` is `~/Library/Application Support` —
    // strictly more specific than HOME and disjoint from XDG. Match it
    // *before* XDG and HOME so a path like `~/Library/Application
    // Support/Code/User/settings.json` produces an AppSupport-rooted
    // inference instead of falling through to HOME and emitting a
    // "nested under $HOME" verdict.
    //
    // On Linux `app_support_dir` collapses to `xdg_config_home`, so
    // either: (a) `canon_app == canon_xdg`, in which case any source
    // under XDG will match here too — but `resolve_xdg_relative`
    // produces the right answer for that root, so we steer Linux
    // sources back through the XDG branch by checking equality first;
    // or (b) the user explicitly set `app_uses_library = false` on
    // macOS, which similarly collapses the two — same handling.
    if canon_app != canon_xdg {
        if canon_source == canon_app {
            return Err(InferenceError::XdgRootItself);
        }
        if let Ok(rel) = canon_source.strip_prefix(&canon_app) {
            return resolve_app_support_relative(rel, is_dir);
        }
    }

    // ── $HOME/Library root (macOS only, after AppSupport) ────────────
    //
    // `~/Library/` covers Preferences/, LaunchAgents/, Fonts/, etc.
    // Adopt sources here round-trip via the `_lib/<rest>` Priority 2d
    // prefix. Matched AFTER AppSupport so a path under
    // `~/Library/Application Support/` lands on the more specific
    // `_app/` encoding instead of the broader `_lib/Application Support/`.
    //
    // Gated on `cfg!(target_os = "macos")` to mirror the symlink
    // resolver: `_lib/` warns-and-skips on non-macOS at deploy time,
    // so producing `_lib/...` plans for Linux sources would just
    // generate guaranteed warnings on the next `up`. Letting Linux
    // users fall through to `UnrecognizedRoot` keeps adopt's "if the
    // file isn't somewhere I know how to deploy back to, refuse"
    // contract clean.
    if cfg!(target_os = "macos") {
        let library_root = canon_home.join("Library");
        if canon_source == library_root {
            return Err(InferenceError::LibraryRootItself);
        }
        if let Ok(rel) = canon_source.strip_prefix(&library_root) {
            return resolve_library_relative(rel, is_dir);
        }
    }

    // ── XDG_CONFIG_HOME root ─────────────────────────────────────────
    //
    // Match XDG *before* HOME because the default XDG config home is
    // `$HOME/.config` — it sits *inside* HOME, so longest-prefix-wins
    // requires checking the more-specific root first. Without this
    // ordering, `~/.config/nvim/init.lua` would match HOME and produce
    // a useless "nested under $HOME" inference.
    if canon_source == canon_xdg {
        return Err(InferenceError::XdgRootItself);
    }
    if let Ok(rel) = canon_source.strip_prefix(&canon_xdg) {
        return resolve_xdg_relative(rel, is_dir);
    }

    // ── HOME root ────────────────────────────────────────────────────
    if canon_source == canon_home {
        return Err(InferenceError::HomeRootItself);
    }
    if let Ok(rel) = canon_source.strip_prefix(&canon_home) {
        return resolve_home_relative(rel, is_dir, force_home);
    }

    // ── No root matched ──────────────────────────────────────────────
    let mut hint_roots = vec![canon_xdg, canon_home];
    if canon_app != hint_roots[0] && canon_app != hint_roots[1] {
        hint_roots.push(canon_app);
    }
    Err(InferenceError::UnrecognizedRoot { hint_roots })
}

/// Build inference output for a path relative to `$XDG_CONFIG_HOME`.
///
/// `rel` is the source path minus the canonical XDG root, so its first
/// component is what we treat as the natural pack name. Examples:
///
/// - `nvim/init.lua` → pack `nvim`, in-pack `init.lua`
/// - `nvim/lua/plugins/foo.lua` → pack `nvim`, in-pack `lua/plugins/foo.lua`
/// - `nvim` (a directory, sole component) → pack `nvim`, expand children
/// - `loose-file.toml` (a file, sole component) → refused (LooseXdgFile)
fn resolve_xdg_relative(rel: &Path, is_dir: bool) -> Result<InferredTarget, InferenceError> {
    let mut comps = rel.components();
    let first = match comps.next() {
        Some(Component::Normal(s)) => s.to_string_lossy().into_owned(),
        // No first component: rel is empty (handled by the `==` check
        // upstream) or starts with something we can't treat as a name.
        // Not reachable in practice given the upstream root match, but
        // we surface a refusal rather than panic if somehow it does.
        _ => return Err(InferenceError::XdgRootItself),
    };
    let rest_path: PathBuf = comps.as_path().to_path_buf();

    if rest_path.as_os_str().is_empty() {
        // Sole component case: the source IS one of XDG's top-level
        // children. A directory is a pack root we can expand; a file is
        // loose with no pack structure.
        if is_dir {
            return Ok(InferredTarget {
                natural_pack: Some(first.clone()),
                // Empty in-pack path: not used directly because
                // `expand_children = true` triggers per-child planning,
                // each of which derives its own in-pack path.
                in_pack_natural: PathBuf::new(),
                in_pack_override: PathBuf::from("_xdg").join(&first),
                source_root: SourceRoot::XdgConfig,
                expand_children: true,
            });
        } else {
            return Err(InferenceError::LooseXdgFile);
        }
    }

    // Nested case: `<pack>/<rest>`. The natural in-pack path is `<rest>`
    // (the resolver's default rule namespaces it back under the pack).
    // The override-aware path uses `_xdg/<pack>/<rest>` so the explicit
    // `_xdg/` prefix bypasses pack-namespacing — required when the
    // user picks a different pack via `--into`.
    Ok(InferredTarget {
        natural_pack: Some(first.clone()),
        in_pack_natural: rest_path.clone(),
        in_pack_override: PathBuf::from("_xdg").join(&first).join(&rest_path),
        source_root: SourceRoot::XdgConfig,
        expand_children: false,
    })
}

/// Build inference output for a path relative to `app_support_dir`.
///
/// `~/Library/Application Support/<X>/<rest>` infers pack `<X>` and
/// in-pack path `_app/<X>/<rest>`. The `_app/` prefix is *mandatory*
/// even at natural pack name because the resolver's default rule
/// (Priority 6) routes `<pack>/<rel>` through `$XDG/<pack>/<rel>`, not
/// `<app_support_dir>/<pack>/<rel>` — without the prefix the
/// round-trip would land the file at `~/.config/<X>/...` on macOS,
/// which is not where `dodot adopt` picked it up. See
/// `docs/proposals/macos-paths.lex` §7.2.
///
/// The `app_aliases` form (declare `[symlink.app_aliases] <X> = "<X>"`
/// in the pack and use bare paths) is an alternative the user can opt
/// into manually; adopt's automatic output is the prefix form.
fn resolve_app_support_relative(
    rel: &Path,
    is_dir: bool,
) -> Result<InferredTarget, InferenceError> {
    let mut comps = rel.components();
    let first = match comps.next() {
        Some(Component::Normal(s)) => s.to_string_lossy().into_owned(),
        _ => return Err(InferenceError::XdgRootItself),
    };
    let rest_path: PathBuf = comps.as_path().to_path_buf();

    if rest_path.as_os_str().is_empty() {
        // Sole component: source IS the app's top-level Application
        // Support directory. A directory expands; a loose file is
        // refused (same shape as XDG's LooseXdgFile case).
        if is_dir {
            return Ok(InferredTarget {
                natural_pack: Some(first.clone()),
                in_pack_natural: PathBuf::from("_app").join(&first),
                in_pack_override: PathBuf::from("_app").join(&first),
                source_root: SourceRoot::AppSupport,
                expand_children: true,
            });
        } else {
            return Err(InferenceError::LooseXdgFile);
        }
    }

    // Nested case: `<X>/<rest>`. Both natural and override encodings
    // use the explicit `_app/<X>/...` prefix — the override flag
    // doesn't change anything for AppSupport because the natural
    // encoding is *already* prefixed (see §7.2 note).
    let in_pack = PathBuf::from("_app").join(&first).join(&rest_path);
    Ok(InferredTarget {
        natural_pack: Some(first.clone()),
        in_pack_natural: in_pack.clone(),
        in_pack_override: in_pack,
        source_root: SourceRoot::AppSupport,
        expand_children: false,
    })
}

/// Build inference output for a path relative to `$HOME/Library`.
///
/// Library-rooted sources (`~/Library/Preferences/...`,
/// `~/Library/LaunchAgents/...`, etc.) round-trip via the `_lib/<rest>`
/// Priority 2d prefix. They carry no useful pack-name structure —
/// filenames are typically reverse-DNS bundle IDs (`com.foo.bar.plist`)
/// — so inference returns `natural_pack = None` and the caller must
/// supply `--into <pack>`.
fn resolve_library_relative(rel: &Path, is_dir: bool) -> Result<InferredTarget, InferenceError> {
    if rel.as_os_str().is_empty() {
        return Err(InferenceError::LibraryRootItself);
    }
    // Refuse `~/Library/Containers/...` defensively — the early-exit in
    // [`infer_target`] already handles this, but bare-rel matching is a
    // backstop in case a caller wires up `resolve_library_relative`
    // directly in tests or future code paths.
    if rel.starts_with("Containers") {
        return Err(InferenceError::SandboxedContainer);
    }

    let in_pack = PathBuf::from("_lib").join(rel);
    Ok(InferredTarget {
        // No natural pack name to mine — bundle IDs and folder names
        // like `Preferences`, `LaunchAgents` are not pack-shaped.
        // Caller must supply `--into <pack>`.
        natural_pack: None,
        in_pack_natural: in_pack.clone(),
        in_pack_override: in_pack,
        source_root: SourceRoot::Library,
        // Expand top-level `~/Library/<subdir>/` directories (e.g.
        // `~/Library/LaunchAgents/`) so each child becomes its own
        // `_lib/<subdir>/<child>` plan instead of adopting the
        // directory itself as one big symlinked subtree. Deeper
        // nested directories (e.g. `~/Library/Foo/Bar/`) stay as a
        // single adoption unit — the user opted into the deeper
        // path, so we don't second-guess them.
        expand_children: is_dir
            && rel
                .components()
                .next()
                .is_some_and(|c| matches!(c, Component::Normal(_)))
            && rel.components().count() == 1,
    })
}

/// Build inference output for a path relative to `$HOME`.
///
/// HOME-rooted sources don't carry pack structure, so this returns
/// `natural_pack = None` and the caller must require `--into <pack>`.
/// What inference *does* compute is the in-pack path: the existing
/// dotfile conventions (`home.X` for files, `_home/X/` for dirs)
/// preserve round-trip regardless of the chosen pack name.
fn resolve_home_relative(
    rel: &Path,
    is_dir: bool,
    force_home: &[String],
) -> Result<InferredTarget, InferenceError> {
    let mut comps = rel.components();
    let first = match comps.next() {
        Some(Component::Normal(s)) => s.to_string_lossy().into_owned(),
        _ => return Err(InferenceError::HomeRootItself),
    };

    if comps.next().is_some() {
        // The source is nested deeper under HOME than a direct child
        // (e.g. `~/Documents/notes.txt`). We don't have rules for these
        // — `~/Documents/`, `~/Desktop/`, etc. are user data, not
        // dotfiles. Refuse with "outside recognized roots" and seed
        // `hint_roots` with the *symbolic* root names so the rendered
        // error stays readable. Empty hint_roots produces awkward
        // "expected under one of: " text.
        return Err(InferenceError::UnrecognizedRoot {
            hint_roots: vec![PathBuf::from("$HOME"), PathBuf::from("$XDG_CONFIG_HOME")],
        });
    }

    // Delegate to the string-shaped helper so both inference and the
    // round-trip property test go through the same convention table.
    // `derive_home_in_pack` returns `force_home`-bare-name, `home.X`,
    // `_home/X`, or a "non-dotted refused" error — exactly the shape we
    // need here.
    let stripped = first.strip_prefix('.').unwrap_or(&first);
    let in_pack_str = derive_home_in_pack(&first, is_dir, force_home).map_err(|_| {
        InferenceError::NonDottedHome {
            stripped: stripped.to_string(),
        }
    })?;
    let in_pack = PathBuf::from(in_pack_str);

    Ok(InferredTarget {
        natural_pack: None,
        // HOME's `home.X` / `_home/X/` prefixes are pack-name-independent
        // — Priority 1 and 2 don't consult the pack name when routing —
        // so the override-aware in-pack path equals the natural one.
        in_pack_natural: in_pack.clone(),
        in_pack_override: in_pack,
        source_root: SourceRoot::Home,
        expand_children: false,
    })
}

/// Canonicalize a path for prefix matching, falling back to the input on
/// failure. Canonicalization may fail on a non-existent path; that's
/// fine — we want the lexical form in that case anyway.
fn canonicalize_for_match(p: &Path) -> PathBuf {
    std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf())
}

/// Canonicalize a source path's parent and re-join the basename, leaving
/// the source itself unfollowed.
///
/// This is the right shape for adopt's source paths: we need OS-level
/// path equivalence (e.g. macOS `/var` ↔ `/private/var`) on the
/// directory-prefix portion so root-membership checks work, but we must
/// *not* follow a symlink source — adopt's `--no-follow` mode treats
/// the link as the thing to move, not the link's target. Canonicalizing
/// only the parent gives us both: the prefix is real, and the basename
/// stays as the user wrote it.
///
/// If the parent can't be canonicalized (rare — it would mean the
/// source is at the filesystem root or the parent doesn't exist), we
/// fall back to the lexical input. The downstream prefix checks may
/// then fail to match a root, which surfaces as `UnrecognizedRoot` —
/// the same outcome the user would see for any path that genuinely
/// isn't under a recognized root.
fn canonicalize_parent_keep_basename(source: &Path) -> PathBuf {
    let parent = match source.parent() {
        Some(p) if !p.as_os_str().is_empty() => p,
        // Filesystem root or empty path: nothing to canonicalize.
        _ => return source.to_path_buf(),
    };
    let canon_parent = match std::fs::canonicalize(parent) {
        Ok(p) => p,
        Err(_) => return source.to_path_buf(),
    };
    match source.file_name() {
        Some(name) => canon_parent.join(name),
        None => canon_parent,
    }
}

/// Capitalization heuristic — does `name` look like a macOS GUI-app
/// folder name?
///
/// macOS GUI apps under `~/Library/Application Support/` follow a
/// strong naming pattern (uppercase letters, spaces, or reverse-DNS
/// segments) that distinguishes them from CLI-tool folders under
/// `~/.config/` (uniformly lowercase-hyphenated). We use this to drive
/// adopt's *suggestion* output — never the resolver, which stays
/// heuristic-free per `docs/proposals/macos-paths.lex` §8.1.
///
/// Returns `true` when:
///
/// - the name contains at least one uppercase letter (`Code`,
///   `Cursor`, `IntelliJ IDEA`), or
/// - the name contains a space (`Sublime Text 3`), or
/// - the name matches a reverse-DNS pattern: at least two
///   dot-separated segments where every segment is non-empty and
///   lowercase (`com.apple.dt.Xcode`, `dev.warp.Warp-Stable` —
///   the trailing "Warp-Stable" segment makes the latter qualify
///   under the *uppercase* clause already, but we keep the rDNS
///   check independent for fully-lowercase rDNS names).
pub(crate) fn is_gui_app_folder(name: &str) -> bool {
    if name.is_empty() {
        return false;
    }
    if name.chars().any(|c| c.is_ascii_uppercase()) {
        return true;
    }
    if name.contains(' ') {
        return true;
    }
    // Reverse-DNS: ≥2 dotted segments, every segment non-empty.
    let segments: Vec<&str> = name.split('.').collect();
    if segments.len() >= 2 && segments.iter().all(|s| !s.is_empty()) {
        return true;
    }
    false
}

/// Compute the in-pack path for a `$HOME/<file_name>` source.
///
/// Kept as a public-in-crate helper so the round-trip property test in
/// `commands::tests::pack_filename_round_trips_through_resolve_target`
/// can drive the same conventions inference uses (without paying the
/// cost of materializing a `Pather` or absolute paths).
///
/// Returns `Err(reason)` when the source has no automatic round-trip
/// path — currently the non-dotted-non-force_home case. The inference
/// entry point translates the same situation into a richer
/// `InferenceError::NonDottedHome`; this thinner string-error variant
/// matches the original `derive_pack_filename` shape.
pub(crate) fn derive_home_in_pack(
    file_name: &str,
    is_dir: bool,
    force_home: &[String],
) -> std::result::Result<String, String> {
    let stripped = file_name.strip_prefix('.').unwrap_or(file_name);
    let in_force_home = force_home
        .iter()
        .any(|entry| entry.strip_prefix('.').unwrap_or(entry) == stripped);

    if in_force_home {
        Ok(stripped.to_string())
    } else if file_name.starts_with('.') {
        if is_dir {
            Ok(format!("_home/{stripped}"))
        } else {
            Ok(format!("home.{stripped}"))
        }
    } else {
        Err(format!(
            "a non-dotted entry in $HOME has no automatic round-trip path \
             under the post-#48 XDG default. Either rename to a dotted name \
             (e.g. .{stripped}) before adopting, or copy into the pack \
             manually and add a [symlink.targets] override pinning the \
             deploy path."
        ))
    }
}

#[cfg(test)]
mod tests {
    //! Inference tests use a hand-built `XdgPather` rather than touching
    //! real env vars. The point is to exercise every branch of the
    //! decision tree on synthetic paths; integration tests in
    //! `commands::tests` cover end-to-end adopt against a real temp FS.
    //!
    //! Each test is named `<root>_<scenario>_<outcome>` so failures are
    //! self-locating in `cargo test` output.
    use super::*;
    use crate::paths::XdgPather;

    /// Build a `Pather` with HOME and XDG explicitly set to non-overlapping
    /// paths under `/tmp/<id>` so prefix matching is unambiguous. We avoid
    /// the default `$HOME/.config` layout in most tests: it's the *harder*
    /// case (XDG nested under HOME), and we cover it explicitly in
    /// `xdg_inside_home_prefers_xdg_root`.
    ///
    /// `app_support_dir` is pinned to a path under HOME (mirroring the
    /// real macOS layout). Tests that need to exercise the
    /// non-macOS / collapsed AppSupport case use [`pather_app_collapsed`]
    /// instead.
    fn pather(home: &str, xdg: &str) -> XdgPather {
        XdgPather::builder()
            .home(home)
            .dotfiles_root(format!("{home}/dotfiles"))
            .data_dir(format!("{home}/.local/share/dodot"))
            .config_dir(format!("{home}/.config/dodot"))
            .cache_dir(format!("{home}/.cache/dodot"))
            .xdg_config_home(xdg)
            .app_support_dir(format!("{home}/Library/Application Support"))
            .build()
            .unwrap()
    }

    /// Variant of [`pather`] where `app_support_dir` collapses onto
    /// `xdg_config_home` — the Linux default and the macOS
    /// `app_uses_library = false` opt-out.
    fn pather_app_collapsed(home: &str, xdg: &str) -> XdgPather {
        XdgPather::builder()
            .home(home)
            .dotfiles_root(format!("{home}/dotfiles"))
            .data_dir(format!("{home}/.local/share/dodot"))
            .config_dir(format!("{home}/.config/dodot"))
            .cache_dir(format!("{home}/.cache/dodot"))
            .xdg_config_home(xdg)
            .app_support_dir(xdg)
            .build()
            .unwrap()
    }

    #[test]
    fn xdg_nested_file_infers_pack_and_relative_path() {
        let p = pather("/u", "/x");
        let t = infer_target(
            Path::new("/x/nvim/init.lua"),
            /*is_dir=*/ false,
            &p,
            &[],
        )
        .unwrap();
        assert_eq!(t.natural_pack.as_deref(), Some("nvim"));
        assert_eq!(t.in_pack_natural, PathBuf::from("init.lua"));
        // Override-aware: `_xdg/<pack>/<rest>` so an arbitrary `--into`
        // still round-trips via the resolver's Priority 2 `_xdg/` prefix.
        assert_eq!(t.in_pack_override, PathBuf::from("_xdg/nvim/init.lua"));
        assert_eq!(t.source_root, SourceRoot::XdgConfig);
        assert!(!t.expand_children);
    }

    #[test]
    fn xdg_deeply_nested_file_keeps_full_subpath() {
        // The first segment under XDG is always the pack; everything
        // below stays in `in_pack` verbatim. This is the case that
        // makes `dodot adopt ~/.config/nvim/lua/plugins/foo.lua` work.
        let p = pather("/u", "/x");
        let t = infer_target(Path::new("/x/nvim/lua/plugins/foo.lua"), false, &p, &[]).unwrap();
        assert_eq!(t.natural_pack.as_deref(), Some("nvim"));
        assert_eq!(t.in_pack_natural, PathBuf::from("lua/plugins/foo.lua"));
        assert_eq!(
            t.in_pack_override,
            PathBuf::from("_xdg/nvim/lua/plugins/foo.lua")
        );
    }

    #[test]
    fn xdg_pack_root_directory_triggers_expansion() {
        // `~/.config/nvim/` (the directory itself) → pack `nvim`,
        // expand_children=true. The caller enumerates entries inside
        // and adopts each as a top-level pack member, instead of
        // making `~/.config/nvim/` itself a single big symlink.
        let p = pather("/u", "/x");
        let t = infer_target(Path::new("/x/nvim"), /*is_dir=*/ true, &p, &[]).unwrap();
        assert_eq!(t.natural_pack.as_deref(), Some("nvim"));
        assert!(t.expand_children);
        assert_eq!(t.in_pack_natural, PathBuf::new());
    }

    #[test]
    fn xdg_loose_file_at_root_is_refused() {
        // `~/.config/standalone.toml` (a file, not a dir) at the very
        // top of XDG has no pack structure to mine. We refuse rather
        // than guess, and steer the user toward `--into` + manual
        // `_xdg/` placement.
        let p = pather("/u", "/x");
        let err = infer_target(
            Path::new("/x/standalone.toml"),
            /*is_dir=*/ false,
            &p,
            &[],
        )
        .unwrap_err();
        assert!(matches!(err, InferenceError::LooseXdgFile));
    }

    #[test]
    fn xdg_root_itself_is_refused() {
        let p = pather("/u", "/x");
        let err = infer_target(Path::new("/x"), true, &p, &[]).unwrap_err();
        assert!(matches!(err, InferenceError::XdgRootItself));
    }

    #[test]
    fn home_dotted_file_uses_home_prefix_no_pack_inference() {
        // `~/.vimrc` → pack name *not* inferred (HOME has no pack
        // structure), in-pack `home.vimrc` so the resolver routes back
        // to `~/.vimrc` via Priority 1.
        let p = pather("/u", "/x");
        let t = infer_target(Path::new("/u/.vimrc"), false, &p, &[]).unwrap();
        assert_eq!(t.natural_pack, None);
        assert_eq!(t.in_pack_natural, PathBuf::from("home.vimrc"));
        // HOME prefixes are pack-name-independent — override path matches.
        assert_eq!(t.in_pack_override, PathBuf::from("home.vimrc"));
        assert_eq!(t.source_root, SourceRoot::Home);
    }

    #[test]
    fn home_dotted_dir_uses_home_subtree_prefix() {
        let p = pather("/u", "/x");
        let t = infer_target(Path::new("/u/.weechat"), /*is_dir=*/ true, &p, &[]).unwrap();
        assert_eq!(t.natural_pack, None);
        assert_eq!(t.in_pack_natural, PathBuf::from("_home/weechat"));
    }

    #[test]
    fn home_force_home_uses_bare_name() {
        // `force_home` curates Unix canons (`.bashrc`, `.ssh`, …) whose
        // Priority 3 resolver rule routes the bare pack-relative name
        // straight back to `~/.X`. So adopt drops the `home.`/`_home/`
        // prefix for matches — keeps the pack tree clean and matches
        // the resolver convention.
        let force = vec!["bashrc".to_string(), "ssh".to_string()];
        let p = pather("/u", "/x");

        let t = infer_target(Path::new("/u/.bashrc"), false, &p, &force).unwrap();
        assert_eq!(t.in_pack_natural, PathBuf::from("bashrc"));

        let t = infer_target(Path::new("/u/.ssh"), true, &p, &force).unwrap();
        assert_eq!(t.in_pack_natural, PathBuf::from("ssh"));
    }

    #[test]
    fn home_non_dotted_is_refused() {
        // No automatic round-trip: a file like `~/myscript.sh` would
        // need `[symlink.targets]` for an explicit deploy path.
        let p = pather("/u", "/x");
        let err = infer_target(Path::new("/u/myscript.sh"), false, &p, &[]).unwrap_err();
        match err {
            InferenceError::NonDottedHome { stripped } => assert_eq!(stripped, "myscript.sh"),
            other => panic!("expected NonDottedHome, got {other:?}"),
        }
    }

    #[test]
    fn home_nested_outside_xdg_is_unrecognized() {
        // `~/Documents/notes.txt` isn't HOME-direct and isn't under
        // XDG either; we refuse rather than guess at user-data layout.
        let p = pather("/u", "/x");
        let err = infer_target(Path::new("/u/Documents/notes.txt"), false, &p, &[]).unwrap_err();
        assert!(matches!(err, InferenceError::UnrecognizedRoot { .. }));
    }

    #[test]
    fn home_root_itself_is_refused() {
        let p = pather("/u", "/x");
        let err = infer_target(Path::new("/u"), true, &p, &[]).unwrap_err();
        assert!(matches!(err, InferenceError::HomeRootItself));
    }

    #[test]
    fn xdg_inside_home_prefers_xdg_root() {
        // The default config has `XDG_CONFIG_HOME = $HOME/.config`, so
        // an XDG-rooted source's path also starts with HOME. Inference
        // must pick the *more specific* root (XDG) — checking HOME
        // first would produce a useless "nested under $HOME" verdict.
        // This test pins that ordering as a regression guard.
        let p = pather("/u", "/u/.config");
        let t = infer_target(Path::new("/u/.config/nvim/init.lua"), false, &p, &[]).unwrap();
        assert_eq!(t.source_root, SourceRoot::XdgConfig);
        assert_eq!(t.natural_pack.as_deref(), Some("nvim"));
        assert_eq!(t.in_pack_natural, PathBuf::from("init.lua"));
    }

    #[test]
    fn unrecognized_root_lists_known_roots() {
        // Random absolute path under neither HOME nor XDG — refused
        // with the recognized roots in the error so the user knows
        // where adopt looks.
        let p = pather("/u", "/x");
        let err = infer_target(Path::new("/etc/passwd"), false, &p, &[]).unwrap_err();
        match err {
            InferenceError::UnrecognizedRoot { hint_roots } => {
                assert!(hint_roots.iter().any(|r| r.starts_with("/x")));
                assert!(hint_roots.iter().any(|r| r.starts_with("/u")));
            }
            other => panic!("expected UnrecognizedRoot, got {other:?}"),
        }
    }

    // ── AppSupport source root ──────────────────────────────────

    #[test]
    fn app_support_nested_file_uses_app_prefix() {
        // `~/Library/Application Support/Code/User/settings.json` →
        // pack `Code`, in-pack `_app/Code/User/settings.json`. The
        // `_app/` prefix is mandatory at natural pack name (see
        // resolver Priority 6 → §7.2).
        let p = pather("/u", "/x");
        let t = infer_target(
            Path::new("/u/Library/Application Support/Code/User/settings.json"),
            false,
            &p,
            &[],
        )
        .unwrap();
        assert_eq!(t.natural_pack.as_deref(), Some("Code"));
        assert_eq!(
            t.in_pack_natural,
            PathBuf::from("_app/Code/User/settings.json")
        );
        // The override-aware path is identical for AppSupport — the
        // explicit prefix is required either way.
        assert_eq!(
            t.in_pack_override,
            PathBuf::from("_app/Code/User/settings.json")
        );
        assert_eq!(t.source_root, SourceRoot::AppSupport);
        assert!(!t.expand_children);
    }

    #[test]
    fn app_support_pack_root_directory_triggers_expansion() {
        // `~/Library/Application Support/Cursor/` (the directory) →
        // pack `Cursor`, expand_children=true. The caller enumerates
        // its children and adopts each as a top-level pack member.
        let p = pather("/u", "/x");
        let t = infer_target(
            Path::new("/u/Library/Application Support/Cursor"),
            /*is_dir=*/ true,
            &p,
            &[],
        )
        .unwrap();
        assert_eq!(t.natural_pack.as_deref(), Some("Cursor"));
        assert!(t.expand_children);
        // For AppSupport pack-root expansion, in_pack_override carries
        // `_app/<X>` so per-child plans can join the child basename.
        assert_eq!(t.in_pack_override, PathBuf::from("_app/Cursor"));
    }

    #[test]
    fn app_support_outranks_home_when_distinct_root() {
        // On a real macOS layout `~/Library/Application Support` lives
        // under HOME — longest-prefix-wins requires AppSupport be
        // checked first. `pather()` mirrors that layout.
        let p = pather("/u", "/x");
        let t = infer_target(
            Path::new("/u/Library/Application Support/Zed/settings.json"),
            false,
            &p,
            &[],
        )
        .unwrap();
        assert_eq!(t.source_root, SourceRoot::AppSupport);
        assert_eq!(t.natural_pack.as_deref(), Some("Zed"));
    }

    #[test]
    fn app_support_collapsed_falls_back_to_lib_or_unrecognized() {
        // When `app_support_dir == xdg_config_home` (Linux, or macOS
        // with `app_uses_library = false`), the AppSupport arm is
        // unreachable and a `~/Library/Application Support/...` path
        // falls through.
        //
        // On macOS the Library recognizer catches it next and produces
        // a `_lib/Application Support/Code/...` plan — that round-trips
        // correctly on deploy because `_lib/` is unaffected by
        // `app_uses_library` (per `macos-paths.lex` §11.2). On non-macOS
        // the Library recognizer is cfg-gated off, the path falls all
        // the way through, and the user gets an `UnrecognizedRoot`
        // refusal — appropriate because `_lib/` warns-and-skips at
        // deploy time on Linux anyway.
        let p = pather_app_collapsed("/u", "/x");
        let result = infer_target(
            Path::new("/u/Library/Application Support/Code/User/settings.json"),
            false,
            &p,
            &[],
        );
        if cfg!(target_os = "macos") {
            let t = result.expect("inference");
            assert_eq!(t.source_root, SourceRoot::Library);
            assert_eq!(
                t.in_pack_natural,
                Path::new("_lib/Application Support/Code/User/settings.json")
            );
        } else {
            let err = result.unwrap_err();
            assert!(matches!(err, InferenceError::UnrecognizedRoot { .. }));
        }
    }

    // ── Capitalization heuristic ────────────────────────────────

    #[test]
    fn gui_app_heuristic_uppercase_yes() {
        // Strong uppercase signal: any non-empty name with at least one
        // uppercase ASCII letter qualifies. This is the dominant
        // pattern for `~/Library/Application Support/` folder names.
        assert!(is_gui_app_folder("Code"));
        assert!(is_gui_app_folder("Cursor"));
        assert!(is_gui_app_folder("IntelliJ"));
        assert!(is_gui_app_folder("Visual Studio Code"));
    }

    #[test]
    fn gui_app_heuristic_space_yes() {
        // Spaces are the second-strongest signal: CLI tools don't put
        // spaces in `~/.config/<X>/` for portability reasons.
        assert!(is_gui_app_folder("sublime text"));
        assert!(is_gui_app_folder("smart code ltd"));
    }

    #[test]
    fn gui_app_heuristic_reverse_dns_yes() {
        // Reverse-DNS is the third signal — captures bundle-ID-shaped
        // names that some apps use (`dev.warp.Warp-Stable`,
        // `com.apple.dt.Xcode`).
        assert!(is_gui_app_folder("dev.warp.warp-stable"));
        assert!(is_gui_app_folder("com.apple.dt.xcode"));
        assert!(is_gui_app_folder("org.videolan.vlc"));
    }

    #[test]
    fn gui_app_heuristic_lowercase_cli_tool_no() {
        // The CLI-tool population is uniformly lowercase-hyphenated and
        // never matches the rDNS shape (single segment, no dots).
        assert!(!is_gui_app_folder("nvim"));
        assert!(!is_gui_app_folder("helix"));
        assert!(!is_gui_app_folder("ghostty"));
        assert!(!is_gui_app_folder("lazygit"));
        assert!(!is_gui_app_folder("starship"));
    }

    #[test]
    fn gui_app_heuristic_empty_name_no() {
        assert!(!is_gui_app_folder(""));
    }

    #[test]
    fn gui_app_heuristic_dotted_but_empty_segment_no() {
        // A name like `.bashrc` or `foo..bar` shouldn't slip through
        // the rDNS branch — rDNS requires every segment non-empty.
        assert!(!is_gui_app_folder(".bashrc"));
        assert!(!is_gui_app_folder("foo..bar"));
        assert!(!is_gui_app_folder("trailing."));
    }

    // ── Library/* (Preferences, LaunchAgents, …) ────────────────────────
    //
    // The Library recognizer is gated on `cfg!(target_os = "macos")` —
    // on Linux it skips entirely so adopt doesn't generate plans that
    // the deploy resolver would warn-and-skip on. Tests are
    // correspondingly macOS-only; on non-macOS they're compiled out.

    #[cfg(target_os = "macos")]
    #[test]
    fn library_preferences_plist_uses_lib_prefix_and_requires_into() {
        let p = pather("/u", "/x");
        let t = infer_target(
            Path::new("/u/Library/Preferences/com.colliderli.iina.plist"),
            false,
            &p,
            &[],
        )
        .expect("inference");
        assert_eq!(t.source_root, SourceRoot::Library);
        assert_eq!(t.natural_pack, None, "Library sources require --into");
        assert_eq!(
            t.in_pack_natural,
            Path::new("_lib/Preferences/com.colliderli.iina.plist")
        );
        assert_eq!(t.in_pack_natural, t.in_pack_override);
        assert!(!t.expand_children);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn library_launch_agents_routes_through_lib_prefix() {
        let p = pather("/u", "/x");
        let t = infer_target(
            Path::new("/u/Library/LaunchAgents/com.example.foo.plist"),
            false,
            &p,
            &[],
        )
        .expect("inference");
        assert_eq!(t.source_root, SourceRoot::Library);
        assert_eq!(
            t.in_pack_natural,
            Path::new("_lib/LaunchAgents/com.example.foo.plist")
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn library_subdirectory_top_level_expands_children() {
        // `~/Library/LaunchAgents/` as a directory source: the sole
        // path component triggers expand_children so callers enumerate
        // entries.
        let p = pather("/u", "/x");
        let t =
            infer_target(Path::new("/u/Library/LaunchAgents"), true, &p, &[]).expect("inference");
        assert!(t.expand_children);
        assert_eq!(t.in_pack_natural, Path::new("_lib/LaunchAgents"));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn library_root_itself_is_refused() {
        let p = pather("/u", "/x");
        let err = infer_target(Path::new("/u/Library"), true, &p, &[]).unwrap_err();
        assert!(matches!(err, InferenceError::LibraryRootItself));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn library_application_support_still_routes_through_app_support() {
        // The more-specific AppSupport prefix must win over the
        // broader Library prefix; otherwise paths like
        // `~/Library/Application Support/Code/...` would land at
        // `_lib/Application Support/...` instead of `_app/Code/...`.
        let p = pather("/u", "/x");
        let t = infer_target(
            Path::new("/u/Library/Application Support/Code/User/settings.json"),
            false,
            &p,
            &[],
        )
        .expect("inference");
        assert_eq!(t.source_root, SourceRoot::AppSupport);
        assert!(t.in_pack_natural.starts_with("_app"));
    }

    #[test]
    fn macos_containers_path_is_refused() {
        // `~/Library/Containers/<bundle>/...` is a sandboxed-app data
        // directory — partially OS-managed and not intended for
        // external editing. Refused on every platform so no one
        // accidentally adopts a container's plist on a CI Linux box
        // by feeding it a path that mimics the macOS layout.
        let p = pather("/u", "/x");
        let err = infer_target(
            Path::new("/u/Library/Containers/com.example.app/Data/Library/Preferences/foo.plist"),
            false,
            &p,
            &[],
        )
        .unwrap_err();
        assert!(matches!(err, InferenceError::SandboxedContainer));
    }
}
