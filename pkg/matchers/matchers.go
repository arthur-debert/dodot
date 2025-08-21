package matchers

import (
	"fmt"
	"sort"

	"github.com/arthur-debert/dodot/pkg/config"
	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/registry"
	"github.com/arthur-debert/dodot/pkg/types"

	// Import handlers and triggers to register them via init() functions
	_ "github.com/arthur-debert/dodot/pkg/handlers/homebrew"
	_ "github.com/arthur-debert/dodot/pkg/handlers/install"
	_ "github.com/arthur-debert/dodot/pkg/handlers/path"
	_ "github.com/arthur-debert/dodot/pkg/handlers/shell_add_path"
	_ "github.com/arthur-debert/dodot/pkg/handlers/shell_profile"
	_ "github.com/arthur-debert/dodot/pkg/handlers/symlink"
	_ "github.com/arthur-debert/dodot/pkg/triggers"
)

// defaultMatchers stores the default matchers
var defaultMatchers = make(map[string]types.Matcher)

// init registers all default handlers and triggers needed by the default matchers
// by importing the packages, which triggers their init() functions
func init() {
	// The import of handlers and triggers packages above will automatically
	// register all handlers and triggers through their init() functions.
	// This ensures that any code importing matchers gets all the default
	// handlers and triggers registered without needing separate imports.
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
			HandlerName:    mc.HandlerType,
			Priority:       mc.Priority,
			TriggerOptions: mc.TriggerData,
			HandlerOptions: mc.HandlerData,
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
		HandlerName:    config.Handler,
		Priority:       config.Priority,
		Options:        config.Options,
		TriggerOptions: config.TriggerOptions,
		HandlerOptions: config.HandlerOptions,
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

	if config.Target != "" && matcher.HandlerOptions == nil {
		matcher.HandlerOptions = make(map[string]interface{})
		matcher.HandlerOptions["target"] = config.Target
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

	if matcher.HandlerName == "" {
		return fmt.Errorf("power-up name is required")
	}

	// Check if trigger factory exists
	_, err := registry.GetTriggerFactory(matcher.TriggerName)
	if err != nil {
		return fmt.Errorf("unknown trigger: %s", matcher.TriggerName)
	}

	// Check if power-up factory exists
	_, err = registry.GetHandlerFactory(matcher.HandlerName)
	if err != nil {
		return fmt.Errorf("unknown power-up: %s", matcher.HandlerName)
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
				Str("handler", m.HandlerName).
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
				// For unnamed matchers, use trigger+handler as key
				key = fmt.Sprintf("%s:%s", matcher.TriggerName, matcher.HandlerName)
			}

			if _, exists := matcherMap[key]; exists {
				logger.Debug().
					Str("name", matcher.Name).
					Str("trigger", matcher.TriggerName).
					Str("handler", matcher.HandlerName).
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
