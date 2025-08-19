package core

import (
	"fmt"
	"path/filepath"
	"sort"
	"strings"

	"github.com/arthur-debert/dodot/pkg/errors"
	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/paths"
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

	// First, check for cross-pack symlink conflicts
	if err := checkCrossPackSymlinkConflicts(matches); err != nil {
		return nil, err
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

// checkCrossPackSymlinkConflicts checks for symlink conflicts across different packs
func checkCrossPackSymlinkConflicts(matches []types.TriggerMatch) error {
	logger := logging.GetLogger("core.actions")

	// Initialize paths instance for mapping
	pathsInstance, err := paths.New("")
	if err != nil {
		return fmt.Errorf("failed to initialize paths: %w", err)
	}

	// Filter only symlink matches
	symlinkMatches := make([]types.TriggerMatch, 0)
	for _, match := range matches {
		if match.PowerUpName == "symlink" {
			symlinkMatches = append(symlinkMatches, match)
		}
	}

	logger.Debug().
		Int("totalMatches", len(matches)).
		Int("symlinkMatches", len(symlinkMatches)).
		Msg("Checking for cross-pack symlink conflicts")

	if len(symlinkMatches) < 2 {
		// No possibility of conflict with less than 2 symlinks
		return nil
	}

	// Track target paths and their sources
	targetMap := make(map[string][]types.TriggerMatch)

	// Build map of target paths to source matches
	for _, match := range symlinkMatches {
		// Use centralized path mapping to get the actual target path
		// Note: We approximate the pack path from the absolute path by taking the parent directory.
		// This works because match.AbsolutePath is the full path to the file within the pack,
		// so its parent is the pack directory. This approximation is acceptable for Release A
		// since we're only preserving current behavior. Future releases may need more precise
		// pack resolution when implementing advanced mapping features.
		pack := &types.Pack{
			Name: match.Pack,
			Path: filepath.Dir(match.AbsolutePath),
		}
		targetPath := pathsInstance.MapPackFileToSystem(pack, match.Path)
		targetMap[targetPath] = append(targetMap[targetPath], match)

		logger.Debug().
			Str("pack", match.Pack).
			Str("path", match.Path).
			Str("targetPath", targetPath).
			Msg("Symlink match mapped")
	}

	// Check for conflicts
	for targetPath, sources := range targetMap {
		logger.Debug().
			Str("targetPath", targetPath).
			Int("sourceCount", len(sources)).
			Msg("Checking target path for conflicts")

		if len(sources) > 1 {
			// We have a conflict - multiple sources want the same target
			packList := make([]string, 0, len(sources))
			sourceList := make([]string, 0, len(sources))

			for _, source := range sources {
				packList = append(packList, source.Pack)
				sourceList = append(sourceList, fmt.Sprintf("%s/%s", source.Pack, source.Path))
			}

			logger.Error().
				Str("target", targetPath).
				Strs("packs", packList).
				Strs("sources", sourceList).
				Msg("Cross-pack symlink conflict detected")

			return fmt.Errorf("symlink conflict detected: multiple packs want to create symlink '%s'\n"+
				"Conflicting files:\n  - %s\n\n"+
				"You need to rename or remove one of these files to resolve the conflict",
				targetPath, strings.Join(sourceList, "\n  - "))
		}
	}

	return nil
}
