package config

import (
	"os"
	"strings"
)

// Security holds security-related configuration
type Security struct {
	// ProtectedPaths defines paths that should not be symlinked for security reasons
	ProtectedPaths map[string]bool `koanf:"protected_paths"`
}

// Patterns holds various ignore and exclude patterns
type Patterns struct {
	PackIgnore      []string     `koanf:"pack_ignore" yaml:"packIgnore" json:"packIgnore"`
	CatchallExclude []string     `koanf:"catchall_exclude" yaml:"catchallExclude" json:"catchallExclude"`
	SpecialFiles    SpecialFiles `koanf:"special_files" yaml:"specialFiles" json:"specialFiles"`
}

// SpecialFiles holds names of special configuration files
type SpecialFiles struct {
	PackConfig string `koanf:"pack_config"`
	IgnoreFile string `koanf:"ignore_file"`
}

// Rule defines a pattern-to-handler mapping
type Rule struct {
	Pattern string                 `koanf:"pattern" yaml:"pattern" json:"pattern"`
	Handler string                 `koanf:"handler" yaml:"handler" json:"handler"`
	Options map[string]interface{} `koanf:"options" yaml:"options" json:"options"`
}

// FilePermissions holds file and directory permission settings
// IMPORTANT: These permissions are intentionally NOT used throughout the codebase.
// File permissions (0755, 0644, etc.) should remain hardcoded where they are used
// as they are security-critical and context-specific. This struct exists only for
// potential future use cases where centralized permissions might be beneficial.
type FilePermissions struct {
	Directory  os.FileMode `koanf:"directory"`
	File       os.FileMode `koanf:"file"`
	Executable os.FileMode `koanf:"executable"`
}

// ShellIntegration holds shell integration snippets
type ShellIntegration struct {
	BashZshSnippet           string `koanf:"bash_zsh_snippet"`
	BashZshSnippetWithCustom string `koanf:"bash_zsh_snippet_with_custom"`
	FishSnippet              string `koanf:"fish_snippet"`
}

// Paths holds path-related configuration
// NOTE: Internal datastore paths (StateDir, BackupsDir, etc.) are defined in
// pkg/paths/paths.go and are NOT user-configurable. They are part of dodot's
// internal structure and should remain consistent across all installations.
// This struct intentionally left empty for now but may hold user-configurable
// paths in the future.
type Paths struct {
	// Reserved for future user-configurable paths
}

// LinkPaths holds link path mapping configuration
type LinkPaths struct {
	// CoreUnixExceptions lists tools that should always deploy to $HOME
	// These are typically security-critical or shell-expected locations
	// Release C: Layer 2 - Exception List
	CoreUnixExceptions map[string]bool `koanf:"force_home"`
}

// Mappings holds file name to handler mappings
type Mappings struct {
	// Path specifies directory names that should be added to PATH
	Path string `koanf:"path" toml:"path"`
	// Install specifies the filename pattern for install scripts
	Install string `koanf:"install" toml:"install"`
	// Shell specifies filename patterns for shell scripts
	Shell []string `koanf:"shell" toml:"shell"`
	// Homebrew specifies the filename pattern for Homebrew files
	Homebrew string `koanf:"homebrew" toml:"homebrew"`
	// Ignore specifies patterns for files to exclude from processing
	Ignore []string `koanf:"ignore" toml:"ignore"`
}

// Symlink holds symlink-specific configuration that matches TOML structure
type Symlink struct {
	ProtectedPaths []string `koanf:"protected_paths" toml:"protected_paths"`
	ForceHome      []string `koanf:"force_home" toml:"force_home"`
}

// Pack holds pack-specific configuration that matches TOML structure
type Pack struct {
	Ignore []string `koanf:"ignore" toml:"ignore"`
}

// Config is the main configuration structure
type Config struct {
	// Direct TOML sections
	Pack             Pack             `koanf:"pack"`
	Symlink          Symlink          `koanf:"symlink"`
	FilePermissions  FilePermissions  `koanf:"file_permissions"`
	ShellIntegration ShellIntegration `koanf:"shell_integration"`
	Mappings         Mappings         `koanf:"mappings"`

	// Derived/internal structures
	Security  Security  `koanf:"-"`
	Patterns  Patterns  `koanf:"-"`
	Rules     []Rule    `koanf:"-"`
	Paths     Paths     `koanf:"-"`
	LinkPaths LinkPaths `koanf:"-"`
}

// Default returns the default configuration
func Default() *Config {
	// Load the actual defaults from embedded files
	cfg, err := LoadConfiguration()
	if err != nil {
		// Fallback to minimal config if loading fails
		return &Config{
			Security: Security{
				ProtectedPaths: make(map[string]bool),
			},
			Patterns: Patterns{
				SpecialFiles: SpecialFiles{
					PackConfig: ".dodot.toml",
					IgnoreFile: ".dodotignore",
				},
			},
			LinkPaths: LinkPaths{
				CoreUnixExceptions: make(map[string]bool),
			},
		}
	}
	return cfg
}

// GenerateRulesFromMapping generates rules based on current mappings
func (c *Config) GenerateRulesFromMapping() []Rule {
	var rules []Rule

	// Path handler (for directories like bin/)
	if c.Mappings.Path != "" {
		rules = append(rules, Rule{
			Pattern: c.Mappings.Path,
			Handler: "path",
		})
	}

	// Install handler
	if c.Mappings.Install != "" {
		rules = append(rules, Rule{
			Pattern: c.Mappings.Install,
			Handler: "install",
		})
	}

	// Shell handler
	for _, pattern := range c.Mappings.Shell {
		if pattern != "" {
			rules = append(rules, Rule{
				Pattern: pattern,
				Handler: "shell",
			})
		}
	}

	// Homebrew handler
	if c.Mappings.Homebrew != "" {
		rules = append(rules, Rule{
			Pattern: c.Mappings.Homebrew,
			Handler: "homebrew",
		})
	}

	// Create ignore rules for each ignore pattern
	for _, pattern := range c.Mappings.Ignore {
		if pattern != "" {
			rules = append(rules, Rule{
				Pattern: pattern,
				Handler: "ignore",
			})
		}
	}

	return rules
}

// IsProtectedPath checks if a path is protected from symlinking
func (c *Config) IsProtectedPath(path string) bool {
	if c.Security.ProtectedPaths == nil {
		return false
	}

	// Direct match
	if c.Security.ProtectedPaths[path] {
		return true
	}

	// Check if path is under a protected directory
	for protectedPath := range c.Security.ProtectedPaths {
		// If protected path doesn't end with /, check if our path starts with it plus /
		if !strings.HasSuffix(protectedPath, "/") {
			if strings.HasPrefix(path, protectedPath+"/") {
				return true
			}
		} else if strings.HasPrefix(path, protectedPath) {
			return true
		}
	}

	return false
}

// defaultRules returns the set of built-in rules
func defaultRules() []Rule {
	return []Rule{
		{
			Pattern: "bin",
			Handler: "path",
		},
		{
			Pattern: "install.sh",
			Handler: "install",
		},
		{
			Pattern: "aliases.sh",
			Handler: "shell",
		},
		{
			Pattern: "profile.sh",
			Handler: "shell",
		},
		{
			Pattern: "login.sh",
			Handler: "shell",
		},
		{
			Pattern: "Brewfile",
			Handler: "homebrew",
		},
		{
			Pattern: "*",
			Handler: "symlink",
			Options: map[string]interface{}{
				"exclude": []interface{}{
					".dodot.toml",
					".dodotignore",
				},
			},
		},
	}
}
