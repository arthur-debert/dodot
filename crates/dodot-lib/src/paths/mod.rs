//! Path resolution for dodot.
//!
//! `Pather` is dodot's single source of truth for *every* filesystem
//! coordinate the rest of the codebase touches: `$HOME`, the dotfiles
//! repo root, the XDG data/config/cache directories, and per-pack and
//! per-handler subdirectories. Two reasons it's a trait, not free
//! functions:
//!
//! 1. **Testability.** Constructing a `Pather` whose roots all live
//!    under a `tempfile::TempDir` lets every command run end-to-end
//!    against a real filesystem without ever touching the user's
//!    actual `$HOME`. The `testing::TempEnvironment` builder does
//!    exactly this.
//!
//! 2. **Centralisation of OS-shaped policy.** The XDG fallback chain,
//!    the `DOTFILES_ROOT` env-var lookup, and (planned, per
//!    `docs/proposals/macos-paths.lex`) the macOS `app_support_dir`
//!    selection all live in one place. The resolver, the symlink
//!    handler, and `adopt`'s source-path inference all consult the
//!    same accessors — drift between them is impossible by construction.
//!
//! ## Adopt source-root invariants
//!
//! The inference function in `commands::adopt::infer` needs *stable
//! root strings* it can prefix-match against canonicalised source
//! paths. The accessors exposed here meet two requirements that make
//! that work safely:
//!
//! - `home_dir()` and `xdg_config_home()` return paths that
//!   `std::fs::canonicalize` resolves to themselves on a real
//!   filesystem (they're real directories, not synthetic constants).
//!   This is what makes the `/var` ↔ `/private/var` macOS equivalence
//!   collapse cleanly when both a source and a root are canonicalised
//!   before comparison.
//!
//! - On the default config (no `XDG_CONFIG_HOME` set), `xdg_config_home()`
//!   is `home_dir().join(".config")` — i.e. *nested under* `$HOME`.
//!   Inference must check the more-specific (XDG) root before HOME so
//!   `~/.config/nvim/init.lua` matches XDG, not "nested under HOME".
//!   That's enforced by the inference function, not by `Pather`, but
//!   the nesting shape originates here.

use std::path::{Path, PathBuf};

use crate::Result;

/// Provides all path calculations for dodot.
///
/// Every path that dodot uses — XDG directories, pack locations,
/// handler data directories — is computed through this trait. This
/// keeps path logic centralised and makes testing straightforward:
/// construct a `Pather` whose directories all live under a temp dir.
///
/// Use `&dyn Pather` (trait objects) throughout the codebase.
pub trait Pather: Send + Sync {
    /// The user's home directory (e.g. `/home/alice`).
    fn home_dir(&self) -> &Path;

    /// Root of the dotfiles repository.
    fn dotfiles_root(&self) -> &Path;

    /// XDG data directory for dodot (e.g. `~/.local/share/dodot`).
    fn data_dir(&self) -> &Path;

    /// XDG config directory for dodot (e.g. `~/.config/dodot`).
    fn config_dir(&self) -> &Path;

    /// XDG cache directory for dodot (e.g. `~/.cache/dodot`).
    fn cache_dir(&self) -> &Path;

    /// XDG config home (e.g. `~/.config`). Used by symlink handler
    /// for subdirectory target mapping.
    fn xdg_config_home(&self) -> &Path;

    /// Application-support root, the third filesystem coordinate the
    /// symlink resolver understands.
    ///
    /// On macOS this resolves to `$HOME/Library/Application Support` by
    /// default, the canonical home for GUI app config. On Linux and
    /// other platforms it resolves to `xdg_config_home()` so the `_app/`
    /// prefix and `app_aliases` route through `~/.config` —
    /// indistinguishable from `_xdg/` on those platforms but the
    /// mechanism stays platform-agnostic.
    ///
    /// The OS check lives only in [`XdgPatherBuilder::build`]; the
    /// resolver operates on textual prefixes alone. See
    /// `docs/proposals/macos-paths.lex` §2.1.
    fn app_support_dir(&self) -> &Path;

    /// Shell scripts directory (e.g. `~/.local/share/dodot/shell`).
    fn shell_dir(&self) -> &Path;

    /// Absolute path to a pack's source directory.
    fn pack_path(&self, pack: &str) -> PathBuf {
        self.dotfiles_root().join(pack)
    }

    /// Data directory for a specific pack (e.g. `.../data/packs/{pack}`).
    fn pack_data_dir(&self, pack: &str) -> PathBuf {
        self.data_dir().join("packs").join(pack)
    }

    /// Data directory for a specific handler within a pack
    /// (e.g. `.../data/packs/{pack}/{handler}`).
    fn handler_data_dir(&self, pack: &str, handler: &str) -> PathBuf {
        self.pack_data_dir(pack).join(handler)
    }

    /// Log directory for dodot (e.g. `~/.cache/dodot/logs`).
    fn log_dir(&self) -> PathBuf {
        self.cache_dir().join("logs")
    }

    /// Path to the generated shell init script.
    fn init_script_path(&self) -> PathBuf {
        self.shell_dir().join("dodot-init.sh")
    }

    /// Path to the deployment map TSV, overwritten on every `up` / `down`.
    /// See `docs/proposals/profiling.lex` §3.2.
    fn deployment_map_path(&self) -> PathBuf {
        self.data_dir().join("deployment-map.tsv")
    }

    /// Path to a single-line file recording the unix timestamp of the
    /// most recent successful `dodot up`. Used by `dodot probe
    /// shell-init` to flag profiles captured before that `up` as stale.
    /// Absent until the first `up` runs.
    fn last_up_path(&self) -> PathBuf {
        self.data_dir().join("last-up-at")
    }

    /// Directory where shell-init profile reports are written, one TSV
    /// per shell start. See `docs/proposals/profiling.lex` §3.1.
    fn probes_shell_init_dir(&self) -> PathBuf {
        self.data_dir().join("probes").join("shell-init")
    }

    /// On-disk cache for homebrew-cask probe data. One JSON file per
    /// cask token; TTL-based invalidation. See
    /// `docs/proposals/macos-paths.lex` §8.2.
    ///
    /// Lives under `cache_dir` (not `data_dir`) because the contents are
    /// rederivable — losing them is fine, the next probe re-runs `brew
    /// info`. Co-located with future probe caches under `probes/`.
    fn probes_brew_cache_dir(&self) -> PathBuf {
        self.cache_dir().join("probes").join("brew")
    }

    /// Persistent record of prompts the user has dismissed (e.g.
    /// onboarding hints, install offers). Content-agnostic: callers
    /// pass opaque keys, the registry just tracks dismissed/active.
    /// Lives under `data_dir` (not `cache_dir`) because losing it
    /// would re-prompt the user — preference state, not cache.
    fn prompts_path(&self) -> PathBuf {
        self.data_dir().join("prompts.json")
    }
}

/// XDG-compliant path resolver.
///
/// Reads standard environment variables (`HOME`, `XDG_DATA_HOME`, etc.)
/// and the dodot-specific `DOTFILES_ROOT`. All paths can also be set
/// explicitly via the builder for testing.
#[derive(Debug, Clone)]
pub struct XdgPather {
    home: PathBuf,
    dotfiles_root: PathBuf,
    data_dir: PathBuf,
    config_dir: PathBuf,
    cache_dir: PathBuf,
    xdg_config_home: PathBuf,
    app_support_dir: PathBuf,
    shell_dir: PathBuf,
}

/// Builder for [`XdgPather`].
///
/// All fields are optional. Unset fields are resolved from environment
/// variables or XDG defaults.
#[derive(Debug, Default)]
pub struct XdgPatherBuilder {
    home: Option<PathBuf>,
    dotfiles_root: Option<PathBuf>,
    data_dir: Option<PathBuf>,
    config_dir: Option<PathBuf>,
    cache_dir: Option<PathBuf>,
    xdg_config_home: Option<PathBuf>,
    app_support_dir: Option<PathBuf>,
}

impl XdgPatherBuilder {
    pub fn home(mut self, path: impl Into<PathBuf>) -> Self {
        self.home = Some(path.into());
        self
    }

    pub fn dotfiles_root(mut self, path: impl Into<PathBuf>) -> Self {
        self.dotfiles_root = Some(path.into());
        self
    }

    pub fn data_dir(mut self, path: impl Into<PathBuf>) -> Self {
        self.data_dir = Some(path.into());
        self
    }

    pub fn config_dir(mut self, path: impl Into<PathBuf>) -> Self {
        self.config_dir = Some(path.into());
        self
    }

    pub fn cache_dir(mut self, path: impl Into<PathBuf>) -> Self {
        self.cache_dir = Some(path.into());
        self
    }

    pub fn xdg_config_home(mut self, path: impl Into<PathBuf>) -> Self {
        self.xdg_config_home = Some(path.into());
        self
    }

    /// Override the application-support root.
    ///
    /// Tests pin this to a non-default location so prefix matches are
    /// deterministic across platforms. End users may also flip this
    /// (typically via the `app_uses_library` config key, which is
    /// ultimately what wires through here) to opt into Linux-style
    /// `~/.config` placement on macOS.
    pub fn app_support_dir(mut self, path: impl Into<PathBuf>) -> Self {
        self.app_support_dir = Some(path.into());
        self
    }

    pub fn build(self) -> Result<XdgPather> {
        let home = self.home.unwrap_or_else(resolve_home);

        let dotfiles_root = self
            .dotfiles_root
            .unwrap_or_else(|| resolve_dotfiles_root(&home));

        let xdg_config_home = self.xdg_config_home.unwrap_or_else(|| {
            std::env::var("XDG_CONFIG_HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|_| home.join(".config"))
        });

        let data_dir = self.data_dir.unwrap_or_else(|| {
            let xdg_data = std::env::var("XDG_DATA_HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|_| home.join(".local").join("share"));
            xdg_data.join("dodot")
        });

        let config_dir = self
            .config_dir
            .unwrap_or_else(|| xdg_config_home.join("dodot"));

        let cache_dir = self.cache_dir.unwrap_or_else(|| {
            let xdg_cache = std::env::var("XDG_CACHE_HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|_| home.join(".cache"));
            xdg_cache.join("dodot")
        });

        let shell_dir = data_dir.join("shell");

        // Application-support root: macOS routes to `~/Library/Application Support`,
        // every other platform falls through to `xdg_config_home`. The OS
        // branch lives here exclusively; the resolver only sees a path.
        let app_support_dir = self.app_support_dir.unwrap_or_else(|| {
            if cfg!(target_os = "macos") {
                home.join("Library").join("Application Support")
            } else {
                xdg_config_home.clone()
            }
        });

        Ok(XdgPather {
            home,
            dotfiles_root,
            data_dir,
            config_dir,
            cache_dir,
            xdg_config_home,
            app_support_dir,
            shell_dir,
        })
    }
}

impl XdgPather {
    /// Creates a builder for configuring an `XdgPather`.
    pub fn builder() -> XdgPatherBuilder {
        XdgPatherBuilder::default()
    }

    /// Creates an `XdgPather` using environment variables and XDG defaults.
    pub fn from_env() -> Result<Self> {
        Self::builder().build()
    }
}

impl Pather for XdgPather {
    fn home_dir(&self) -> &Path {
        &self.home
    }

    fn dotfiles_root(&self) -> &Path {
        &self.dotfiles_root
    }

    fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    fn config_dir(&self) -> &Path {
        &self.config_dir
    }

    fn cache_dir(&self) -> &Path {
        &self.cache_dir
    }

    fn xdg_config_home(&self) -> &Path {
        &self.xdg_config_home
    }

    fn app_support_dir(&self) -> &Path {
        &self.app_support_dir
    }

    fn shell_dir(&self) -> &Path {
        &self.shell_dir
    }
}

/// Resolve `HOME` from environment, falling back to the `dirs` approach.
fn resolve_home() -> PathBuf {
    std::env::var("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            // Last resort fallback
            PathBuf::from("/tmp/dodot-unknown-home")
        })
}

/// Resolve the dotfiles root directory.
///
/// Priority:
/// 1. `DOTFILES_ROOT` environment variable
/// 2. Git repository root (`git rev-parse --show-toplevel`)
/// 3. `$HOME/dotfiles` fallback
fn resolve_dotfiles_root(home: &Path) -> PathBuf {
    // 1. Explicit env var
    if let Ok(root) = std::env::var("DOTFILES_ROOT") {
        return expand_tilde(&root, home);
    }

    // 2. Git toplevel
    if let Ok(output) = std::process::Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .output()
    {
        if output.status.success() {
            let toplevel = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !toplevel.is_empty() {
                return PathBuf::from(toplevel);
            }
        }
    }

    // 3. Fallback
    home.join("dotfiles")
}

/// Expand a leading `~` to the home directory.
fn expand_tilde(path: &str, home: &Path) -> PathBuf {
    if let Some(rest) = path.strip_prefix("~/") {
        home.join(rest)
    } else if path == "~" {
        home.to_path_buf()
    } else {
        PathBuf::from(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builder_explicit_paths() {
        let pather = XdgPather::builder()
            .home("/test/home")
            .dotfiles_root("/test/home/dotfiles")
            .data_dir("/test/data/dodot")
            .config_dir("/test/config/dodot")
            .cache_dir("/test/cache/dodot")
            .xdg_config_home("/test/home/.config")
            .build()
            .unwrap();

        assert_eq!(pather.home_dir(), Path::new("/test/home"));
        assert_eq!(pather.dotfiles_root(), Path::new("/test/home/dotfiles"));
        assert_eq!(pather.data_dir(), Path::new("/test/data/dodot"));
        assert_eq!(pather.config_dir(), Path::new("/test/config/dodot"));
        assert_eq!(pather.cache_dir(), Path::new("/test/cache/dodot"));
        assert_eq!(pather.xdg_config_home(), Path::new("/test/home/.config"));
    }

    #[test]
    fn shell_dir_derived_from_data_dir() {
        let pather = XdgPather::builder()
            .home("/h")
            .dotfiles_root("/h/dots")
            .data_dir("/h/data/dodot")
            .build()
            .unwrap();

        assert_eq!(pather.shell_dir(), Path::new("/h/data/dodot/shell"));
    }

    #[test]
    fn pack_path_joins_dotfiles_root() {
        let pather = XdgPather::builder()
            .home("/h")
            .dotfiles_root("/h/dotfiles")
            .build()
            .unwrap();

        assert_eq!(pather.pack_path("vim"), PathBuf::from("/h/dotfiles/vim"));
    }

    #[test]
    fn pack_data_dir_structure() {
        let pather = XdgPather::builder()
            .home("/h")
            .data_dir("/h/data/dodot")
            .build()
            .unwrap();

        assert_eq!(
            pather.pack_data_dir("vim"),
            PathBuf::from("/h/data/dodot/packs/vim")
        );
    }

    #[test]
    fn handler_data_dir_structure() {
        let pather = XdgPather::builder()
            .home("/h")
            .data_dir("/h/data/dodot")
            .build()
            .unwrap();

        assert_eq!(
            pather.handler_data_dir("vim", "symlink"),
            PathBuf::from("/h/data/dodot/packs/vim/symlink")
        );
    }

    #[test]
    fn init_script_path() {
        let pather = XdgPather::builder()
            .home("/h")
            .data_dir("/h/data/dodot")
            .build()
            .unwrap();

        assert_eq!(
            pather.init_script_path(),
            PathBuf::from("/h/data/dodot/shell/dodot-init.sh")
        );
    }

    #[test]
    fn expand_tilde_cases() {
        let home = Path::new("/home/alice");
        assert_eq!(
            expand_tilde("~/dotfiles", home),
            PathBuf::from("/home/alice/dotfiles")
        );
        assert_eq!(expand_tilde("~", home), PathBuf::from("/home/alice"));
        assert_eq!(
            expand_tilde("/absolute/path", home),
            PathBuf::from("/absolute/path")
        );
        assert_eq!(expand_tilde("relative", home), PathBuf::from("relative"));
    }

    /// Default-XDG nesting: with no explicit `xdg_config_home`, the
    /// builder defaults to `$HOME/.config`. Adopt's inference relies on
    /// XDG being checked *before* HOME (longest-prefix wins) precisely
    /// because of this nesting; pin the layout so a future change that
    /// flips the default to `$HOME/Library/...` (macOS) or somewhere
    /// outside HOME forces a deliberate update to the inference rules.
    #[test]
    fn default_xdg_config_home_is_nested_under_home() {
        let pather = XdgPather::builder()
            .home("/u")
            .dotfiles_root("/u/dotfiles")
            .data_dir("/u/.local/share/dodot")
            .config_dir("/u/.config/dodot")
            .cache_dir("/u/.cache/dodot")
            // No xdg_config_home set; falls back to env or `$HOME/.config`.
            .build()
            .unwrap();
        // The default fallback (no `XDG_CONFIG_HOME` env) is `$HOME/.config`.
        // The assertion has to tolerate a user-set `XDG_CONFIG_HOME` since
        // tests inherit the ambient env — `cargo test` from a developer
        // shell with the env set would otherwise fail spuriously. The
        // disjunct below means: either XDG nests under HOME (the default
        // case the invariant talks about), OR the env override is set
        // (the user opted out of the default; adopt's inference handles
        // that case via root canonicalization, separate code path).
        let xdg = pather.xdg_config_home();
        let home = pather.home_dir();
        assert!(
            xdg.starts_with(home) || std::env::var("XDG_CONFIG_HOME").is_ok(),
            "default xdg_config_home `{}` is not nested under home `{}` \
             — adopt's inference assumes XDG ⊆ HOME on the default config; \
             update both if this changes",
            xdg.display(),
            home.display()
        );
    }

    /// Explicit `xdg_config_home(...)` takes precedence over env / defaults.
    /// Critical for the test environment, where adopt-inference tests pin
    /// XDG to a non-default location so prefix matches are unambiguous.
    #[test]
    fn explicit_xdg_config_home_overrides_default() {
        let pather = XdgPather::builder()
            .home("/u")
            .dotfiles_root("/u/dotfiles")
            .xdg_config_home("/somewhere/else/.config")
            .build()
            .unwrap();
        assert_eq!(
            pather.xdg_config_home(),
            Path::new("/somewhere/else/.config")
        );
    }

    /// Each accessor returns a stable, distinct subdir layout. Adopt's
    /// auto-create path lands the new pack at `dotfiles_root/<pack>`,
    /// and the data layer keeps state at `data_dir/packs/<pack>/...`;
    /// these must not alias.
    #[test]
    fn dotfiles_root_and_data_dir_are_distinct_namespaces() {
        let pather = XdgPather::builder()
            .home("/u")
            .dotfiles_root("/u/dotfiles")
            .data_dir("/u/.local/share/dodot")
            .build()
            .unwrap();
        let pack_dir = pather.pack_path("nvim");
        let pack_data = pather.pack_data_dir("nvim");
        assert!(
            !pack_dir.starts_with(&pack_data) && !pack_data.starts_with(&pack_dir),
            "pack_path `{}` and pack_data_dir `{}` overlap",
            pack_dir.display(),
            pack_data.display(),
        );
    }

    /// Explicit `app_support_dir(...)` overrides the platform default.
    /// Tests rely on this to pin the third coordinate at a known
    /// non-XDG, non-HOME location so the resolver's `_app/` rule has
    /// somewhere unambiguous to land.
    #[test]
    fn explicit_app_support_dir_overrides_default() {
        let pather = XdgPather::builder()
            .home("/u")
            .dotfiles_root("/u/dotfiles")
            .xdg_config_home("/u/.config")
            .app_support_dir("/u/Library/Application Support")
            .build()
            .unwrap();
        assert_eq!(
            pather.app_support_dir(),
            Path::new("/u/Library/Application Support")
        );
    }

    /// Default app_support_dir on Linux/non-macOS collapses to xdg_config_home.
    /// On macOS it points under `$HOME/Library/Application Support`.
    /// We don't `cfg!` the assertion here because the explicit-builder
    /// test above pins the override path; this test exercises the
    /// implicit default and the platform branch together.
    #[test]
    fn default_app_support_dir_is_platform_aware() {
        let pather = XdgPather::builder()
            .home("/u")
            .dotfiles_root("/u/dotfiles")
            .xdg_config_home("/u/.config")
            .build()
            .unwrap();
        if cfg!(target_os = "macos") {
            assert_eq!(
                pather.app_support_dir(),
                Path::new("/u/Library/Application Support"),
                "macOS default should route under $HOME/Library/Application Support"
            );
        } else {
            assert_eq!(
                pather.app_support_dir(),
                pather.xdg_config_home(),
                "non-macOS default should collapse to xdg_config_home"
            );
        }
    }

    // Compile-time check: Pather must be object-safe
    #[allow(dead_code)]
    fn assert_object_safe(_: &dyn Pather) {}
}
