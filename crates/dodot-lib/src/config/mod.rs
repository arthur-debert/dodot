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

    #[config(nested)]
    pub profiling: ProfilingSection,

    #[config(nested)]
    pub secret: SecretSection,
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

    /// Whether `_app/` and `app_aliases` route through the macOS
    /// `~/Library/Application Support` root. Defaults to `true` on
    /// macOS, ignored on other platforms (where `app_support_dir`
    /// always collapses to `xdg_config_home`).
    ///
    /// Setting this to `false` on macOS opts the user into Linux-style
    /// `~/.config` placement for *every* `_app/` and `app_aliases`
    /// entry. `_lib/` is unaffected — it explicitly targets
    /// `~/Library/`.
    ///
    /// See `docs/proposals/macos-paths.lex` §11.2.
    #[config(default = true)]
    pub app_uses_library: bool,

    /// Curated list of GUI-app folder names whose first path segment
    /// routes to `<app_support_dir>/<seg>/<rest>` without requiring a
    /// `_app/` prefix in the pack tree. Capped at 100 entries; see
    /// `docs/proposals/macos-paths.lex` §3.4.
    ///
    /// Matching is case-sensitive (Library folder names are case-sensitive
    /// on macOS) and on the first path segment only.
    #[config(default = ["Code", "Cursor", "Zed", "Emacs"])]
    pub force_app: Vec<String>,

    /// Pack-name → GUI-app folder name rewrites. When the pack name
    /// matches a key here, the resolver's default rule reroutes to
    /// `<app_support_dir>/<value>/<rel_path>`. See
    /// `docs/proposals/macos-paths.lex` §3.3.
    #[config(default = {})]
    pub app_aliases: std::collections::HashMap<String, String>,

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

    /// Filename suffixes (without leading dot) that should be detected
    /// as plists for `dodot git-install-filters` adopt hints and the
    /// `.gitattributes` line. Defaults to `["plist"]`. Some apps store
    /// plists with non-standard suffixes (`.binplist`, `.savedState`,
    /// etc.); register additional extensions here to flow them through
    /// the same clean/smudge pipeline.
    ///
    /// Comparison is case-insensitive, matching the existing detection
    /// behavior. Honors the standard root → pack inheritance.
    /// See `docs/proposals/plists.lex` §8.1.
    #[config(default = ["plist"])]
    pub plist_extensions: Vec<String>,
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

    #[config(nested)]
    pub age: PreprocessorAgeSection,

    #[config(nested)]
    pub gpg: PreprocessorGpgSection,
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

    /// Glob patterns for source files whose reverse-merge should be
    /// skipped. Templates matching are still rendered on `dodot up` and
    /// tracked in the divergence cache, but `dodot transform check` and
    /// the clean filter both bypass the burgertocow reverse-merge step
    /// (echo stdin / report-only). Useful for templates that are mostly
    /// dynamic — the heuristic degrades there and produces more conflict
    /// markers than usable diffs.
    ///
    /// Patterns are matched against the source path's filename component
    /// (e.g. `"complex-config.toml.tmpl"`, `"*.gen.tmpl"`).
    #[config(default = [])]
    pub no_reverse: Vec<String>,
}

/// `age` whole-file decryption preprocessor settings
/// (`docs/proposals/secrets.lex` §4).
///
/// Default-disabled so a fresh dodot install never shells out to
/// `age` against random files; users opt in by flipping `enabled =
/// true` in their root `.dodot.toml`. The identity path defaults to
/// `~/.config/age/identity.txt` (the conventional `age-keygen`
/// destination); set explicitly when storing keys elsewhere or
/// rotating identities per-pack.
#[derive(Config, Debug, Clone, Serialize, Deserialize)]
pub struct PreprocessorAgeSection {
    /// Whether `*.age` files are matched and decrypted on `dodot
    /// up`. Default false — opt-in posture mirrors the
    /// `[secret.providers.*]` blocks.
    #[config(default = false)]
    pub enabled: bool,

    /// File extensions that trigger age decryption. Same shape as
    /// `template.extensions`; multi-extension config is mostly
    /// useful for users whose conventions diverge (e.g. `.age.txt`).
    #[config(default = ["age"])]
    pub extensions: Vec<String>,

    /// Path to the age identity file. Empty (the default) defers to
    /// the runtime: `$AGE_IDENTITY` env var, then
    /// `~/.config/age/identity.txt`.
    #[config(default = "")]
    pub identity: String,
}

/// `gpg` whole-file decryption preprocessor settings
/// (`docs/proposals/secrets.lex` §4).
///
/// Same opt-in posture as `age`. gpg picks up its identity from
/// gpg-agent so there's no `identity` field — auth is the user's
/// existing gpg setup, not dodot's job to configure.
#[derive(Config, Debug, Clone, Serialize, Deserialize)]
pub struct PreprocessorGpgSection {
    /// Whether `*.gpg` files are matched and decrypted on
    /// `dodot up`. Default false — opt-in.
    #[config(default = false)]
    pub enabled: bool,

    /// File extensions that trigger gpg decryption. Default
    /// `["gpg"]` only. **Do not include `asc` here unless your
    /// dotfiles repo only stores ASCII-armored *encrypted*
    /// payloads under that suffix.** `.asc` is conventionally used
    /// for armored *public keys* and *detached signatures* (release
    /// signatures, package-manager keys), neither of which gpg
    /// will decrypt; routing them through `gpg --decrypt` produces
    /// confusing failures. Users storing armored encrypted
    /// payloads as `.asc` opt in by setting
    /// `extensions = ["gpg", "asc"]` explicitly.
    #[config(default = ["gpg"])]
    pub extensions: Vec<String>,
}

/// Shell-init profiling settings. Root-only — per-pack overrides are
/// meaningless (the init script is one thing; you can't half-profile it).
///
/// See `docs/proposals/profiling.lex` for the full design.
#[derive(Config, Debug, Clone, Serialize, Deserialize)]
pub struct ProfilingSection {
    /// Whether the generated `dodot-init.sh` carries the timing wrapper
    /// around each `source` and PATH line. When false, the init script
    /// is byte-identical to the pre-Phase-2 form. When true, bash 5+ /
    /// zsh sessions emit one TSV per shell startup under
    /// `<data_dir>/probes/shell-init/`; older shells fall through to
    /// the no-op path even with the wrapper present.
    #[config(default = true)]
    pub enabled: bool,

    /// Maximum number of `<data_dir>/probes/shell-init/profile-*.tsv`
    /// files to retain. Older files are pruned at the end of every
    /// `dodot up`. At ~4 KB per run, the default budget is roughly
    /// 400 KB on disk.
    #[config(default = 100)]
    pub keep_last_runs: usize,
}

/// Secret-handling settings (`docs/proposals/secrets.lex`).
///
/// Top-level kill switch + per-provider blocks. Disabling the
/// section globally (`[secret] enabled = false`) is equivalent to
/// disabling every provider; templates that call `secret(...)` then
/// surface a "no providers configured" render error.
///
/// **This section is root-only.** Unlike most config sections, the
/// `[secret]` block is always read from the root `.dodot.toml`;
/// per-pack overrides are ignored. Secret tooling
/// (`$PASSWORD_STORE_DIR`, `OP_SERVICE_ACCOUNT_TOKEN`, the binaries
/// themselves) is a property of the user's environment, not of any
/// individual pack — a pack-level override would invalidate the
/// once-per-run preflight contract (`secrets.lex` §5.4) and would
/// surface as confusing "secret X probed under config A but
/// resolved under config B" failures. Treat the root section as the
/// single source of truth.
#[derive(Config, Debug, Clone, Serialize, Deserialize)]
pub struct SecretSection {
    /// Master switch. Default true; flip to false to disable all
    /// secret resolution without removing the per-provider blocks.
    #[config(default = true)]
    pub enabled: bool,

    #[config(nested)]
    pub providers: SecretProvidersSection,
}

/// Per-provider configuration. Each block has an `enabled` flag plus
/// any provider-specific knobs (e.g. `pass.store_dir`). Providers
/// disabled here are not registered in the runtime
/// `SecretRegistry`; references to their schemes raise
/// "no provider for scheme" at resolution time.
#[derive(Config, Debug, Clone, Serialize, Deserialize)]
pub struct SecretProvidersSection {
    #[config(nested)]
    pub pass: SecretProviderPass,

    #[config(nested)]
    pub op: SecretProviderOp,

    #[config(nested)]
    pub bw: SecretProviderBw,

    #[config(nested)]
    pub sops: SecretProviderSops,

    #[config(nested)]
    pub keychain: SecretProviderKeychain,

    /// Note the TOML key here is `secret_tool` (underscore), even
    /// though the scheme prefix in `secret(...)` references is
    /// `secret-tool:` (hyphen, matching the binary name). The
    /// reason: confique's `Config` derive (re-exported from
    /// clapfig as `Config`) maps each TOML key 1:1 to a Rust
    /// struct field name, and Rust identifiers can't contain
    /// hyphens — `pub secret-tool: ...` won't compile. TOML
    /// itself accepts bare hyphenated keys; it's the Rust-side
    /// field-name constraint that forces the underscore form.
    /// User-facing error messages translate via
    /// [`crate::secret::registry::scheme_to_config_key`] so a
    /// "no provider for scheme `secret-tool`" hint suggests the
    /// correct `[secret.providers.secret_tool]` block.
    #[config(nested)]
    pub secret_tool: SecretProviderSecretTool,
}

/// `pass` (password-store) provider config.
#[derive(Config, Debug, Clone, Serialize, Deserialize)]
pub struct SecretProviderPass {
    /// Whether the `pass:` scheme is registered. Default false —
    /// users opt in explicitly so a freshly-installed dodot doesn't
    /// shell out to `pass` on every render.
    #[config(default = false)]
    pub enabled: bool,

    /// Override `$PASSWORD_STORE_DIR`. Empty (the default) leaves
    /// dodot reading the env var, which falls back to
    /// `$HOME/.password-store`.
    #[config(default = "")]
    pub store_dir: String,
}

/// `op` (1Password CLI) provider config.
#[derive(Config, Debug, Clone, Serialize, Deserialize)]
pub struct SecretProviderOp {
    /// Whether the `op://` scheme is registered. Default false —
    /// same opt-in posture as `pass`.
    #[config(default = false)]
    pub enabled: bool,
}

/// `bw` (Bitwarden CLI) provider config.
#[derive(Config, Debug, Clone, Serialize, Deserialize)]
pub struct SecretProviderBw {
    /// Whether the `bw:` scheme is registered. Default false —
    /// same opt-in posture as `pass` and `op`.
    #[config(default = false)]
    pub enabled: bool,
}

/// `sops` (Mozilla SOPS) provider config.
#[derive(Config, Debug, Clone, Serialize, Deserialize)]
pub struct SecretProviderSops {
    /// Whether the `sops:` scheme is registered. Default false —
    /// same opt-in posture as the other providers.
    #[config(default = false)]
    pub enabled: bool,
}

/// `keychain` (macOS Keychain via `security`) provider config.
///
/// macOS-only; on other platforms the provider's `probe()`
/// surfaces `NotInstalled` with a "use secret-tool instead"
/// pointer. Default `enabled = false` matches the rest of the
/// secret providers — opt-in posture.
#[derive(Config, Debug, Clone, Serialize, Deserialize)]
pub struct SecretProviderKeychain {
    #[config(default = false)]
    pub enabled: bool,
}

/// `secret-tool` (freedesktop Secret Service via `secret-tool`)
/// provider config.
///
/// Linux-first; on macOS the provider's `probe()` redirects users
/// to the `keychain` provider. Default `enabled = false`. The
/// scheme prefix in references is `secret-tool:` (hyphen) — see
/// the comment on `SecretProvidersSection::secret_tool` for the
/// reason the config field uses the underscore form instead.
#[derive(Config, Debug, Clone, Serialize, Deserialize)]
pub struct SecretProviderSecretTool {
    #[config(default = false)]
    pub enabled: bool,
}

/// File-to-handler mapping patterns.
#[derive(Config, Debug, Clone, Serialize, Deserialize)]
pub struct MappingsSection {
    /// Directory name pattern for PATH handler.
    #[config(default = "bin")]
    pub path: String,

    /// Filename patterns for install scripts.
    ///
    /// The extension selects the interpreter used to run the script
    /// (`.sh`/`.bash` → `bash`, `.zsh` → `zsh`); see the install handler
    /// for the exact mapping.
    #[config(default = ["install.sh", "install.bash", "install.zsh"])]
    pub install: Vec<String>,

    /// Filename patterns for shell scripts to source at login.
    ///
    /// Sourced files run *in the user's shell* (whichever shell reads
    /// `dodot-init.sh`), so `.zsh` files will only parse cleanly in zsh
    /// sessions and `.bash` files in bash sessions. `.sh` is the
    /// portable bucket for snippets that work in either.
    #[config(default = [
        "aliases.sh", "aliases.bash", "aliases.zsh",
        "profile.sh", "profile.bash", "profile.zsh",
        "login.sh", "login.bash", "login.zsh",
        "env.sh", "env.bash", "env.zsh",
    ])]
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
            // `force_app` and `app_aliases` always pass through to the
            // resolver. On non-macOS (and on macOS with
            // `app_uses_library = false`) the `app_support_dir` accessor
            // already collapses to `xdg_config_home`, so the routing is
            // mechanically correct without an extra branch here.
            force_app: self.symlink.force_app.clone(),
            app_aliases: self.symlink.app_aliases.clone(),
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
    for pattern in &mappings.install {
        if !pattern.is_empty() {
            rules.push(Rule {
                pattern: pattern.clone(),
                handler: "install".into(),
                priority: 10,
                options: HashMap::new(),
            });
        }
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
        assert_eq!(
            cfg.mappings.install,
            vec!["install.sh", "install.bash", "install.zsh"]
        );
        assert_eq!(cfg.mappings.homebrew, "Brewfile");
        assert_eq!(
            cfg.mappings.shell,
            vec![
                "aliases.sh",
                "aliases.bash",
                "aliases.zsh",
                "profile.sh",
                "profile.bash",
                "profile.zsh",
                "login.sh",
                "login.bash",
                "login.zsh",
                "env.sh",
                "env.bash",
                "env.zsh",
            ]
        );
        assert!(cfg.mappings.skip.is_empty());

        // ── profiling defaults ──────────────────────────────────
        assert!(cfg.profiling.enabled);
        assert_eq!(cfg.profiling.keep_last_runs, 100);
    }

    #[test]
    fn profiling_section_overridable() {
        let env = TempEnvironment::builder().build();
        env.fs
            .write_file(
                &env.dotfiles_root.join(".dodot.toml"),
                b"[profiling]\nenabled = false\nkeep_last_runs = 25\n",
            )
            .unwrap();

        let mgr = ConfigManager::new(&env.dotfiles_root).unwrap();
        let cfg = mgr.root_config().unwrap();
        assert!(!cfg.profiling.enabled);
        assert_eq!(cfg.profiling.keep_last_runs, 25);
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
install = ["setup.sh"]
homebrew = "MyBrewfile"
"#,
            )
            .unwrap();

        let mgr = ConfigManager::new(&env.dotfiles_root).unwrap();
        let cfg = mgr.root_config().unwrap();

        assert_eq!(cfg.mappings.install, vec!["setup.sh"]);
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
install = ["vim-setup.sh"]
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
install = ["install.sh"]
homebrew = "RootBrewfile"
"#,
            )
            .unwrap();

        let mgr = ConfigManager::new(&env.dotfiles_root).unwrap();

        // Root config
        let root_cfg = mgr.root_config().unwrap();
        assert_eq!(root_cfg.mappings.install, vec!["install.sh"]);

        // Pack config merges root + pack
        let pack_path = env.dotfiles_root.join("vim");
        let pack_cfg = mgr.config_for_pack(&pack_path).unwrap();
        assert_eq!(pack_cfg.mappings.install, vec!["vim-setup.sh"]); // overridden
        assert_eq!(pack_cfg.mappings.homebrew, "RootBrewfile"); // inherited
        assert_eq!(pack_cfg.pack.ignore, vec!["*.bak"]); // from pack
    }

    #[test]
    fn mappings_to_rules_produces_expected_rules() {
        let mappings = MappingsSection {
            path: "bin".into(),
            install: vec!["install.sh".into(), "install.zsh".into()],
            shell: vec!["aliases.sh".into(), "profile.sh".into()],
            homebrew: "Brewfile".into(),
            skip: vec!["*.tmp".into()],
        };

        let rules = mappings_to_rules(&mappings);

        // Should have: path, 2x install, 2x shell, homebrew, 1x exclude, catchall = 8
        assert_eq!(rules.len(), 8, "rules: {rules:#?}");

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
        assert_eq!(hcfg.force_app, cfg.symlink.force_app);
        assert_eq!(hcfg.app_aliases, cfg.symlink.app_aliases);
        assert_eq!(hcfg.protected_paths, cfg.symlink.protected_paths);
    }

    /// Hard cap on the seeded `force_app` defaults — see
    /// `docs/proposals/macos-paths.lex` §3.4.1. Adding entry 101 is
    /// supposed to be a forcing function to drop the weakest-justified
    /// existing entry; this test makes that forcing function *visible*
    /// rather than relying on review discipline alone.
    #[test]
    fn default_force_app_under_hundred_entry_cap() {
        let env = TempEnvironment::builder().build();
        let mgr = ConfigManager::new(&env.dotfiles_root).unwrap();
        let cfg = mgr.root_config().unwrap();
        assert!(
            cfg.symlink.force_app.len() <= 100,
            "force_app default has {} entries; cap is 100. \
             Drop the weakest-justified entry before adding another. \
             See docs/proposals/macos-paths.lex §3.4.1.",
            cfg.symlink.force_app.len()
        );
    }

    /// Compile-time sanity on the seeded force_app entries. These ship
    /// in the default config and must stay correctly capitalized to
    /// match the actual macOS Application Support folder names.
    #[test]
    fn default_force_app_seed_contains_expected_entries() {
        let env = TempEnvironment::builder().build();
        let mgr = ConfigManager::new(&env.dotfiles_root).unwrap();
        let cfg = mgr.root_config().unwrap();
        for expected in ["Code", "Cursor", "Zed", "Emacs"] {
            assert!(
                cfg.symlink.force_app.iter().any(|e| e == expected),
                "expected default force_app to contain `{expected}`; got {:?}",
                cfg.symlink.force_app
            );
        }
    }

    #[test]
    fn app_uses_library_default_is_true() {
        let env = TempEnvironment::builder().build();
        let mgr = ConfigManager::new(&env.dotfiles_root).unwrap();
        let cfg = mgr.root_config().unwrap();
        assert!(
            cfg.symlink.app_uses_library,
            "app_uses_library must default to true; macOS gets the Library \
             root, Linux already collapses app_support_dir to xdg_config_home"
        );
    }

    #[test]
    fn app_aliases_overridable_in_root_config() {
        let env = TempEnvironment::builder().build();
        env.fs
            .write_file(
                &env.dotfiles_root.join(".dodot.toml"),
                br#"
[symlink.app_aliases]
vscode = "Code"
warp = "dev.warp.Warp-Stable"
"#,
            )
            .unwrap();
        let mgr = ConfigManager::new(&env.dotfiles_root).unwrap();
        let cfg = mgr.root_config().unwrap();
        assert_eq!(
            cfg.symlink.app_aliases.get("vscode").map(String::as_str),
            Some("Code")
        );
        assert_eq!(
            cfg.symlink.app_aliases.get("warp").map(String::as_str),
            Some("dev.warp.Warp-Stable")
        );
    }
}
