package matchers

import (
	"fmt"
	"sort"

	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/registry"
	"github.com/arthur-debert/dodot/pkg/types"
)

// defaultMatchers stores the default matchers
var defaultMatchers = make(map[string]types.Matcher)

// RegisterDefaultMatcher registers a default matcher
func RegisterDefaultMatcher(name string, matcher types.Matcher) {
	defaultMatchers[name] = matcher
}

// DefaultMatchers returns a set of common matchers for typical dotfiles
func DefaultMatchers() []types.Matcher {
	matchers := []types.Matcher{
		// Vim configuration
		{
			Name:        "vim-config",
			TriggerName: "filename",
			PowerUpName: "symlink",
			Priority:    100,
			TriggerOptions: map[string]interface{}{
				"pattern": ".vimrc",
			},
			Enabled: true,
		},
		{
			Name:        "neovim-config",
			TriggerName: "filename",
			PowerUpName: "symlink",
			Priority:    100,
			TriggerOptions: map[string]interface{}{
				"pattern": ".config/nvim",
			},
			Enabled: true,
		},

		// Shell configurations
		{
			Name:        "bash-config",
			TriggerName: "filename",
			PowerUpName: "symlink",
			Priority:    100,
			TriggerOptions: map[string]interface{}{
				"pattern": ".bashrc",
			},
			Enabled: true,
		},
		{
			Name:        "zsh-config",
			TriggerName: "filename",
			PowerUpName: "symlink",
			Priority:    100,
			TriggerOptions: map[string]interface{}{
				"pattern": ".zshrc",
			},
			Enabled: true,
		},
		{
			Name:        "fish-config",
			TriggerName: "filename",
			PowerUpName: "symlink",
			Priority:    100,
			TriggerOptions: map[string]interface{}{
				"pattern": ".config/fish",
			},
			Enabled: true,
		},

		// Git configuration
		{
			Name:        "git-config",
			TriggerName: "filename",
			PowerUpName: "symlink",
			Priority:    100,
			TriggerOptions: map[string]interface{}{
				"pattern": ".gitconfig",
			},
			Enabled: true,
		},
		{
			Name:        "git-ignore",
			TriggerName: "filename",
			PowerUpName: "symlink",
			Priority:    100,
			TriggerOptions: map[string]interface{}{
				"pattern": ".gitignore_global",
			},
			Enabled: true,
		},

		// Common development tools
		{
			Name:        "tmux-config",
			TriggerName: "filename",
			PowerUpName: "symlink",
			Priority:    100,
			TriggerOptions: map[string]interface{}{
				"pattern": ".tmux.conf",
			},
			Enabled: true,
		},
		{
			Name:        "ssh-config",
			TriggerName: "filename",
			PowerUpName: "symlink",
			Priority:    90,
			TriggerOptions: map[string]interface{}{
				"pattern": ".ssh/config",
			},
			Enabled: true,
		},

		// Shell profile and path
		{
			Name:        "shell-profile",
			TriggerName: "filename",
			PowerUpName: "shell_profile",
			Priority:    80,
			TriggerOptions: map[string]interface{}{
				"pattern": "*aliases.sh",
			},
			Enabled: true,
		},
		{
			Name:        "shell-path",
			TriggerName: "filename",
			PowerUpName: "shell_add_path",
			Priority:    80,
			TriggerOptions: map[string]interface{}{
				"pattern": "bin",
			},
			Enabled: true,
		},

		// Bin power-up matchers
		{
			Name:        "bin-dir",
			TriggerName: "directory",
			PowerUpName: "bin",
			Priority:    90,
			TriggerOptions: map[string]interface{}{
				"pattern": "bin",
			},
			Enabled: true,
		},
		{
			Name:        "bin-local",
			TriggerName: "directory",
			PowerUpName: "bin",
			Priority:    90,
			TriggerOptions: map[string]interface{}{
				"pattern": ".local/bin",
			},
			Enabled: true,
		},

		// Template power-up matcher
		{
			Name:        "template",
			TriggerName: "extension",
			PowerUpName: "template",
			Priority:    70,
			TriggerOptions: map[string]interface{}{
				"extension": ".tmpl",
			},
			Enabled: true,
		},

		// Brewfile power-up matcher
		{
			Name:        "brewfile",
			TriggerName: "filename",
			PowerUpName: "brewfile",
			Priority:    90,
			TriggerOptions: map[string]interface{}{
				"pattern": "Brewfile",
			},
			Enabled: true,
		},

		// Install script power-up matcher
		{
			Name:        "install-script",
			TriggerName: "filename",
			PowerUpName: "install_script",
			Priority:    90,
			TriggerOptions: map[string]interface{}{
				"pattern": "install.sh",
			},
			Enabled: true,
		},
	}

	// Add any dynamically registered matchers
	for _, matcher := range defaultMatchers {
		matchers = append(matchers, matcher)
	}

	return matchers
}

// CreateMatcher creates a new matcher from configuration
func CreateMatcher(config *types.MatcherConfig) (*types.Matcher, error) {
	matcher := &types.Matcher{
		Name:           config.Name,
		TriggerName:    config.Trigger,
		PowerUpName:    config.PowerUp,
		Priority:       config.Priority,
		Options:        config.Options,
		TriggerOptions: config.TriggerOptions,
		PowerUpOptions: config.PowerUpOptions,
		Enabled:        true,
	}

	// Handle enabled flag
	if config.Enabled != nil {
		matcher.Enabled = *config.Enabled
	}

	// Handle convenience fields
	if config.Pattern != "" && matcher.TriggerOptions == nil {
		matcher.TriggerOptions = make(map[string]interface{})
		matcher.TriggerOptions["pattern"] = config.Pattern
	}

	if config.Target != "" && matcher.PowerUpOptions == nil {
		matcher.PowerUpOptions = make(map[string]interface{})
		matcher.PowerUpOptions["target"] = config.Target
	}

	// Validate the matcher
	if err := ValidateMatcher(matcher); err != nil {
		return nil, fmt.Errorf("invalid matcher configuration: %w", err)
	}

	return matcher, nil
}

// ValidateMatcher checks if a matcher configuration is valid
func ValidateMatcher(matcher *types.Matcher) error {
	if matcher.TriggerName == "" {
		return fmt.Errorf("trigger name is required")
	}

	if matcher.PowerUpName == "" {
		return fmt.Errorf("power-up name is required")
	}

	// Check if trigger factory exists
	_, err := registry.GetTriggerFactory(matcher.TriggerName)
	if err != nil {
		return fmt.Errorf("unknown trigger: %s", matcher.TriggerName)
	}

	// Check if power-up factory exists
	_, err = registry.GetPowerUpFactory(matcher.PowerUpName)
	if err != nil {
		return fmt.Errorf("unknown power-up: %s", matcher.PowerUpName)
	}

	return nil
}

// SortMatchersByPriority sorts matchers by priority (highest first)
func SortMatchersByPriority(matchers []types.Matcher) {
	sort.Slice(matchers, func(i, j int) bool {
		// Higher priority comes first
		if matchers[i].Priority != matchers[j].Priority {
			return matchers[i].Priority > matchers[j].Priority
		}
		// For same priority, sort by name for stability
		return matchers[i].Name < matchers[j].Name
	})
}

// FilterEnabledMatchers returns only enabled matchers
func FilterEnabledMatchers(matchers []types.Matcher) []types.Matcher {
	logger := logging.GetLogger("matchers")
	enabled := make([]types.Matcher, 0, len(matchers))

	for _, m := range matchers {
		if m.Enabled {
			enabled = append(enabled, m)
		} else {
			logger.Debug().
				Str("name", m.Name).
				Str("trigger", m.TriggerName).
				Str("powerup", m.PowerUpName).
				Msg("skipping disabled matcher")
		}
	}

	return enabled
}

// MergeMatchers combines multiple matcher slices, with later ones taking precedence
func MergeMatchers(matcherSets ...[]types.Matcher) []types.Matcher {
	logger := logging.GetLogger("matchers")

	// Use a map to track matchers by name for deduplication
	matcherMap := make(map[string]types.Matcher)

	// Process each set in order, later sets override earlier ones
	for _, set := range matcherSets {
		for _, matcher := range set {
			key := matcher.Name
			if key == "" {
				// For unnamed matchers, use trigger+powerup as key
				key = fmt.Sprintf("%s:%s", matcher.TriggerName, matcher.PowerUpName)
			}

			if _, exists := matcherMap[key]; exists {
				logger.Debug().
					Str("name", matcher.Name).
					Str("trigger", matcher.TriggerName).
					Str("powerup", matcher.PowerUpName).
					Msg("overriding existing matcher")
			}

			matcherMap[key] = matcher
		}
	}

	// Convert map back to slice
	result := make([]types.Matcher, 0, len(matcherMap))
	for _, matcher := range matcherMap {
		result = append(result, matcher)
	}

	// Sort by priority for consistent ordering
	SortMatchersByPriority(result)

	return result
}
