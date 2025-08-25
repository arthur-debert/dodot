package config

import (
	"fmt"
	"os"
	"path/filepath"

	"github.com/arthur-debert/dodot/pkg/logging"
	toml "github.com/pelletier/go-toml/v2"
)

var log = logging.GetLogger("config")

// PackConfig represents configuration options for a pack from .dodot.toml
type PackConfig struct {
	Ignore   []IgnoreRule      `toml:"ignore"`
	Override []OverrideRule    `toml:"override"`
	Mappings map[string]string `toml:"mappings"`
}

// IgnoreRule defines a file or pattern to be ignored
type IgnoreRule struct {
	Path string `toml:"path"`
}

// OverrideRule defines a behavior override for a specific file or pattern
type OverrideRule struct {
	Path    string                 `toml:"path"`
	Handler string                 `toml:"handler"`
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

// LoadPackConfig reads and parses a pack's .dodot.toml configuration file
func LoadPackConfig(configPath string) (PackConfig, error) {
	logger := log.With().Str("configPath", configPath).Logger()

	data, err := os.ReadFile(configPath)
	if err != nil {
		return PackConfig{}, fmt.Errorf("failed to read config file: %w", err)
	}

	var config PackConfig
	if err := toml.Unmarshal(data, &config); err != nil {
		return PackConfig{}, fmt.Errorf("failed to parse TOML: %w", err)
	}

	logger.Debug().
		Int("ignore_rules", len(config.Ignore)).
		Int("override_rules", len(config.Override)).
		Int("mappings", len(config.Mappings)).
		Msg("Pack config loaded")

	return config, nil
}

// FileExists is a helper to check if a file exists
func FileExists(path string) bool {
	info, err := os.Stat(path)
	if err != nil {
		return false
	}
	return !info.IsDir()
}
