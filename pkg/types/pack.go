package types

import "path/filepath"

// Pack represents a directory containing dotfiles and configuration
type Pack struct {
	// Name is the pack name (usually the directory name)
	Name string

	// Path is the absolute path to the pack directory
	Path string

	// Config contains pack-specific configuration from .dodot.toml
	Config PackConfig

	// Metadata contains any additional pack information
	Metadata map[string]interface{}
}

// PackConfig represents configuration options for a pack
type PackConfig struct {
	// Files maps file patterns to actions:
	// - "ignore": skip the file entirely
	// - "<powerup-name>": use this power-up instead of default
	Files map[string]string `toml:"files"`
}

// GetFileAction returns the action for a file (empty string means use defaults)
func (c PackConfig) GetFileAction(filename string) string {
	if c.Files == nil {
		return ""
	}

	// Check exact match first
	if action, exists := c.Files[filename]; exists {
		return action
	}

	// Check glob patterns
	for pattern, action := range c.Files {
		if matched, _ := filepath.Match(pattern, filename); matched {
			return action
		}
	}

	return ""
}
