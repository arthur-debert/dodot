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

// PackConfig represents configuration options for a pack from .dodot.toml
type PackConfig struct {
	Ignore   []IgnoreRule   `toml:"ignore"`
	Override []OverrideRule `toml:"override"`
}

// IgnoreRule defines a file or pattern to be ignored
type IgnoreRule struct {
	Path string `toml:"path"`
}

// OverrideRule defines a behavior override for a specific file or pattern
type OverrideRule struct {
	Path    string                 `toml:"path"`
	Powerup string                 `toml:"powerup"`
	With    map[string]interface{} `toml:"with"`
}

// IsIgnored checks if a given file path should be ignored based on the pack's configuration.
// It matches the filename against the list of ignore rules.
func (c *PackConfig) IsIgnored(filename string) bool {
	for _, rule := range c.Ignore {
		if matched, _ := filepath.Match(rule.Path, filename); matched {
			return true
		}
	}
	return false
}

// FindOverride returns the override rule that matches the given filename, if any.
// It prioritizes exact matches over pattern matches.
func (c *PackConfig) FindOverride(filename string) *OverrideRule {
	var bestMatch *OverrideRule
	longestMatch := 0

	for i, rule := range c.Override {
		// Exact match is always preferred
		if rule.Path == filename {
			return &c.Override[i]
		}

		// Glob matching for patterns
		if matched, _ := filepath.Match(rule.Path, filename); matched {
			// Basic glob specificity: longer pattern is better
			if len(rule.Path) > longestMatch {
				bestMatch = &c.Override[i]
				longestMatch = len(rule.Path)
			}
		}
	}

	return bestMatch
}
