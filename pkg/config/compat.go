package config

import (
	"os"

	"github.com/knadh/koanf/v2"
)

// GetRootConfigNew loads root config using the simplified koanf approach
// but returns a Config struct for backward compatibility
func GetRootConfigNew(dotfilesRoot string) (*Config, error) {
	k, err := GetSimpleRootConfig(dotfilesRoot)
	if err != nil {
		return nil, err
	}

	return koanfToConfig(k)
}

// GetPackConfigNew loads pack config using the simplified koanf approach
// but returns a Config struct for backward compatibility
func GetPackConfigNew(rootConfig *Config, packPath string) (*Config, error) {
	// We need the dotfiles root - get it from env or use current dir
	dotfilesRoot := os.Getenv("DOTFILES_ROOT")
	if dotfilesRoot == "" {
		dotfilesRoot = "."
	}

	// Get pack config with proper layering
	k, err := GetSimplePackConfig(dotfilesRoot, packPath)
	if err != nil {
		return nil, err
	}

	return koanfToConfig(k)
}

// koanfToConfig converts a koanf instance to a Config struct
// This handles the user format -> internal format mapping
func koanfToConfig(k *koanf.Koanf) (*Config, error) {
	cfg := &Config{}

	// Map user format to internal format manually
	// This is temporary until we can update all code to use koanf directly

	// Security
	cfg.Security.ProtectedPaths = make(map[string]bool)
	if paths := k.Strings("security.protected_paths"); len(paths) > 0 {
		for _, p := range paths {
			cfg.Security.ProtectedPaths[p] = true
		}
	} else if paths := k.Strings("symlink.protected_paths"); len(paths) > 0 {
		// Support user format
		for _, p := range paths {
			cfg.Security.ProtectedPaths[p] = true
		}
	}

	// Patterns
	if ignore := k.Strings("patterns.pack_ignore"); len(ignore) > 0 {
		cfg.Patterns.PackIgnore = ignore
	} else if ignore := k.Strings("pack.ignore"); len(ignore) > 0 {
		// Support user format
		cfg.Patterns.PackIgnore = ignore
	}
	cfg.Patterns.CatchallExclude = k.Strings("patterns.catchall_exclude")
	cfg.Patterns.SpecialFiles.PackConfig = k.String("patterns.special_files.pack_config")
	cfg.Patterns.SpecialFiles.IgnoreFile = k.String("patterns.special_files.ignore_file")

	// Link paths
	cfg.LinkPaths.CoreUnixExceptions = make(map[string]bool)
	// Try user format first
	if forceHome := k.Strings("symlink.force_home"); len(forceHome) > 0 {
		for _, p := range forceHome {
			cfg.LinkPaths.CoreUnixExceptions[p] = true
		}
	} else if forceHome := k.Strings("link_paths.force_home"); len(forceHome) > 0 {
		// Try internal format
		for _, p := range forceHome {
			cfg.LinkPaths.CoreUnixExceptions[p] = true
		}
	}

	// File permissions
	cfg.FilePermissions.Directory = os.FileMode(k.Int64("file_permissions.directory"))
	cfg.FilePermissions.File = os.FileMode(k.Int64("file_permissions.file"))
	cfg.FilePermissions.Executable = os.FileMode(k.Int64("file_permissions.executable"))

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

	// Post-process for any derived values
	if cfg.Patterns.SpecialFiles.PackConfig != "" || cfg.Patterns.SpecialFiles.IgnoreFile != "" {
		cfg.Patterns.CatchallExclude = []string{
			cfg.Patterns.SpecialFiles.PackConfig,
			cfg.Patterns.SpecialFiles.IgnoreFile,
		}
	}

	return cfg, nil
}
