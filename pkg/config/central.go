package config

import (
	"os"
	"strings"
)

// Security holds security-related configuration
type Security struct {
	// ProtectedPaths defines paths that should not be symlinked for security reasons
	// TODO: Implement in pkg/handlers/lib/symlink/symlink.go ProcessLinking()
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
	Path string `koanf:"path"`
	// Install specifies the filename pattern for install scripts
	Install string `koanf:"install"`
	// Shell specifies filename patterns for shell scripts
	Shell []string `koanf:"shell"`
	// Homebrew specifies the filename pattern for Homebrew files
	Homebrew string `koanf:"homebrew"`
	// Ignore specifies patterns for files to exclude from processing
	Ignore []string `koanf:"ignore"`
}

// Config is the main configuration structure
type Config struct {
	Security         Security         `koanf:"security"`
	Patterns         Patterns         `koanf:"patterns"`
	Rules            []Rule           `koanf:"rules"`
	FilePermissions  FilePermissions  `koanf:"file_permissions"`
	ShellIntegration ShellIntegration `koanf:"shell_integration"`
	Paths            Paths            `koanf:"paths"`
	LinkPaths        LinkPaths        `koanf:"link_paths"`
	Mappings         Mappings         `koanf:"mappings"`
}

// Default returns the default configuration
func Default() *Config {
	cfg := &Config{
		Security: Security{
			ProtectedPaths: map[string]bool{
				".ssh/authorized_keys": true,
				".ssh/id_rsa":          true,
				".ssh/id_ed25519":      true,
				".gnupg":               true,
				".password-store":      true,
				".config/gh/hosts.yml": true, // GitHub CLI auth
				".aws/credentials":     true,
				".kube/config":         true,
				".docker/config.json":  true,
			},
		},
		Patterns: Patterns{
			PackIgnore: []string{
				".git",
				".svn",
				".hg",
				"node_modules",
				".DS_Store",
				"*.swp",
				"*~",
				"#*#",
			},
			// CatchallExclude is now derived from SpecialFiles
			CatchallExclude: []string{},
			SpecialFiles: SpecialFiles{
				PackConfig: ".dodot.toml",
				IgnoreFile: ".dodotignore",
			},
		},
		Rules: defaultRules(),
		FilePermissions: FilePermissions{
			Directory:  0755,
			File:       0644,
			Executable: 0755,
		},
		ShellIntegration: ShellIntegration{
			BashZshSnippet:           `[ -f "$HOME/.local/share/dodot/shell/dodot-init.sh" ] && source "$HOME/.local/share/dodot/shell/dodot-init.sh"`,
			BashZshSnippetWithCustom: `[ -f "%s/shell/dodot-init.sh" ] && source "%s/shell/dodot-init.sh"`,
			FishSnippet: `if test -f "$HOME/.local/share/dodot/shell/dodot-init.fish"
    source "$HOME/.local/share/dodot/shell/dodot-init.fish"
end`,
		},
		Paths: Paths{
			// Reserved for future user-configurable paths
		},
		LinkPaths: LinkPaths{
			// These files/dirs always deploy to $HOME for security or compatibility reasons
			CoreUnixExceptions: map[string]bool{
				"ssh":       true, // .ssh/ - security critical, expects $HOME
				"gnupg":     true, // .gnupg/ - security critical, expects $HOME
				"aws":       true, // .aws/ - credentials, expects $HOME
				"kube":      true, // .kube/ - kubernetes config
				"docker":    true, // .docker/ - docker config
				"gitconfig": true, // .gitconfig - git expects in $HOME
				"bashrc":    true, // .bashrc - shell expects in $HOME
				"zshrc":     true, // .zshrc - shell expects in $HOME
				"profile":   true, // .profile - shell expects in $HOME
			},
		},
	}

	// Derive CatchallExclude from SpecialFiles to avoid redundancy
	cfg.Patterns.CatchallExclude = []string{
		cfg.Patterns.SpecialFiles.PackConfig,
		cfg.Patterns.SpecialFiles.IgnoreFile,
	}

	return cfg
}

func defaultRules() []Rule {
	return []Rule{
		// Exclusions
		{Pattern: "!*.bak"},
		{Pattern: "!*.tmp"},
		{Pattern: "!*.swp"},
		{Pattern: "!.DS_Store"},
		{Pattern: "!#*#"},
		{Pattern: "!*~"},

		// Exact matches
		{Pattern: "install.sh", Handler: "install"},
		{Pattern: "Brewfile", Handler: "homebrew"},
		{Pattern: "profile.sh", Handler: "shell",
			Options: map[string]interface{}{"placement": "environment"}},
		{Pattern: "login.sh", Handler: "shell",
			Options: map[string]interface{}{"placement": "login"}},

		// Glob patterns
		{Pattern: "*aliases.sh", Handler: "shell",
			Options: map[string]interface{}{"placement": "aliases"}},

		// Directory patterns
		{Pattern: "bin/", Handler: "path"},
		{Pattern: ".local/bin/", Handler: "path"},

		// Catchall
		{Pattern: "*", Handler: "symlink"},
	}
}

// GenerateRulesFromMapping creates rules based on mappings configuration
func (c *Config) GenerateRulesFromMapping() []Rule {
	var rules []Rule

	// Path rule for bin directories
	if c.Mappings.Path != "" {
		rules = append(rules, Rule{
			Pattern: c.Mappings.Path + "/",
			Handler: "path",
		})
	}

	// Install script rule
	if c.Mappings.Install != "" {
		rules = append(rules, Rule{
			Pattern: c.Mappings.Install,
			Handler: "install",
		})
	}

	// Shell script rules
	for _, pattern := range c.Mappings.Shell {
		placement := "environment" // Default placement
		if strings.Contains(pattern, "aliases") {
			placement = "aliases"
		} else if strings.Contains(pattern, "login") {
			placement = "login"
		}

		rules = append(rules, Rule{
			Pattern: pattern,
			Handler: "shell",
			Options: map[string]interface{}{
				"placement": placement,
			},
		})
	}

	// Homebrew rule
	if c.Mappings.Homebrew != "" {
		rules = append(rules, Rule{
			Pattern: c.Mappings.Homebrew,
			Handler: "homebrew",
		})
	}

	// Ignore rules (exclusion patterns start with !)
	for _, pattern := range c.Mappings.Ignore {
		rules = append(rules, Rule{
			Pattern: "!" + pattern,
			Handler: "exclude",
		})
	}

	return rules
}
