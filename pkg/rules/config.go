package rules

import (
	"fmt"
	"os"
	"path/filepath"
	"strings"

	"github.com/arthur-debert/dodot/pkg/config"
	"github.com/arthur-debert/dodot/pkg/logging"
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

// MergeRules merges pack-specific rules with global rules
// Pack rules take precedence (higher priority)
func MergeRules(global, packSpecific []Rule) []Rule {
	// Bump pack rule priorities to ensure they override global rules
	const packPriorityBoost = 1000
	for i := range packSpecific {
		packSpecific[i].Priority += packPriorityBoost
	}

	// Combine and return
	return append(packSpecific, global...)
}

// getDefaultRules returns the default set of rules
func getDefaultRules() []Rule {
	return []Rule{
		// Exclusions
		{Pattern: "!*.bak", Priority: 1000},
		{Pattern: "!*.tmp", Priority: 1000},
		{Pattern: "!*.swp", Priority: 1000},
		{Pattern: "!.DS_Store", Priority: 1000},
		{Pattern: "!#*#", Priority: 1000},
		{Pattern: "!*~", Priority: 1000},

		// Provisioning
		{Pattern: "install.sh", Handler: "install", Priority: 90},
		{Pattern: "Brewfile", Handler: "homebrew", Priority: 90},

		// Shell integration
		{Pattern: "*aliases.sh", Handler: "shell", Priority: 80,
			Options: map[string]interface{}{"placement": "aliases"}},
		{Pattern: "profile.sh", Handler: "shell", Priority: 80,
			Options: map[string]interface{}{"placement": "environment"}},
		{Pattern: "login.sh", Handler: "shell", Priority: 80,
			Options: map[string]interface{}{"placement": "login"}},

		// Path directories
		{Pattern: "bin/", Handler: "path", Priority: 90},
		{Pattern: ".local/bin/", Handler: "path", Priority: 90},

		// Catchall
		{Pattern: "*", Handler: "symlink", Priority: 0},
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
			Pattern:  pattern,
			Handler:  m.Handler.Type,
			Priority: m.Priority,
			Options:  m.Handler.Data,
		}

		rules = append(rules, rule)
	}

	return rules
}
