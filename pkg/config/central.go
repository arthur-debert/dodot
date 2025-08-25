package config

import (
	"os"
)

// Security holds security-related configuration
type Security struct {
	ProtectedPaths map[string]bool
}

// Patterns holds various ignore and exclude patterns
type Patterns struct {
	PackIgnore      []string
	CatchallExclude []string
	SpecialFiles    SpecialFiles
}

// SpecialFiles holds names of special configuration files
type SpecialFiles struct {
	PackConfig string
	IgnoreFile string
}

// Priorities holds component priority settings
type Priorities struct {
	Triggers map[string]int
	Handlers map[string]int
	Matchers map[string]int
}

// MatcherConfig represents a matcher configuration
type MatcherConfig struct {
	Name        string
	Type        string
	Priority    int
	TriggerType string
	TriggerData map[string]interface{}
	HandlerType string
	HandlerData map[string]interface{}
}

// FilePermissions holds file and directory permission settings
// IMPORTANT: These permissions are intentionally NOT used throughout the codebase.
// File permissions (0755, 0644, etc.) should remain hardcoded where they are used
// as they are security-critical and context-specific. This struct exists only for
// potential future use cases where centralized permissions might be beneficial.
type FilePermissions struct {
	Directory  os.FileMode
	File       os.FileMode
	Executable os.FileMode
}

// ShellIntegration holds shell integration snippets
type ShellIntegration struct {
	BashZshSnippet           string
	BashZshSnippetWithCustom string
	FishSnippet              string
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
	CoreUnixExceptions map[string]bool
}

// LoggingConfig holds logging-related configuration
type LoggingConfig struct {
	// VerbosityLevels maps verbosity flags to log levels
	VerbosityLevels map[int]string
	// DefaultLevel is the default log level
	DefaultLevel string
	// TimeFormat is the time format for console output
	TimeFormat string
	// EnableColor enables color output in console
	EnableColor bool
	// EnableCaller enables caller information for debug and trace levels
	EnableCallerAtVerbosity int
}

// Config is the main configuration structure
type Config struct {
	Security         Security
	Patterns         Patterns
	Priorities       Priorities
	Matchers         []MatcherConfig
	FilePermissions  FilePermissions
	ShellIntegration ShellIntegration
	Paths            Paths
	LinkPaths        LinkPaths
	Logging          LoggingConfig
}

// Default returns the default configuration
func Default() *Config {
	return &Config{
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
			CatchallExclude: []string{
				".dodot.toml",
				".dodotignore",
			},
			SpecialFiles: SpecialFiles{
				PackConfig: ".dodot.toml",
				IgnoreFile: ".dodotignore",
			},
		},
		Priorities: Priorities{
			Triggers: map[string]int{
				"filename": 100,
				"catchall": 0,
			},
			Handlers: map[string]int{
				"symlink":  100,
				"path":     90,
				"template": 70,
			},
			Matchers: map[string]int{
				"install-script":   90,
				"brewfile":         90,
				"shell-aliases":    80,
				"shell-profile":    80,
				"bin-dir":          90,
				"bin-path":         80,
				"local-bin-dir":    90,
				"local-bin-path":   80,
				"template":         70,
				"symlink-catchall": 0,
			},
		},
		Matchers: defaultMatchers(),
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
		Logging: LoggingConfig{
			VerbosityLevels: map[int]string{
				0: "warn",
				1: "info",
				2: "debug",
				3: "trace",
			},
			DefaultLevel:            "warn",
			TimeFormat:              "15:04",
			EnableColor:             true,
			EnableCallerAtVerbosity: 2,
		},
	}
}

func defaultMatchers() []MatcherConfig {
	return []MatcherConfig{
		{
			Name:        "install-script",
			Type:        "matcher",
			Priority:    90,
			TriggerType: "filename",
			TriggerData: map[string]interface{}{
				"pattern": "install.sh",
			},
			HandlerType: "provision",
			HandlerData: map[string]interface{}{},
		},
		{
			Name:        "brewfile",
			Type:        "matcher",
			Priority:    90,
			TriggerType: "filename",
			TriggerData: map[string]interface{}{
				"pattern": "Brewfile",
			},
			HandlerType: "homebrew",
			HandlerData: map[string]interface{}{},
		},
		{
			Name:        "shell-aliases",
			Type:        "matcher",
			Priority:    80,
			TriggerType: "filename",
			TriggerData: map[string]interface{}{
				"pattern": "*aliases.sh",
			},
			HandlerType: "shell_profile",
			HandlerData: map[string]interface{}{
				"placement": "aliases",
			},
		},
		{
			Name:        "shell-profile",
			Type:        "matcher",
			Priority:    80,
			TriggerType: "filename",
			TriggerData: map[string]interface{}{
				"pattern": "profile.sh",
			},
			HandlerType: "shell_profile",
			HandlerData: map[string]interface{}{
				"placement": "environment",
			},
		},
		{
			Name:        "bin-dir",
			Type:        "matcher",
			Priority:    90,
			TriggerType: "directory",
			TriggerData: map[string]interface{}{
				"pattern": "bin",
			},
			HandlerType: "path",
			HandlerData: map[string]interface{}{},
		},
		{
			Name:        "bin-path",
			Type:        "matcher",
			Priority:    80,
			TriggerType: "directory",
			TriggerData: map[string]interface{}{
				"pattern": "bin",
			},
			HandlerType: "shell_add_path",
			HandlerData: map[string]interface{}{},
		},
		{
			Name:        "local-bin-dir",
			Type:        "matcher",
			Priority:    90,
			TriggerType: "directory",
			TriggerData: map[string]interface{}{
				"pattern": ".local/bin",
			},
			HandlerType: "path",
			HandlerData: map[string]interface{}{},
		},
		{
			Name:        "local-bin-path",
			Type:        "matcher",
			Priority:    80,
			TriggerType: "directory",
			TriggerData: map[string]interface{}{
				"pattern": ".local/bin",
			},
			HandlerType: "shell_add_path",
			HandlerData: map[string]interface{}{},
		},
		{
			Name:        "symlink-catchall",
			Type:        "matcher",
			Priority:    0,
			TriggerType: "catchall",
			TriggerData: map[string]interface{}{},
			HandlerType: "symlink",
			HandlerData: map[string]interface{}{},
		},
	}
}
