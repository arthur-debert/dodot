package matchers

import (
	"fmt"
	"sort"

	"github.com/arthur-debert/dodot/pkg/config"
	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/registry"
	"github.com/arthur-debert/dodot/pkg/types"

	// Import powerups and triggers to register them via init() functions
	_ "github.com/arthur-debert/dodot/pkg/powerups/homebrew"
	_ "github.com/arthur-debert/dodot/pkg/powerups/install"
	_ "github.com/arthur-debert/dodot/pkg/powerups/path"
	_ "github.com/arthur-debert/dodot/pkg/powerups/shell_add_path"
	_ "github.com/arthur-debert/dodot/pkg/powerups/shell_profile"
	_ "github.com/arthur-debert/dodot/pkg/powerups/symlink"
	_ "github.com/arthur-debert/dodot/pkg/triggers"
)

// defaultMatchers stores the default matchers
var defaultMatchers = make(map[string]types.Matcher)

// init registers all default powerups and triggers needed by the default matchers
// by importing the packages, which triggers their init() functions
func init() {
	// The import of powerups and triggers packages above will automatically
	// register all powerups and triggers through their init() functions.
	// This ensures that any code importing matchers gets all the default
	// powerups and triggers registered without needing separate imports.
}

// RegisterDefaultMatcher registers a default matcher
func RegisterDefaultMatcher(name string, matcher types.Matcher) {
	defaultMatchers[name] = matcher
}

// DefaultMatchers returns a set of common matchers for typical dotfiles
func DefaultMatchers() []types.Matcher {
	cfg := config.Default()
	matchers := make([]types.Matcher, len(cfg.Matchers))

	for i, mc := range cfg.Matchers {
		matchers[i] = types.Matcher{
			Name:           mc.Name,
			TriggerName:    mc.TriggerType,
			PowerUpName:    mc.PowerUpType,
			Priority:       mc.Priority,
			TriggerOptions: mc.TriggerData,
			PowerUpOptions: mc.PowerUpData,
			Enabled:        true,
		}
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
