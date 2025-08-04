package core

import (
	"fmt"
	"sort"
	"strings"

	"github.com/arthur-debert/dodot/pkg/errors"
	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/registry"
	"github.com/arthur-debert/dodot/pkg/types"
)

// GetActions converts trigger matches into concrete actions to perform
func GetActions(matches []types.TriggerMatch) ([]types.Action, error) {
	logger := logging.GetLogger("core.actions")
	logger.Debug().Int("matchCount", len(matches)).Msg("Converting matches to actions")

	if len(matches) == 0 {
		return nil, nil
	}

	// Group matches by power-up, pack, and options
	// This allows power-ups to process related files together
	groups := groupMatches(matches)
	logger.Debug().
		Int("group_count", len(groups)).
		Msg("Grouped matches by power-up and pack")

	var allActions []types.Action

	// Process each group
	for groupKey, groupMatches := range groups {
		logger.Debug().
			Str("group", groupKey).
			Int("matchCount", len(groupMatches)).
			Msg("Processing match group")

		actions, err := ProcessMatchGroup(groupMatches)
		if err != nil {
			logger.Error().
				Err(err).
				Str("group", groupKey).
				Msg("Failed to process match group")
			return nil, err
		}

		allActions = append(allActions, actions...)
	}

	logger.Info().Int("actionCount", len(allActions)).Msg("Generated actions")
	return allActions, nil
}

// ProcessMatchGroup processes a group of related matches with the same power-up
func ProcessMatchGroup(matches []types.TriggerMatch) ([]types.Action, error) {
	if len(matches) == 0 {
		return nil, nil
	}

	// All matches in a group have the same power-up
	powerUpName := matches[0].PowerUpName

	logger := logging.GetLogger("core.actions").With().
		Str("powerup", powerUpName).
		Int("matchCount", len(matches)).
		Logger()

	logger.Debug().Msg("Processing match group")

	// Get the power-up factory from registry
	powerUpFactory, err := registry.GetPowerUpFactory(powerUpName)
	if err != nil {
		logger.Error().Err(err).Str("powerUpName", powerUpName).Msg("Failed to get power-up factory")
		return nil, errors.Wrapf(err, errors.ErrPowerUpNotFound,
			"failed to get power-up factory for %s", powerUpName)
	}

	// Get common options from the first match (all matches in group have same options)
	options := matches[0].PowerUpOptions
	if options == nil {
		options = make(map[string]interface{})
	}

	// Create power-up instance with options
	powerUp, err := powerUpFactory(options)
	if err != nil {
		return nil, errors.Wrapf(err, errors.ErrPowerUpInvalid,
			"failed to create power-up instance for %s", powerUpName)
	}

	// Validate options
	if err := powerUp.ValidateOptions(options); err != nil {
		return nil, errors.Wrapf(err, errors.ErrPowerUpInvalid,
			"invalid options for power-up %s", powerUpName)
	}

	// Process the matches
	actions, err := powerUp.Process(matches)
	if err != nil {
		return nil, errors.Wrapf(err, errors.ErrPowerUpExecute,
			"power-up %s failed to process matches", powerUpName)
	}

	logger.Debug().
		Int("actionCount", len(actions)).
		Msg("Power-up generated actions")

	return actions, nil
}

// groupMatches groups trigger matches by power-up, pack, and options
// This ensures that related files are processed together
func groupMatches(matches []types.TriggerMatch) map[string][]types.TriggerMatch {
	groups := make(map[string][]types.TriggerMatch)

	for _, match := range matches {
		// Create a key that uniquely identifies the group
		key := createGroupKey(match)
		groups[key] = append(groups[key], match)
	}

	// Sort matches within each group by priority and path for consistent processing
	for _, group := range groups {
		sort.Slice(group, func(i, j int) bool {
			// Higher priority first
			if group[i].Priority != group[j].Priority {
				return group[i].Priority > group[j].Priority
			}
			// Then by path for stability
			return group[i].Path < group[j].Path
		})
	}

	return groups
}

// createGroupKey creates a unique key for grouping matches
func createGroupKey(match types.TriggerMatch) string {
	// Group by power-up, pack, and options
	// This ensures files with the same processing requirements are grouped together
	parts := []string{
		match.PowerUpName,
		match.Pack,
		hashOptions(match.PowerUpOptions),
	}
	return strings.Join(parts, ":")
}

// hashOptions creates a simple hash of options for grouping
// This is a simple implementation - could be improved if needed
func hashOptions(options map[string]interface{}) string {
	if len(options) == 0 {
		return "default"
	}

	// Sort keys for consistent hashing
	keys := make([]string, 0, len(options))
	for k := range options {
		keys = append(keys, k)
	}
	sort.Strings(keys)

	// Create a simple string representation
	var parts []string
	for _, k := range keys {
		parts = append(parts, fmt.Sprintf("%s=%v", k, options[k]))
	}

	return strings.Join(parts, ";")
}
