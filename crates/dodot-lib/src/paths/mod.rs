use std::path::{Path, PathBuf};

use crate::Result;

/// Provides all path calculations for dodot.
///
/// Every path that dodot uses -- XDG directories, pack locations,
/// handler data directories -- is computed through this trait. This
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

        Ok(XdgPather {
            home,
            dotfiles_root,
            data_dir,
            config_dir,
            cache_dir,
            xdg_config_home,
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

    // Compile-time check: Pather must be object-safe
    #[allow(dead_code)]
    fn assert_object_safe(_: &dyn Pather) {}
}
