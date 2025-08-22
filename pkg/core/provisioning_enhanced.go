package core

import (
	"os"

	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/paths"
	"github.com/arthur-debert/dodot/pkg/testutil"
	"github.com/arthur-debert/dodot/pkg/types"
)

// EnrichProvisioningActionsWithChecksums adds checksums to the metadata of provisioning actions.
// This is needed so the executor can write checksums to sentinel files and
// FilterProvisioningActions can compare them on subsequent runs.
func EnrichProvisioningActionsWithChecksums(actions []types.Action) []types.Action {
	logger := logging.GetLogger("core.provisioning")

	for i := range actions {
		action := &actions[i]

		// Only process provisioning action types
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
					Msg("Failed to calculate checksum for provisioning action")
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
				Msg("Added checksum to provisioning action")
		}
	}

	return actions
}

// FilterProvisioningTriggersEarly filters out trigger matches for provisioning handlers that have
// already been executed. This is the new approach that checks sentinel files before
// handler processing, avoiding unnecessary work.
func FilterProvisioningTriggersEarly(triggers []types.TriggerMatch, force bool, pathsInstance *paths.Paths) []types.TriggerMatch {
	if force {
		// With force flag, include all triggers
		return triggers
	}

	logger := logging.GetLogger("core.provisioning")
	filtered := make([]types.TriggerMatch, 0, len(triggers))

	for _, trigger := range triggers {
		// Check if this trigger is for a provisioning handler
		if !isProvisioningTrigger(trigger) {
			// Not a provisioning trigger, include it
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
		handlerType := getHandlerTypeFromTrigger(trigger)
		if handlerType == "" {
			// Unknown handler type, include it
			filtered = append(filtered, trigger)
			continue
		}

		// Check sentinel file
		sentinelPath := pathsInstance.SentinelPath(handlerType, trigger.Pack)
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

// isProvisioningTrigger checks if a trigger match is for a provisioning handler
func isProvisioningTrigger(trigger types.TriggerMatch) bool {
	// Check based on matcher name or file patterns
	switch trigger.HandlerName {
	case "brewfile", "homebrew":
		return true
	case "install_script", "provision":
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
	case "install_script", "provision":
		return "provision"
	}

	// Fallback to checking filenames
	if trigger.Path == "Brewfile" {
		return "homebrew"
	}
	if trigger.Path == "install.sh" {
		return "provision"
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
