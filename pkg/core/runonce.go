package core

import (
	"fmt"
	"os"
	"path/filepath"

	"github.com/arthur-debert/dodot/pkg/errors"
	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/powerups"
	"github.com/arthur-debert/dodot/pkg/types"
)

// ShouldRunOnceAction checks if a run-once action should be executed
// based on its sentinel file and checksum. Returns true if the action
// should run, false if it has already run with the same checksum.
func ShouldRunOnceAction(action types.Action, force bool) (bool, error) {
	logger := logging.GetLogger("core.runonce")
	
	// If force flag is set, always run
	if force {
		logger.Debug().
			Str("action_type", string(action.Type)).
			Str("pack", action.Pack).
			Msg("Force flag set, will run action")
		return true, nil
	}
	
	// Only check sentinel files for run-once action types
	switch action.Type {
	case types.ActionTypeBrew, types.ActionTypeInstall:
		// Continue with sentinel check
	default:
		// Not a run-once action, always run
		return true, nil
	}
	
	// Get checksum from metadata
	checksum, ok := action.Metadata["checksum"].(string)
	if !ok || checksum == "" {
		logger.Warn().
			Str("action_type", string(action.Type)).
			Str("pack", action.Pack).
			Msg("Missing checksum in action metadata, will run")
		return true, nil
	}
	
	// Get pack from metadata
	pack, ok := action.Metadata["pack"].(string)
	if !ok || pack == "" {
		logger.Warn().
			Str("action_type", string(action.Type)).
			Msg("Missing pack in action metadata, will run")
		return true, nil
	}
	
	// Determine sentinel path based on action type
	var sentinelPath string
	switch action.Type {
	case types.ActionTypeBrew:
		sentinelPath = filepath.Join(types.GetBrewfileDir(), pack)
	case types.ActionTypeInstall:
		sentinelPath = filepath.Join(types.GetInstallDir(), pack)
	}
	
	// Check if sentinel file exists
	info, err := os.Stat(sentinelPath)
	if err != nil {
		if os.IsNotExist(err) {
			logger.Debug().
				Str("action_type", string(action.Type)).
				Str("pack", pack).
				Msg("Sentinel file does not exist, will run")
			return true, nil
		}
		return false, errors.Wrapf(err, errors.ErrFileAccess, 
			"failed to check sentinel file: %s", sentinelPath)
	}
	
	// Sentinel exists, check if it's a regular file
	if !info.Mode().IsRegular() {
		logger.Warn().
			Str("sentinel_path", sentinelPath).
			Msg("Sentinel path exists but is not a regular file, will run")
		return true, nil
	}
	
	// Read existing checksum
	existingChecksum, err := os.ReadFile(sentinelPath)
	if err != nil {
		return false, errors.Wrapf(err, errors.ErrFileAccess,
			"failed to read sentinel file: %s", sentinelPath)
	}
	
	// Compare checksums
	if string(existingChecksum) == checksum {
		logger.Debug().
			Str("action_type", string(action.Type)).
			Str("pack", pack).
			Str("checksum", checksum).
			Msg("Checksum matches sentinel, skipping")
		return false, nil
	}
	
	logger.Debug().
		Str("action_type", string(action.Type)).
		Str("pack", pack).
		Str("old_checksum", string(existingChecksum)).
		Str("new_checksum", checksum).
		Msg("Checksum differs from sentinel, will run")
	return true, nil
}

// FilterRunOnceActions filters a list of actions based on their run-once status.
// It removes actions that have already been executed with the same checksum,
// unless the force flag is set.
func FilterRunOnceActions(actions []types.Action, force bool) ([]types.Action, error) {
	logger := logging.GetLogger("core.runonce")
	logger.Debug().
		Int("action_count", len(actions)).
		Bool("force", force).
		Msg("Filtering run-once actions")
	
	if len(actions) == 0 {
		return actions, nil
	}
	
	filtered := make([]types.Action, 0, len(actions))
	
	for _, action := range actions {
		shouldRun, err := ShouldRunOnceAction(action, force)
		if err != nil {
			return nil, err
		}
		
		if shouldRun {
			filtered = append(filtered, action)
		} else {
			logger.Info().
				Str("action_type", string(action.Type)).
				Str("pack", action.Pack).
				Str("description", action.Description).
				Msg("Skipping run-once action (already executed)")
		}
	}
	
	logger.Info().
		Int("original_count", len(actions)).
		Int("filtered_count", len(filtered)).
		Int("skipped_count", len(actions)-len(filtered)).
		Msg("Filtered run-once actions")
	
	return filtered, nil
}

// CalculateActionChecksum calculates the checksum for an action's source file.
// This is used to update the checksum metadata for run-once actions.
func CalculateActionChecksum(action types.Action) (string, error) {
	if action.Source == "" {
		return "", errors.New(errors.ErrActionInvalid, "action has no source file")
	}
	
	// For brew and install actions, calculate checksum of source file
	switch action.Type {
	case types.ActionTypeBrew, types.ActionTypeInstall:
		return powerups.CalculateFileChecksum(action.Source)
	default:
		return "", fmt.Errorf("checksum calculation not supported for action type: %s", action.Type)
	}
}