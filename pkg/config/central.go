package config

import (
	"os"
)

// Security holds security-related configuration
type Security struct {
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

// Priorities holds component priority settings
type Priorities struct {
	Triggers map[string]int `koanf:"triggers"`
	Handlers map[string]int `koanf:"handlers"`
	Matchers map[string]int `koanf:"matchers"`
}

// TriggerConfig represents trigger configuration within a matcher
type TriggerConfig struct {
	Type string                 `yaml:"type" json:"type"`
	Data map[string]interface{} `yaml:"data" json:"data"`
}

// HandlerConfig represents handler configuration within a matcher
type HandlerConfig struct {
	Type string                 `yaml:"type" json:"type"`
	Data map[string]interface{} `yaml:"data" json:"data"`
}

// MatcherConfig represents a matcher configuration
type MatcherConfig struct {
	Name     string        `yaml:"name" json:"name"`
	Priority int           `yaml:"priority" json:"priority"`
	Trigger  TriggerConfig `yaml:"trigger" json:"trigger"`
	Handler  HandlerConfig `yaml:"handler" json:"handler"`
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

// LoggingConfig holds logging-related configuration
type LoggingConfig struct {
	// DefaultLevel is the default log level
	DefaultLevel string `koanf:"default_level"`
	// TimeFormat is the time format for console output
	TimeFormat string `koanf:"time_format"`
	// EnableColor enables color output in console
	EnableColor bool `koanf:"enable_color"`
	// EnableCaller enables caller information for debug and trace levels
	EnableCallerAtVerbosity int `koanf:"enable_caller_at_verbosity"`
}

// Config is the main configuration structure
type Config struct {
	Security         Security         `koanf:"security"`
	Patterns         Patterns         `koanf:"patterns"`
	Priorities       Priorities       `koanf:"priorities"`
	Matchers         []MatcherConfig  `koanf:"matchers"`
	FilePermissions  FilePermissions  `koanf:"file_permissions"`
	ShellIntegration ShellIntegration `koanf:"shell_integration"`
	Paths            Paths            `koanf:"paths"`
	LinkPaths        LinkPaths        `koanf:"link_paths"`
	Logging          LoggingConfig    `koanf:"logging"`
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
			DefaultLevel:            "warn",
			TimeFormat:              "15:04",
			EnableColor:             true,
			EnableCallerAtVerbosity: 2,
		},
	}

	// Derive CatchallExclude from SpecialFiles to avoid redundancy
	cfg.Patterns.CatchallExclude = []string{
		cfg.Patterns.SpecialFiles.PackConfig,
		cfg.Patterns.SpecialFiles.IgnoreFile,
	}

	return cfg
}

func defaultMatchers() []MatcherConfig {
	return []MatcherConfig{
		{
			Name:     "install-script",
			Priority: 90,
			Trigger: TriggerConfig{
				Type: "filename",
				Data: map[string]interface{}{
					"pattern": "install.sh",
				},
			},
			Handler: HandlerConfig{
				Type: "install",
				Data: map[string]interface{}{},
			},
		},
		{
			Name:     "brewfile",
			Priority: 90,
			Trigger: TriggerConfig{
				Type: "filename",
				Data: map[string]interface{}{
					"pattern": "Brewfile",
				},
			},
			Handler: HandlerConfig{
				Type: "homebrew",
				Data: map[string]interface{}{},
			},
		},
		{
			Name:     "shell-aliases",
			Priority: 80,
			Trigger: TriggerConfig{
				Type: "filename",
				Data: map[string]interface{}{
					"pattern": "*aliases.sh",
				},
			},
			Handler: HandlerConfig{
				Type: "shell",
				Data: map[string]interface{}{
					"placement": "aliases",
				},
			},
		},
		{
			Name:     "shell-profile",
			Priority: 80,
			Trigger: TriggerConfig{
				Type: "filename",
				Data: map[string]interface{}{
					"pattern": "profile.sh",
				},
			},
			Handler: HandlerConfig{
				Type: "shell",
				Data: map[string]interface{}{
					"placement": "environment",
				},
			},
		},
		{
			Name:     "bin-dir",
			Priority: 90,
			Trigger: TriggerConfig{
				Type: "directory",
				Data: map[string]interface{}{
					"pattern": "bin",
				},
			},
			Handler: HandlerConfig{
				Type: "path",
				Data: map[string]interface{}{},
			},
		},
		{
			Name:     "bin-path",
			Priority: 80,
			Trigger: TriggerConfig{
				Type: "directory",
				Data: map[string]interface{}{
					"pattern": "bin",
				},
			},
			Handler: HandlerConfig{
				Type: "shell_add_path",
				Data: map[string]interface{}{},
			},
		},
		{
			Name:     "local-bin-dir",
			Priority: 90,
			Trigger: TriggerConfig{
				Type: "directory",
				Data: map[string]interface{}{
					"pattern": ".local/bin",
				},
			},
			Handler: HandlerConfig{
				Type: "path",
				Data: map[string]interface{}{},
			},
		},
		{
			Name:     "local-bin-path",
			Priority: 80,
			Trigger: TriggerConfig{
				Type: "directory",
				Data: map[string]interface{}{
					"pattern": ".local/bin",
				},
			},
			Handler: HandlerConfig{
				Type: "shell_add_path",
				Data: map[string]interface{}{},
			},
		},
		{
			Name:     "symlink-catchall",
			Priority: 0,
			Trigger: TriggerConfig{
				Type: "catchall",
				Data: map[string]interface{}{},
			},
			Handler: HandlerConfig{
				Type: "symlink",
				Data: map[string]interface{}{},
			},
		},
	}
}
