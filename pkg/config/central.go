package config

import (
	"os"
)

// Security holds security-related configuration
type Security struct {
	ProtectedPaths    map[string]bool
	AllowHomeSymlinks bool
	BackupExisting    bool
	EnableRollback    bool
}

// Patterns holds various ignore and exclude patterns
type Patterns struct {
	PackIgnore      []string
	CatchallExclude []string
	SpecialFiles    SpecialFiles
}

// SpecialFiles holds names of special configuration files
type SpecialFiles struct {
	PackConfig    string
	AltPackConfig string
	IgnoreFile    string
}

// Priorities holds component priority settings
type Priorities struct {
	Triggers map[string]int
	PowerUps map[string]int
	Matchers map[string]int
}

// MatcherConfig represents a matcher configuration
type MatcherConfig struct {
	Name        string
	Type        string
	Priority    int
	TriggerType string
	TriggerData map[string]interface{}
	PowerUpType string
	PowerUpData map[string]interface{}
}

// FilePermissions holds file and directory permission settings
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
type Paths struct {
	DefaultDotfilesDir string
	DodotDirName       string
	StateDir           string
	BackupsDir         string
	TemplatesDir       string
	DeployedDir        string
	ShellDir           string
	InstallDir         string
	HomebrewDir        string
	InitScriptName     string
	LogFileName        string
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
			AllowHomeSymlinks: false,
			BackupExisting:    true,
			EnableRollback:    true,
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
				"pack.dodot.toml",
				".dodotignore",
			},
			SpecialFiles: SpecialFiles{
				PackConfig:    ".dodot.toml",
				AltPackConfig: "pack.dodot.toml",
				IgnoreFile:    ".dodotignore",
			},
		},
		Priorities: Priorities{
			Triggers: map[string]int{
				"filename": 100,
				"catchall": 0,
			},
			PowerUps: map[string]int{
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
			DefaultDotfilesDir: "dotfiles",
			DodotDirName:       "dodot",
			StateDir:           "state",
			BackupsDir:         "backups",
			TemplatesDir:       "templates",
			DeployedDir:        "deployed",
			ShellDir:           "shell",
			InstallDir:         "install",
			HomebrewDir:        "homebrew",
			InitScriptName:     "dodot-init.sh",
			LogFileName:        "dodot.log",
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
			PowerUpType: "install_script",
			PowerUpData: map[string]interface{}{},
		},
		{
			Name:        "brewfile",
			Type:        "matcher",
			Priority:    90,
			TriggerType: "filename",
			TriggerData: map[string]interface{}{
				"pattern": "Brewfile",
			},
			PowerUpType: "homebrew",
			PowerUpData: map[string]interface{}{},
		},
		{
			Name:        "shell-aliases",
			Type:        "matcher",
			Priority:    80,
			TriggerType: "filename",
			TriggerData: map[string]interface{}{
				"pattern": "*aliases.sh",
			},
			PowerUpType: "shell_profile",
			PowerUpData: map[string]interface{}{
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
			PowerUpType: "shell_profile",
			PowerUpData: map[string]interface{}{
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
			PowerUpType: "path",
			PowerUpData: map[string]interface{}{},
		},
		{
			Name:        "bin-path",
			Type:        "matcher",
			Priority:    80,
			TriggerType: "directory",
			TriggerData: map[string]interface{}{
				"pattern": "bin",
			},
			PowerUpType: "shell_add_path",
			PowerUpData: map[string]interface{}{},
		},
		{
			Name:        "local-bin-dir",
			Type:        "matcher",
			Priority:    90,
			TriggerType: "directory",
			TriggerData: map[string]interface{}{
				"pattern": ".local/bin",
			},
			PowerUpType: "path",
			PowerUpData: map[string]interface{}{},
		},
		{
			Name:        "local-bin-path",
			Type:        "matcher",
			Priority:    80,
			TriggerType: "directory",
			TriggerData: map[string]interface{}{
				"pattern": ".local/bin",
			},
			PowerUpType: "shell_add_path",
			PowerUpData: map[string]interface{}{},
		},
		{
			Name:        "symlink-catchall",
			Type:        "matcher",
			Priority:    0,
			TriggerType: "catchall",
			TriggerData: map[string]interface{}{},
			PowerUpType: "symlink",
			PowerUpData: map[string]interface{}{},
		},
	}
}
