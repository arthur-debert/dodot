package rules

import (
	"fmt"
	"os"
	"path/filepath"
	"strings"

	"github.com/arthur-debert/dodot/pkg/config"
	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/knadh/koanf/v2"
	toml "github.com/pelletier/go-toml/v2"
)

// LoadRules loads rules from configuration
func LoadRules(k *koanf.Koanf) ([]config.Rule, error) {
	logger := logging.GetLogger("rules.config")

	// First, try to load new-style rules
	var rules []config.Rule
	if err := k.Unmarshal("rules", &rules); err == nil && len(rules) > 0 {
		logger.Info().Msg("Loaded new-style rules from configuration")
		return validateRules(rules)
	}

	// Use defaults if no configuration found
	logger.Info().Msg("No rules configured, using defaults")
	return GetDefaultRules(), nil
}

// validateRules checks that rules are valid
func validateRules(rules []config.Rule) ([]config.Rule, error) {
	for i, rule := range rules {
		if rule.Pattern == "" {
			return nil, fmt.Errorf("rule %d has empty pattern", i)
		}
		if rule.Handler == "" && !strings.HasPrefix(rule.Pattern, "!") {
			return nil, fmt.Errorf("rule %d has empty handler", i)
		}
	}
	return rules, nil
}

// LoadPackRules loads pack-specific rules from a pack's .dodot.toml
func LoadPackRules(packPath string) ([]config.Rule, error) {
	logger := logging.GetLogger("rules.config")

	configPath := filepath.Join(packPath, ".dodot.toml")
	if _, err := os.Stat(configPath); os.IsNotExist(err) {
		// No pack config is fine, just return empty rules
		return nil, nil
	}

	// Load the pack configuration
	packConfig, err := config.LoadPackConfig(configPath)
	if err != nil {
		return nil, fmt.Errorf("failed to load pack config: %w", err)
	}

	// Generate rules from the pack's mappings
	baseConfig := config.Config{Mappings: packConfig.Mappings}
	mappingRules := baseConfig.GenerateRulesFromMapping()

	// Generate ignore rules from pack's [pack] ignore patterns
	ignoreRules := packConfig.GenerateIgnoreRules()

	// Combine rules - ignore rules should come first to exclude files early
	rules := append(ignoreRules, mappingRules...)

	logger.Debug().
		Str("pack", packPath).
		Int("mappingRules", len(mappingRules)).
		Int("ignoreRules", len(ignoreRules)).
		Int("totalRules", len(rules)).
		Msg("Loaded pack-specific rules")

	return rules, nil
}

// LoadPackRulesFS loads pack-specific rules from a pack's .dodot.toml using the provided filesystem
func LoadPackRulesFS(packPath string, fs types.FS) ([]config.Rule, error) {
	logger := logging.GetLogger("rules.config")

	configPath := filepath.Join(packPath, ".dodot.toml")
	if _, err := fs.Stat(configPath); err != nil {
		// No pack config is fine, just return empty rules
		return nil, nil
	}

	// Read config using filesystem
	data, err := fs.ReadFile(configPath)
	if err != nil {
		return nil, fmt.Errorf("failed to read pack config: %w", err)
	}

	// Parse the config
	var packConfig config.PackConfig
	if err := toml.Unmarshal(data, &packConfig); err != nil {
		return nil, fmt.Errorf("failed to parse pack config: %w", err)
	}

	// Generate rules from the pack's mappings
	baseConfig := config.Config{Mappings: packConfig.Mappings}
	mappingRules := baseConfig.GenerateRulesFromMapping()

	// Generate ignore rules from pack's [pack] ignore patterns
	ignoreRules := packConfig.GenerateIgnoreRules()

	// Combine rules - ignore rules should come first to exclude files early
	rules := append(ignoreRules, mappingRules...)

	logger.Debug().
		Str("pack", packPath).
		Int("mappingRules", len(mappingRules)).
		Int("ignoreRules", len(ignoreRules)).
		Int("totalRules", len(rules)).
		Msg("Loaded pack-specific rules")

	return rules, nil
}

// MergeRules merges pack-specific rules with global rules
// Pack rules are placed first to take precedence
func MergeRules(global, packSpecific []config.Rule) []config.Rule {
	// Pack rules come first, so they match before global rules
	return append(packSpecific, global...)
}

// GetDefaultRules returns the default set of rules
// Order matters: exclusions, exact matches, globs, directories, catchall
func GetDefaultRules() []config.Rule {
	return []config.Rule{
		// Exclusions (processed first)
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

		// Catchall (last)
		{Pattern: "*", Handler: "symlink"},
	}
}
