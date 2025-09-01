package handlerpipeline

import (
	"fmt"
	"os"
	"path/filepath"
	"strings"

	"github.com/arthur-debert/dodot/pkg/config"
	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/knadh/koanf/v2"
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
	return getDefaultRules(), nil
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

	// For now, we'll just return empty rules until we migrate pack configs
	// In the future, we'll load rules directly from the pack config
	logger.Debug().
		Str("pack", packPath).
		Msg("Pack config exists but rules loading not yet implemented")

	return nil, nil
}

// LoadPackRulesFS loads pack-specific rules from a pack's .dodot.toml using the provided filesystem
func LoadPackRulesFS(packPath string, fs types.FS) ([]config.Rule, error) {
	logger := logging.GetLogger("rules.config")

	configPath := filepath.Join(packPath, ".dodot.toml")
	if _, err := fs.Stat(configPath); err != nil {
		// No pack config is fine, just return empty rules
		return nil, nil
	}

	// For now, we'll just return empty rules until we migrate pack configs
	// In the future, we'll load rules directly from the pack config
	logger.Debug().
		Str("pack", packPath).
		Msg("Pack config exists but rules loading not yet implemented")

	return nil, nil
}

// MergeRules merges pack-specific rules with global rules
// Pack rules are placed first to take precedence
func MergeRules(global, packSpecific []config.Rule) []config.Rule {
	// Pack rules come first, so they match before global rules
	return append(packSpecific, global...)
}

// getDefaultRules returns the default set of rules
// Order matters: exclusions, exact matches, globs, directories, catchall
func getDefaultRules() []config.Rule {
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
