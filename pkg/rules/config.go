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
)

// LoadRules loads rules from configuration
func LoadRules(k *koanf.Koanf) ([]Rule, error) {
	logger := logging.GetLogger("rules.config")

	// First, try to load new-style rules
	var rules []Rule
	if err := k.Unmarshal("rules", &rules); err == nil && len(rules) > 0 {
		logger.Info().Msg("Loaded new-style rules from configuration")
		return validateRules(rules)
	}

	// Fall back to converting existing matchers
	var matchers []config.MatcherConfig
	if err := k.Unmarshal("matchers", &matchers); err == nil && len(matchers) > 0 {
		logger.Info().Msg("Converting matchers to rules")
		rules = adaptConfigMatchersToRules(matchers)
		return validateRules(rules)
	}

	// Use defaults if no configuration found
	logger.Info().Msg("No rules configured, using defaults")
	return getDefaultRules(), nil
}

// validateRules checks that rules are valid
func validateRules(rules []Rule) ([]Rule, error) {
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
func LoadPackRules(packPath string) ([]Rule, error) {
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
func LoadPackRulesFS(packPath string, fs types.FS) ([]Rule, error) {
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
func MergeRules(global, packSpecific []Rule) []Rule {
	// Pack rules come first, so they match before global rules
	return append(packSpecific, global...)
}

// getDefaultRules returns the default set of rules
// Order matters: exclusions, exact matches, globs, directories, catchall
func getDefaultRules() []Rule {
	return []Rule{
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

// adaptConfigMatchersToRules converts old matcher format from config to new rule format
// This is temporary during the migration period
func adaptConfigMatchersToRules(matchers []config.MatcherConfig) []Rule {
	var rules []Rule

	for _, m := range matchers {
		// Extract pattern from trigger data
		pattern := ""
		if data, ok := m.Trigger.Data["pattern"].(string); ok {
			pattern = data
		}

		// Handle special trigger types
		if m.Trigger.Type == "catchall" {
			pattern = "*"
		}

		// Skip if no pattern found
		if pattern == "" {
			continue
		}

		// Adapt directory triggers
		if m.Trigger.Type == "directory" {
			pattern = pattern + "/"
		}

		// Create rule from matcher
		rule := Rule{
			Pattern: pattern,
			Handler: m.Handler.Type,
			Options: m.Handler.Data,
		}

		rules = append(rules, rule)
	}

	return rules
}
