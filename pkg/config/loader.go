package config

import (
	"fmt"
	"os"
	"path/filepath"

	"github.com/knadh/koanf/parsers/toml"
	"github.com/knadh/koanf/providers/file"
	"github.com/knadh/koanf/v2"
)

// LoadConfiguration loads the main configuration
func LoadConfiguration() (*Config, error) {
	dotfilesRoot := os.Getenv("DOTFILES_ROOT")
	if dotfilesRoot == "" {
		dotfilesRoot = "."
	}

	k, err := loadKoanfConfig(dotfilesRoot)
	if err != nil {
		return nil, err
	}

	return koanfToConfig(k)
}

// GetRootConfig loads root configuration and returns Config struct
func GetRootConfig(dotfilesRoot string) (*Config, error) {
	k, err := loadKoanfConfig(dotfilesRoot)
	if err != nil {
		return nil, err
	}

	return koanfToConfig(k)
}

// GetPackConfig loads pack configuration and returns Config struct
func GetPackConfig(rootConfig *Config, packPath string) (*Config, error) {
	// Get dotfiles root from environment or use current dir
	dotfilesRoot := os.Getenv("DOTFILES_ROOT")
	if dotfilesRoot == "" {
		dotfilesRoot = "."
	}

	k, err := loadPackKoanfConfig(dotfilesRoot, packPath)
	if err != nil {
		return nil, err
	}

	return koanfToConfig(k)
}

// loadKoanfConfig loads configuration using koanf
func loadKoanfConfig(dotfilesRoot string) (*koanf.Koanf, error) {
	k := koanf.New(".")

	// 1. Load defaults
	if err := k.Load(&rawBytesProvider{bytes: defaultConfig}, toml.Parser()); err != nil {
		return nil, fmt.Errorf("failed to load defaults: %w", err)
	}

	// 2. Load app config
	if err := k.Load(&rawBytesProvider{bytes: appConfig}, toml.Parser()); err != nil {
		return nil, fmt.Errorf("failed to load app config: %w", err)
	}

	// Save defaults for fields that should merge instead of replace
	defaultProtectedPaths := k.Strings("symlink.protected_paths")
	defaultForceHome := k.Strings("symlink.force_home")
	defaultPackIgnore := k.Strings("pack.ignore")

	// 3. Load root config if it exists
	for _, filename := range []string{".dodot.toml", "dodot.toml"} {
		path := filepath.Join(dotfilesRoot, filename)
		if _, err := os.Stat(path); err == nil {
			if err := k.Load(file.Provider(path), toml.Parser()); err != nil {
				return nil, fmt.Errorf("failed to load root config from %s: %w", path, err)
			}

			// Merge fields that should be additive
			userProtectedPaths := k.Strings("symlink.protected_paths")
			userForceHome := k.Strings("symlink.force_home")
			userPackIgnore := k.Strings("pack.ignore")

			// Combine defaults with user values
			allProtectedPaths := append(defaultProtectedPaths, userProtectedPaths...)
			allForceHome := append(defaultForceHome, userForceHome...)
			allPackIgnore := append(defaultPackIgnore, userPackIgnore...)

			// Set the merged values back
			_ = k.Set("symlink.protected_paths", allProtectedPaths)
			_ = k.Set("symlink.force_home", allForceHome)
			_ = k.Set("pack.ignore", allPackIgnore)

			break
		}
	}

	return k, nil
}

// loadPackKoanfConfig loads pack configuration
func loadPackKoanfConfig(dotfilesRoot, packPath string) (*koanf.Koanf, error) {
	// Load all configs in order
	k := koanf.New(".")

	// 1. Load defaults
	if err := k.Load(&rawBytesProvider{bytes: defaultConfig}, toml.Parser()); err != nil {
		return nil, fmt.Errorf("failed to load defaults: %w", err)
	}

	// 2. Load app config
	if err := k.Load(&rawBytesProvider{bytes: appConfig}, toml.Parser()); err != nil {
		return nil, fmt.Errorf("failed to load app config: %w", err)
	}

	// Save defaults for fields that should merge
	defaultProtectedPaths := k.Strings("symlink.protected_paths")
	defaultForceHome := k.Strings("symlink.force_home")
	defaultPackIgnore := k.Strings("pack.ignore")

	// 3. Load root config if it exists
	for _, filename := range []string{".dodot.toml", "dodot.toml"} {
		rootPath := filepath.Join(dotfilesRoot, filename)
		if _, err := os.Stat(rootPath); err == nil {
			if err := k.Load(file.Provider(rootPath), toml.Parser()); err != nil {
				return nil, fmt.Errorf("failed to load root config from %s: %w", rootPath, err)
			}

			// Merge fields that should be additive
			rootProtectedPaths := k.Strings("symlink.protected_paths")
			rootForceHome := k.Strings("symlink.force_home")
			rootPackIgnore := k.Strings("pack.ignore")

			allProtectedPaths := append(defaultProtectedPaths, rootProtectedPaths...)
			allForceHome := append(defaultForceHome, rootForceHome...)
			allPackIgnore := append(defaultPackIgnore, rootPackIgnore...)

			_ = k.Set("symlink.protected_paths", allProtectedPaths)
			_ = k.Set("symlink.force_home", allForceHome)
			_ = k.Set("pack.ignore", allPackIgnore)

			// Update for next merge
			defaultProtectedPaths = allProtectedPaths
			defaultForceHome = allForceHome
			defaultPackIgnore = allPackIgnore

			break
		}
	}

	// 4. Load pack config if it exists
	for _, filename := range []string{".dodot.toml", "dodot.toml"} {
		path := filepath.Join(packPath, filename)
		if _, err := os.Stat(path); err == nil {
			if err := k.Load(file.Provider(path), toml.Parser()); err != nil {
				return nil, fmt.Errorf("failed to load pack config from %s: %w", path, err)
			}

			// Merge fields that should be additive
			packProtectedPaths := k.Strings("symlink.protected_paths")
			packForceHome := k.Strings("symlink.force_home")
			packPackIgnore := k.Strings("pack.ignore")

			allProtectedPaths := append(defaultProtectedPaths, packProtectedPaths...)
			allForceHome := append(defaultForceHome, packForceHome...)
			allPackIgnore := append(defaultPackIgnore, packPackIgnore...)

			_ = k.Set("symlink.protected_paths", allProtectedPaths)
			_ = k.Set("symlink.force_home", allForceHome)
			_ = k.Set("pack.ignore", allPackIgnore)

			break
		}
	}

	return k, nil
}

// koanfToConfig converts koanf data to Config struct
func koanfToConfig(k *koanf.Koanf) (*Config, error) {
	cfg := &Config{}

	// Security - handle both user and internal formats
	cfg.Security.ProtectedPaths = make(map[string]bool)
	// Get all protected paths from the merged config
	if paths := k.Strings("symlink.protected_paths"); len(paths) > 0 {
		for _, p := range paths {
			cfg.Security.ProtectedPaths[p] = true
		}
	}
	// Also check internal format for backward compatibility
	if paths := k.Strings("security.protected_paths"); len(paths) > 0 {
		for _, p := range paths {
			cfg.Security.ProtectedPaths[p] = true
		}
	}

	// Patterns - handle both formats
	if ignore := k.Strings("pack.ignore"); len(ignore) > 0 {
		cfg.Patterns.PackIgnore = ignore
	} else if ignore := k.Strings("patterns.pack_ignore"); len(ignore) > 0 {
		cfg.Patterns.PackIgnore = ignore
	}

	// Special files
	cfg.Patterns.SpecialFiles.PackConfig = k.String("patterns.special_files.pack_config")
	if cfg.Patterns.SpecialFiles.PackConfig == "" {
		cfg.Patterns.SpecialFiles.PackConfig = ".dodot.toml"
	}
	cfg.Patterns.SpecialFiles.IgnoreFile = k.String("patterns.special_files.ignore_file")
	if cfg.Patterns.SpecialFiles.IgnoreFile == "" {
		cfg.Patterns.SpecialFiles.IgnoreFile = ".dodotignore"
	}

	// Link paths - handle both formats
	cfg.LinkPaths.CoreUnixExceptions = make(map[string]bool)
	// Get all force_home paths from the merged config
	if forceHome := k.Strings("symlink.force_home"); len(forceHome) > 0 {
		for _, p := range forceHome {
			cfg.LinkPaths.CoreUnixExceptions[p] = true
		}
	}
	// Also check internal format for backward compatibility
	if forceHome := k.Strings("link_paths.force_home"); len(forceHome) > 0 {
		for _, p := range forceHome {
			cfg.LinkPaths.CoreUnixExceptions[p] = true
		}
	}

	// File permissions
	cfg.FilePermissions.Directory = os.FileMode(k.Int64("file_permissions.directory"))
	if cfg.FilePermissions.Directory == 0 {
		cfg.FilePermissions.Directory = 0755
	}
	cfg.FilePermissions.File = os.FileMode(k.Int64("file_permissions.file"))
	if cfg.FilePermissions.File == 0 {
		cfg.FilePermissions.File = 0644
	}
	cfg.FilePermissions.Executable = os.FileMode(k.Int64("file_permissions.executable"))
	if cfg.FilePermissions.Executable == 0 {
		cfg.FilePermissions.Executable = 0755
	}

	// Shell integration
	cfg.ShellIntegration.BashZshSnippet = k.String("shell_integration.bash_zsh_snippet")
	cfg.ShellIntegration.BashZshSnippetWithCustom = k.String("shell_integration.bash_zsh_snippet_with_custom")
	cfg.ShellIntegration.FishSnippet = k.String("shell_integration.fish_snippet")

	// Mappings
	cfg.Mappings.Path = k.String("mappings.path")
	cfg.Mappings.Install = k.String("mappings.install")
	cfg.Mappings.Shell = k.Strings("mappings.shell")
	cfg.Mappings.Homebrew = k.String("mappings.homebrew")
	cfg.Mappings.Ignore = k.Strings("mappings.ignore")

	// Generate rules from mappings
	cfg.Rules = cfg.GenerateRulesFromMapping()
	// Add default rules
	cfg.Rules = append(defaultRules(), cfg.Rules...)

	// Set catchall exclude
	cfg.Patterns.CatchallExclude = []string{
		cfg.Patterns.SpecialFiles.PackConfig,
		cfg.Patterns.SpecialFiles.IgnoreFile,
	}

	return cfg, nil
}
