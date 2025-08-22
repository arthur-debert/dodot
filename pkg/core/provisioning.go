package core

import (
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"time"

	"github.com/arthur-debert/dodot/pkg/errors"
	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/paths"
	"github.com/arthur-debert/dodot/pkg/testutil"
	"github.com/arthur-debert/dodot/pkg/types"
)

// ShouldProvisionAction checks if a provisioning action should be executed
// based on its sentinel file and checksum. Returns true if the action
// should run, false if it has already run with the same checksum.
func ShouldProvisionAction(action types.Action, force bool, pathsInstance *paths.Paths) (bool, error) {
	logger := logging.GetLogger("core.provisioning")

	// If force flag is set, always run
	if force {
		logger.Debug().
			Str("action_type", string(action.Type)).
			Str("pack", action.Pack).
			Msg("Force flag set, will run action")
		return true, nil
	}

	// Only check sentinel files for provisioning action types
	switch action.Type {
	case types.ActionTypeBrew, types.ActionTypeInstall:
		// Continue with sentinel check
	default:
		// Not a provisioning action, always run
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
	var handlerType string
	switch action.Type {
	case types.ActionTypeBrew:
		handlerType = "homebrew"
	case types.ActionTypeInstall:
		handlerType = "provision"
	}
	sentinelPath := pathsInstance.SentinelPath(handlerType, pack)

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

// FilterProvisioningActions filters a list of actions based on their provisioning status.
// It removes actions that have already been executed with the same checksum,
// unless the force flag is set.
func FilterProvisioningActions(actions []types.Action, force bool, pathsInstance *paths.Paths) ([]types.Action, error) {
	logger := logging.GetLogger("core.provisioning")
	logger.Debug().
		Int("action_count", len(actions)).
		Bool("force", force).
		Msg("Filtering provisioning actions")

	if len(actions) == 0 {
		return actions, nil
	}

	filtered := make([]types.Action, 0, len(actions))

	for _, action := range actions {
		shouldRun, err := ShouldProvisionAction(action, force, pathsInstance)
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
				Msg("Skipping provisioning action (already executed)")
		}
	}

	logger.Info().
		Int("original_count", len(actions)).
		Int("filtered_count", len(filtered)).
		Int("skipped_count", len(actions)-len(filtered)).
		Msg("Filtered provisioning actions")

	return filtered, nil
}

// CalculateActionChecksum calculates the checksum for an action's source file.
// This is used to update the checksum metadata for provisioning actions.
func CalculateActionChecksum(action types.Action) (string, error) {
	if action.Source == "" {
		return "", errors.New(errors.ErrActionInvalid, "action has no source file")
	}

	// For brew and install actions, calculate checksum of source file
	switch action.Type {
	case types.ActionTypeBrew, types.ActionTypeInstall:
		return testutil.CalculateFileChecksum(action.Source)
	default:
		return "", fmt.Errorf("checksum calculation not supported for action type: %s", action.Type)
	}
}

// ProvisioningStatus represents the execution status of a provisioning handler
type ProvisioningStatus struct {
	Executed   bool
	ExecutedAt time.Time
	Checksum   string
	Changed    bool // True if the source file has changed since execution
}

// GetProvisioningStatus checks the status of a provisioning handler for a specific pack
func GetProvisioningStatus(packPath, handlerName string, pathsInstance *paths.Paths) (*ProvisioningStatus, error) {
	logger := logging.GetLogger("core.provisioning")

	// Map handler names to their file patterns
	var filePattern string

	switch handlerName {
	case "provision":
		filePattern = "install.sh"
	case "homebrew":
		filePattern = "Brewfile"
	default:
		return nil, fmt.Errorf("unknown provisioning handler: %s", handlerName)
	}

	// Get the sentinel path using the unified API
	packName := filepath.Base(packPath)
	sentinelDir := pathsInstance.SentinelPath(handlerName, packName)

	// Check if the source file exists
	sourceFile := filepath.Join(packPath, filePattern)
	if _, err := os.Stat(sourceFile); err != nil {
		if os.IsNotExist(err) {
			// No source file, so no status to report
			return nil, nil
		}
		return nil, errors.Wrapf(err, errors.ErrFileAccess, "failed to check source file")
	}

	// Calculate current checksum
	currentChecksum, err := testutil.CalculateFileChecksum(sourceFile)
	if err != nil {
		return nil, err
	}

	// Check for sentinel file
	sentinelPath := sentinelDir // The sentinel dir itself acts as the sentinel file
	sentinelData, err := os.ReadFile(sentinelPath)
	if err != nil {
		if os.IsNotExist(err) {
			// Not executed yet
			return &ProvisioningStatus{
				Executed: false,
				Checksum: currentChecksum,
			}, nil
		}
		return nil, errors.Wrapf(err, errors.ErrFileAccess, "failed to read sentinel file")
	}

	// The sentinel file contains just the checksum
	storedChecksum := strings.TrimSpace(string(sentinelData))

	// Get file info for execution time
	fileInfo, err := os.Stat(sentinelPath)
	var executedAt time.Time
	if err == nil {
		executedAt = fileInfo.ModTime()
	}

	// Check if file has changed
	changed := storedChecksum != currentChecksum

	logger.Trace().
		Str("pack", filepath.Base(packPath)).
		Str("handler", handlerName).
		Bool("executed", true).
		Bool("changed", changed).
		Msg("Provisioning status checked")

	return &ProvisioningStatus{
		Executed:   true,
		ExecutedAt: executedAt,
		Checksum:   currentChecksum,
		Changed:    changed,
	}, nil
}
