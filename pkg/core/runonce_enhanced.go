package core

import (
	"os"

	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/paths"
	"github.com/arthur-debert/dodot/pkg/testutil"
	"github.com/arthur-debert/dodot/pkg/types"
)

// EnrichRunOnceActionsWithChecksums adds checksums to the metadata of run-once actions.
// This is needed so the executor can write checksums to sentinel files and
// FilterRunOnceActions can compare them on subsequent runs.
func EnrichRunOnceActionsWithChecksums(actions []types.Action) []types.Action {
	logger := logging.GetLogger("core.runonce")

	for i := range actions {
		action := &actions[i]

		// Only process run-once action types
		if action.Type != types.ActionTypeBrew && action.Type != types.ActionTypeInstall {
			continue
		}

		// Skip if checksum already exists (shouldn't happen with new approach)
		if _, hasChecksum := action.Metadata["checksum"]; hasChecksum {
			continue
		}

		// Calculate checksum from source file
		if action.Source != "" {
			checksum, err := testutil.CalculateFileChecksum(action.Source)
			if err != nil {
				logger.Warn().
					Err(err).
					Str("source", action.Source).
					Str("action", string(action.Type)).
					Msg("Failed to calculate checksum for run-once action")
				continue
			}

			// Ensure metadata map exists
			if action.Metadata == nil {
				action.Metadata = make(map[string]interface{})
			}

			// Add checksum to metadata
			action.Metadata["checksum"] = checksum

			logger.Debug().
				Str("source", action.Source).
				Str("checksum", checksum).
				Str("action", string(action.Type)).
				Msg("Added checksum to run-once action")
		}
	}

	return actions
}

// FilterRunOnceTriggersEarly filters out trigger matches for run-once handlers that have
// already been executed. This is the new approach that checks sentinel files before
// handler processing, avoiding unnecessary work.
func FilterRunOnceTriggersEarly(triggers []types.TriggerMatch, force bool, pathsInstance *paths.Paths) []types.TriggerMatch {
	if force {
		// With force flag, include all triggers
		return triggers
	}

	logger := logging.GetLogger("core.runonce")
	filtered := make([]types.TriggerMatch, 0, len(triggers))

	for _, trigger := range triggers {
		// Check if this trigger is for a run-once handler
		if !isRunOnceTrigger(trigger) {
			// Not a run-once trigger, include it
			filtered = append(filtered, trigger)
			continue
		}

		// Calculate checksum of the source file
		checksum, err := testutil.CalculateFileChecksum(trigger.AbsolutePath)
		if err != nil {
			logger.Warn().
				Err(err).
				Str("path", trigger.AbsolutePath).
				Msg("Failed to calculate checksum, including trigger")
			filtered = append(filtered, trigger)
			continue
		}

		// Determine handler type from trigger
		powerUpType := getHandlerTypeFromTrigger(trigger)
		if powerUpType == "" {
			// Unknown handler type, include it
			filtered = append(filtered, trigger)
			continue
		}

		// Check sentinel file
		sentinelPath := pathsInstance.SentinelPath(powerUpType, trigger.Pack)
		sentinelChecksum, err := readSentinelChecksum(sentinelPath)
		if err != nil {
			// No sentinel or error reading, include trigger
			logger.Debug().
				Str("path", trigger.Path).
				Str("pack", trigger.Pack).
				Msg("No sentinel file found, including trigger")
			filtered = append(filtered, trigger)
			continue
		}

		// Compare checksums
		if sentinelChecksum != checksum {
			logger.Info().
				Str("path", trigger.Path).
				Str("pack", trigger.Pack).
				Str("old_checksum", sentinelChecksum).
				Str("new_checksum", checksum).
				Msg("Checksum changed, including trigger")
			filtered = append(filtered, trigger)
		} else {
			logger.Info().
				Str("path", trigger.Path).
				Str("pack", trigger.Pack).
				Msg("Skipping already-executed run-once trigger")
		}
	}

	return filtered
}

// isRunOnceTrigger checks if a trigger match is for a run-once handler
func isRunOnceTrigger(trigger types.TriggerMatch) bool {
	// Check based on matcher name or file patterns
	switch trigger.HandlerName {
	case "brewfile", "homebrew":
		return true
	case "install_script", "install":
		return true
	}

	// Also check by filename patterns
	if trigger.Path == "Brewfile" || trigger.Path == "install.sh" {
		return true
	}

	return false
}

// getHandlerTypeFromTrigger determines the handler type from a trigger match
func getHandlerTypeFromTrigger(trigger types.TriggerMatch) string {
	switch trigger.HandlerName {
	case "brewfile", "homebrew":
		return "homebrew"
	case "install_script", "install":
		return "install"
	}

	// Fallback to checking filenames
	if trigger.Path == "Brewfile" {
		return "homebrew"
	}
	if trigger.Path == "install.sh" {
		return "install"
	}

	return ""
}

// readSentinelChecksum reads the checksum from a sentinel file
func readSentinelChecksum(sentinelPath string) (string, error) {
	data, err := os.ReadFile(sentinelPath)
	if err != nil {
		return "", err
	}
	return string(data), nil
}
