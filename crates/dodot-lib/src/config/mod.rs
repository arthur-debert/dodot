//! Configuration system for dodot, powered by clapfig.
//!
//! [`DodotConfig`] is the authoritative schema for all dodot settings.
//! Configuration is loaded from a 3-layer hierarchy:
//!
//! 1. **Compiled defaults** — `#[config(default = ...)]` on struct fields
//! 2. **Root config** — `$DOTFILES_ROOT/.dodot.toml`
//! 3. **Pack config** — `$DOTFILES_ROOT/<pack>/.dodot.toml`
//!
//! [`ConfigManager`] wraps clapfig's `Resolver` to provide per-pack
//! config resolution with automatic caching and merging.

use std::path::{Path, PathBuf};

use clapfig::{Boundary, Clapfig, SearchMode, SearchPath};
use confique::Config;
use serde::{Deserialize, Serialize};

use crate::handlers::HandlerConfig;
use crate::rules::Rule;
use crate::{DodotError, Result};

/// The complete dodot configuration schema.
///
/// All fields have compiled defaults via `#[config(default = ...)]`.
/// Root and pack `.dodot.toml` files can override any subset.
#[derive(Config, Debug, Clone, Serialize, Deserialize)]
pub struct DodotConfig {
    #[config(nested)]
    pub pack: PackSection,

    #[config(nested)]
    pub symlink: SymlinkSection,

    #[config(nested)]
    pub path: PathSection,

    #[config(nested)]
    pub mappings: MappingsSection,

    #[config(nested)]
    pub preprocessor: PreprocessorSection,
}

/// Pack-level settings.
#[derive(Config, Debug, Clone, Serialize, Deserialize)]
pub struct PackSection {
    /// Glob patterns for files and directories to ignore during pack
    /// discovery and file scanning.
    #[config(default = [
        ".git", ".svn", ".hg", "node_modules", ".DS_Store",
        "*.swp", "*~", "#*#", ".env*", ".terraform"
    ])]
    pub ignore: Vec<String>,
}

/// Symlink handler settings.
#[derive(Config, Debug, Clone, Serialize, Deserialize)]
pub struct SymlinkSection {
    /// Files/directories that must deploy to `$HOME` instead of
    /// `$XDG_CONFIG_HOME`. Matched against the first path segment
    /// (without leading dot).
    #[config(default = ["ssh", "aws", "kube", "bashrc", "zshrc", "profile", "bash_profile", "bash_login", "bash_logout", "inputrc"])]
    pub force_home: Vec<String>,

    /// Paths that must not be symlinked for security reasons.
    #[config(default = [
        ".ssh/id_rsa", ".ssh/id_ed25519", ".ssh/id_dsa", ".ssh/id_ecdsa",
        ".ssh/authorized_keys", ".gnupg", ".aws/credentials",
        ".password-store", ".config/gh/hosts.yml",
        ".kube/config", ".docker/config.json"
    ])]
    pub protected_paths: Vec<String>,

    /// Custom per-file symlink target overrides.
    /// Maps relative pack filename to absolute or relative target path.
    /// Absolute paths are used as-is; relative paths are resolved from
    /// `$XDG_CONFIG_HOME`.
    #[config(default = {})]
    pub targets: std::collections::HashMap<String, String>,
}

/// PATH handler settings.
#[derive(Config, Debug, Clone, Serialize, Deserialize)]
pub struct PathSection {
    /// Automatically add execute permissions (`+x`) to files inside
    /// `bin/` directories staged by the path handler.
    ///
    /// # Rationale
    ///
    /// Files placed in a `bin/` directory are there because the pack
    /// author intends them as executables — the directory's purpose is
    /// to expose commands via `$PATH`. However, execute bits can be
    /// lost in common workflows:
    ///
    /// - **Git on macOS** defaults to `core.fileMode = false`, so
    ///   cloned repos may have `0o644` on scripts.
    /// - **Manual file creation** often forgets `chmod +x`.
    ///
    /// Without `+x` the shell finds the file via PATH lookup but fails
    /// with "permission denied" — a confusing error when the file is
    /// clearly in the right place.
    ///
    /// With this option enabled (the default), `dodot up` ensures every
    /// file in a path-handler directory is executable, matching the
    /// user's intent. Files that are already executable are left
    /// untouched. Failures are reported as warnings, not hard errors.
    ///
    /// Set to `false` if you have `bin/` files that intentionally
    /// should not be executable (e.g. data files or library scripts
    /// sourced by other scripts).
    #[config(default = true)]
    pub auto_chmod_exec: bool,
}

/// Preprocessing pipeline settings.
#[derive(Config, Debug, Clone, Serialize, Deserialize)]
pub struct PreprocessorSection {
    /// Global kill switch for all preprocessing.
    #[config(default = true)]
    pub enabled: bool,

    #[config(nested)]
    pub template: PreprocessorTemplateSection,
}

/// Template preprocessor settings.
#[derive(Config, Debug, Clone, Serialize, Deserialize)]
pub struct PreprocessorTemplateSection {
    /// File extensions that trigger template rendering. Each extension
    /// is matched as a suffix (e.g. `"tmpl"` matches `config.toml.tmpl`).
    #[config(default = ["tmpl", "template"])]
    pub extensions: Vec<String>,

    /// User-defined variables, accessible as bare names in templates
    /// (e.g. `name = "Alice"` makes `{{ name }}` render as `Alice`).
    ///
    /// Reserved: `dodot` and `env` are built-in namespaces; using them
    /// as var names raises an error at load time.
    #[config(default = {})]
    pub vars: std::collections::HashMap<String, String>,
}

/// File-to-handler mapping patterns.
#[derive(Config, Debug, Clone, Serialize, Deserialize)]
pub struct MappingsSection {
    /// Directory name pattern for PATH handler.
    #[config(default = "bin")]
    pub path: String,

    /// Filename pattern for install scripts.
    #[config(default = "install.sh")]
    pub install: String,

    /// Filename patterns for shell scripts to source.
    #[config(default = ["aliases.sh", "profile.sh", "login.sh"])]
    pub shell: Vec<String>,

    /// Filename pattern for Homebrew Brewfile.
    #[config(default = "Brewfile")]
    pub homebrew: String,

    /// Additional filename patterns to exclude from handler processing
    /// within a pack. Distinct from [pack] ignore which controls discovery.
    #[config(default = [])]
    pub skip: Vec<String>,
}

// ── Conversions ─────────────────────────────────────────────────

impl DodotConfig {
    /// Convert to the handler-relevant config subset.
    pub fn to_handler_config(&self) -> HandlerConfig {
        HandlerConfig {
            force_home: self.symlink.force_home.clone(),
            protected_paths: self.symlink.protected_paths.clone(),
            targets: self.symlink.targets.clone(),
            auto_chmod_exec: self.path.auto_chmod_exec,
            pack_ignore: self.pack.ignore.clone(),
        }
    }
}

/// Generate rules from the mappings section.
///
/// This produces the default rule set that maps filename patterns to
/// handlers, matching the Go implementation's `GenerateRulesFromMapping`.
pub fn mappings_to_rules(mappings: &MappingsSection) -> Vec<Rule> {
    use std::collections::HashMap;

    let mut rules = Vec::new();

    // Path handler (directory pattern with trailing slash convention)
    if !mappings.path.is_empty() {
        let pattern = if mappings.path.ends_with('/') {
            mappings.path.clone()
        } else {
            format!("{}/", mappings.path)
        };
        rules.push(Rule {
            pattern,
            handler: "path".into(),
            priority: 10,
            options: HashMap::new(),
        });
    }

    // Install handler
    if !mappings.install.is_empty() {
        rules.push(Rule {
            pattern: mappings.install.clone(),
            handler: "install".into(),
            priority: 10,
            options: HashMap::new(),
        });
    }

    // Shell handler
    for pattern in &mappings.shell {
        if !pattern.is_empty() {
            rules.push(Rule {
                pattern: pattern.clone(),
                handler: "shell".into(),
                priority: 10,
                options: HashMap::new(),
            });
        }
    }

    // Homebrew handler
    if !mappings.homebrew.is_empty() {
        rules.push(Rule {
            pattern: mappings.homebrew.clone(),
            handler: "homebrew".into(),
            priority: 10,
            options: HashMap::new(),
        });
    }

    // Skip patterns (exclusion rules)
    for pattern in &mappings.skip {
        if !pattern.is_empty() {
            rules.push(Rule {
                pattern: format!("!{pattern}"),
                handler: "exclude".into(),
                priority: 100, // exclusions checked first
                options: HashMap::new(),
            });
        }
    }

    // Catchall: everything else goes to symlink (lowest priority)
    rules.push(Rule {
        pattern: "*".into(),
        handler: "symlink".into(),
        priority: 0,
        options: HashMap::new(),
    });

    rules
}

// ── ConfigManager ───────────────────────────────────────────────

/// Manages configuration loading and per-pack resolution.
///
/// Wraps clapfig's `Resolver` to provide cached, merged config
/// resolution. Call [`config_for_pack`](ConfigManager::config_for_pack)
/// for each pack — the root `.dodot.toml` is read once and cached.
pub struct ConfigManager {
    resolver: clapfig::Resolver<DodotConfig>,
    dotfiles_root: PathBuf,
}

impl ConfigManager {
    /// Create a new config manager for the given dotfiles root.
    ///
    /// Builds a clapfig Resolver that searches for `.dodot.toml` files
    /// using ancestor-walk from the resolve point up to (and including)
    /// the dotfiles root, identified by its `.git` directory. This
    /// prevents stray `.dodot.toml` files above the repo from leaking in.
    pub fn new(dotfiles_root: &Path) -> Result<Self> {
        let resolver = Clapfig::builder::<DodotConfig>()
            .app_name("dodot")
            .file_name(".dodot.toml")
            .search_paths(vec![SearchPath::Ancestors(Boundary::Marker(".git"))])
            .search_mode(SearchMode::Merge)
            .no_env()
            .build_resolver()
            .map_err(|e| DodotError::Config(format!("failed to build config resolver: {e}")))?;

        Ok(Self {
            resolver,
            dotfiles_root: dotfiles_root.to_path_buf(),
        })
    }

    /// Load the root-level configuration (no pack override).
    pub fn root_config(&self) -> Result<DodotConfig> {
        self.resolver
            .resolve_at(&self.dotfiles_root)
            .map_err(|e| DodotError::Config(format!("failed to load root config: {e}")))
    }

    /// Load merged configuration for a specific pack.
    ///
    /// Resolves by walking from `pack_path` up through ancestors,
    /// merging any `.dodot.toml` files found along the way (including
    /// the root config). Results are cached by absolute path.
    pub fn config_for_pack(&self, pack_path: &Path) -> Result<DodotConfig> {
        self.resolver
            .resolve_at(pack_path)
            .map_err(|e| DodotError::Config(format!("failed to load pack config: {e}")))
    }

    pub fn dotfiles_root(&self) -> &Path {
        &self.dotfiles_root
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fs::Fs;
    use crate::testing::TempEnvironment;

    #[test]
    fn default_config_has_expected_values() {
        // Load with no files — should use compiled defaults
        let env = TempEnvironment::builder().build();
        let mgr = ConfigManager::new(&env.dotfiles_root).unwrap();
        let cfg = mgr.root_config().unwrap();

        // ── pack.ignore defaults ────────────────────────────────
        let expected_ignore: Vec<String> = vec![
            ".git",
            ".svn",
            ".hg",
            "node_modules",
            ".DS_Store",
            "*.swp",
            "*~",
            "#*#",
            ".env*",
            ".terraform",
        ]
        .into_iter()
        .map(Into::into)
        .collect();
        assert_eq!(cfg.pack.ignore, expected_ignore);

        // ── symlink.force_home defaults ─────────────────────────
        let expected_force_home: Vec<String> = vec![
            "ssh",
            "aws",
            "kube",
            "bashrc",
            "zshrc",
            "profile",
            "bash_profile",
            "bash_login",
            "bash_logout",
            "inputrc",
        ]
        .into_iter()
        .map(Into::into)
        .collect();
        assert_eq!(cfg.symlink.force_home, expected_force_home);

        // ── symlink.protected_paths defaults ────────────────────
        let expected_protected: Vec<String> = vec![
            ".ssh/id_rsa",
            ".ssh/id_ed25519",
            ".ssh/id_dsa",
            ".ssh/id_ecdsa",
            ".ssh/authorized_keys",
            ".gnupg",
            ".aws/credentials",
            ".password-store",
            ".config/gh/hosts.yml",
            ".kube/config",
            ".docker/config.json",
        ]
        .into_iter()
        .map(Into::into)
        .collect();
        assert_eq!(cfg.symlink.protected_paths, expected_protected);

        // ── symlink.targets defaults ────────────────────────────
        assert!(cfg.symlink.targets.is_empty());

        // ── path defaults ──────────────────────────────────────
        assert!(cfg.path.auto_chmod_exec);

        // ── mappings defaults ───────────────────────────────────
        assert_eq!(cfg.mappings.path, "bin");
        assert_eq!(cfg.mappings.install, "install.sh");
        assert_eq!(cfg.mappings.homebrew, "Brewfile");
        assert_eq!(
            cfg.mappings.shell,
            vec!["aliases.sh", "profile.sh", "login.sh"]
        );
        assert!(cfg.mappings.skip.is_empty());
    }

    #[test]
    fn root_config_overrides_defaults() {
        let env = TempEnvironment::builder().build();

        // Write a root .dodot.toml
        env.fs
            .write_file(
                &env.dotfiles_root.join(".dodot.toml"),
                br#"
[mappings]
install = "setup.sh"
homebrew = "MyBrewfile"
"#,
            )
            .unwrap();

        let mgr = ConfigManager::new(&env.dotfiles_root).unwrap();
        let cfg = mgr.root_config().unwrap();

        assert_eq!(cfg.mappings.install, "setup.sh");
        assert_eq!(cfg.mappings.homebrew, "MyBrewfile");
        // Unset fields keep defaults
        assert_eq!(cfg.mappings.path, "bin");
    }

    #[test]
    fn pack_config_overrides_root() {
        let env = TempEnvironment::builder()
            .pack("vim")
            .file("vimrc", "x")
            .config(
                r#"
[pack]
ignore = ["*.bak"]

[mappings]
install = "vim-setup.sh"
"#,
            )
            .done()
            .build();

        // Root config
        env.fs
            .write_file(
                &env.dotfiles_root.join(".dodot.toml"),
                br#"
[mappings]
install = "install.sh"
homebrew = "RootBrewfile"
"#,
            )
            .unwrap();

        let mgr = ConfigManager::new(&env.dotfiles_root).unwrap();

        // Root config
        let root_cfg = mgr.root_config().unwrap();
        assert_eq!(root_cfg.mappings.install, "install.sh");

        // Pack config merges root + pack
        let pack_path = env.dotfiles_root.join("vim");
        let pack_cfg = mgr.config_for_pack(&pack_path).unwrap();
        assert_eq!(pack_cfg.mappings.install, "vim-setup.sh"); // overridden
        assert_eq!(pack_cfg.mappings.homebrew, "RootBrewfile"); // inherited
        assert_eq!(pack_cfg.pack.ignore, vec!["*.bak"]); // from pack
    }

    #[test]
    fn mappings_to_rules_produces_expected_rules() {
        let mappings = MappingsSection {
            path: "bin".into(),
            install: "install.sh".into(),
            shell: vec!["aliases.sh".into(), "profile.sh".into()],
            homebrew: "Brewfile".into(),
            skip: vec!["*.tmp".into()],
        };

        let rules = mappings_to_rules(&mappings);

        // Should have: path, install, 2x shell, homebrew, 1x exclude, catchall = 7
        assert_eq!(rules.len(), 7, "rules: {rules:#?}");

        let handler_names: Vec<&str> = rules.iter().map(|r| r.handler.as_str()).collect();
        assert!(handler_names.contains(&"path"));
        assert!(handler_names.contains(&"install"));
        assert!(handler_names.contains(&"shell"));
        assert!(handler_names.contains(&"homebrew"));
        assert!(handler_names.contains(&"exclude"));
        assert!(handler_names.contains(&"symlink"));

        // Exclusion rule should have ! prefix
        let exclude = rules.iter().find(|r| r.handler == "exclude").unwrap();
        assert!(exclude.pattern.starts_with('!'));

        // Catchall should be lowest priority
        let catchall = rules.iter().find(|r| r.pattern == "*").unwrap();
        assert_eq!(catchall.priority, 0);
    }

    #[test]
    fn to_handler_config_converts_correctly() {
        let env = TempEnvironment::builder().build();
        let mgr = ConfigManager::new(&env.dotfiles_root).unwrap();
        let cfg = mgr.root_config().unwrap();

        let hcfg = cfg.to_handler_config();
        assert_eq!(hcfg.force_home, cfg.symlink.force_home);
        assert_eq!(hcfg.protected_paths, cfg.symlink.protected_paths);
    }
}
