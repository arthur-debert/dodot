package core

import (
	"path/filepath"
	"time"

	"github.com/arthur-debert/dodot/pkg/errors"
	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/packs"
	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/rs/zerolog"
)

// handleUnknownEnum logs a warning for unhandled enum values and returns the fallback
func handleUnknownEnum[T ~string](logger zerolog.Logger, enumType string, value T, context string, fallback string) string {
	logger.Warn().
		Str(enumType, string(value)).
		Str("context", context).
		Msgf("Unhandled %s in %s", enumType, context)
	return fallback
}

// GetPackStatus generates display status for a single pack by checking all its actions
func GetPackStatus(pack types.Pack, actions []types.Action, fs types.FS, paths types.Pather) (*types.DisplayPack, error) {
	logger := logging.GetLogger("core.pack_status").With().
		Str("pack", pack.Name).
		Logger()

	logger.Debug().Int("actionCount", len(actions)).Msg("Getting pack status")

	displayPack := &types.DisplayPack{
		Name:      pack.Name,
		Files:     []types.DisplayFile{},
		HasConfig: false,
		IsIgnored: false,
	}

	// Check for special files first
	if err := checkSpecialFiles(pack, displayPack, fs); err != nil {
		return nil, err
	}

	// If pack is ignored, no need to process actions
	// Ignored packs should not have any actions, but we return early to be explicit
	// and avoid unnecessary processing
	if displayPack.IsIgnored {
		displayPack.Status = "ignored"
		return displayPack, nil
	}

	// Process each action
	for _, action := range actions {
		displayFile, err := getActionDisplayStatus(action, fs, paths)
		if err != nil {
			logger.Error().
				Err(err).
				Str("action", action.Description).
				Msg("Failed to get action status")
			return nil, err
		}

		displayPack.Files = append(displayPack.Files, *displayFile)
	}

	// Calculate aggregated pack status
	displayPack.Status = displayPack.GetPackStatus()

	logger.Debug().
		Str("status", displayPack.Status).
		Int("fileCount", len(displayPack.Files)).
		Msg("Pack status determined")

	return displayPack, nil
}

// checkSpecialFiles checks for .dodot.toml and .dodotignore files
func checkSpecialFiles(pack types.Pack, displayPack *types.DisplayPack, fs types.FS) error {
	logger := logging.GetLogger("core.pack_status").With().
		Str("pack", pack.Name).
		Logger()

	// Check for .dodotignore
	ignorePath := filepath.Join(pack.Path, ".dodotignore")
	if _, err := fs.Stat(ignorePath); err == nil {
		logger.Debug().Msg("Found .dodotignore file")
		displayPack.IsIgnored = true
		displayPack.Files = append(displayPack.Files, types.DisplayFile{
			PowerUp: "",
			Path:    ".dodotignore",
			Status:  "ignored",
			Message: "dodot is ignoring this dir",
		})
		return nil
	}

	// Check for .dodot.toml
	configPath := filepath.Join(pack.Path, ".dodot.toml")
	if _, err := fs.Stat(configPath); err == nil {
		logger.Debug().Msg("Found .dodot.toml file")
		displayPack.HasConfig = true
		displayPack.Files = append(displayPack.Files, types.DisplayFile{
			PowerUp: "config",
			Path:    ".dodot.toml",
			Status:  "config",
			Message: "dodot config file found",
		})
	}

	return nil
}

// getActionDisplayStatus converts an action and its status to a DisplayFile
func getActionDisplayStatus(action types.Action, fs types.FS, paths types.Pather) (*types.DisplayFile, error) {
	logger := logging.GetLogger("core.pack_status").With().
		Str("action", action.Description).
		Str("type", string(action.Type)).
		Logger()

	// Check the action's deployment status
	status, err := action.CheckStatus(fs, paths)
	if err != nil {
		return nil, errors.Wrapf(err, errors.ErrStatusCheck,
			"failed to check status for action %s", action.Description)
	}

	logger.Debug().
		Str("state", string(status.State)).
		Str("message", status.Message).
		Msg("Action status checked")

	// Determine the display file path
	filePath := getDisplayPath(action)

	// Check if this is an override (indicated by metadata)
	isOverride := false
	if override, ok := action.Metadata["override"].(bool); ok && override {
		isOverride = true
	}

	displayFile := &types.DisplayFile{
		PowerUp:      getPowerUpDisplayName(action),
		Path:         filePath,
		Status:       mapStatusStateToDisplay(status.State),
		Message:      status.Message,
		IsOverride:   isOverride,
		LastExecuted: status.Timestamp,
	}

	return displayFile, nil
}

// getDisplayPath determines the file path to show for an action
// The display path should match what users expect to see in the output.
// For symlinks, we use the target's basename to match the intermediate symlink naming.
// For source-based actions (install scripts, etc.), we use the source's basename.
// For target-only actions (write, mkdir), we use the target's basename.
func getDisplayPath(action types.Action) string {
	switch action.Type {
	case types.ActionTypeLink:
		// Use target basename to match how intermediate symlinks are named
		// This ensures consistency with GetDeployedSymlinkPath
		return filepath.Base(action.Target)
	case types.ActionTypeCopy, types.ActionTypeInstall:
		// Source-based actions: show the source file being processed
		return filepath.Base(action.Source)
	case types.ActionTypeBrew:
		// Always "Brewfile" for consistency
		return "Brewfile"
	case types.ActionTypePathAdd:
		// Show the directory name being added to PATH
		return filepath.Base(action.Source)
	case types.ActionTypeShellSource:
		// Show the script being sourced
		return filepath.Base(action.Source)
	case types.ActionTypeWrite, types.ActionTypeMkdir:
		// Target-based actions: show what's being created
		return filepath.Base(action.Target)
	default:
		logger := logging.GetLogger("core.pack_status")
		return handleUnknownEnum(logger, "actionType", action.Type, "getDisplayPath", action.Description)
	}
}

// getPowerUpDisplayName returns the display name for a power-up based on action type
func getPowerUpDisplayName(action types.Action) string {
	// Map action types to power-up display names
	switch action.Type {
	case types.ActionTypeLink:
		return "symlink"
	case types.ActionTypeBrew:
		return "homebrew"
	case types.ActionTypeInstall:
		return "install"
	case types.ActionTypePathAdd:
		return "path"
	case types.ActionTypeShellSource:
		return "shell_profile"
	case types.ActionTypeWrite:
		return "write"
	case types.ActionTypeMkdir:
		return "mkdir"
	default:
		logger := logging.GetLogger("core.pack_status")
		// Use the PowerUpName from the action if available, otherwise use the action type
		fallback := action.PowerUpName
		if fallback == "" {
			fallback = string(action.Type)
		}
		return handleUnknownEnum(logger, "actionType", action.Type, "getPowerUpDisplayName", fallback)
	}
}

// mapStatusStateToDisplay converts internal StatusState to display status string
func mapStatusStateToDisplay(state types.StatusState) string {
	switch state {
	case types.StatusStateSuccess:
		return "success"
	case types.StatusStatePending:
		return "queue"
	case types.StatusStateError:
		return "error"
	case types.StatusStateIgnored:
		return "ignored"
	case types.StatusStateConfig:
		return "config"
	default:
		logger := logging.GetLogger("core.pack_status")
		return handleUnknownEnum(logger, "state", state, "mapStatusStateToDisplay", "queue")
	}
}

// GetMultiPackStatus processes multiple packs and returns a DisplayResult
func GetMultiPackStatus(packList []types.Pack, command string, fs types.FS, paths types.Pather) (*types.DisplayResult, error) {
	logger := logging.GetLogger("core.pack_status").With().
		Str("command", command).
		Int("packCount", len(packList)).
		Logger()

	logger.Debug().Msg("Getting multi-pack status")

	result := &types.DisplayResult{
		Command:   command,
		Packs:     []types.DisplayPack{},
		DryRun:    false,
		Timestamp: time.Now(),
	}

	// Process each pack
	for _, pack := range packList {
		logger.Debug().Str("pack", pack.Name).Msg("Processing pack")

		// Check if pack should be ignored BEFORE processing triggers
		// This ensures we don't scan ignored packs for privacy reasons
		if packs.ShouldIgnorePackFS(pack.Path, fs) {
			logger.Info().Str("pack", pack.Name).Msg("Pack has .dodotignore, skipping trigger processing")

			// Create display pack with just the ignore status
			displayPack := &types.DisplayPack{
				Name:      pack.Name,
				Status:    "ignored",
				IsIgnored: true,
				Files: []types.DisplayFile{
					{
						PowerUp: "",
						Path:    ".dodotignore",
						Status:  "ignored",
						Message: "dodot is ignoring this dir",
					},
				},
			}
			result.Packs = append(result.Packs, *displayPack)
			continue
		}

		// Get triggers and actions for this pack
		triggers, err := GetFiringTriggersFS([]types.Pack{pack}, fs)
		if err != nil {
			logger.Error().Err(err).Str("pack", pack.Name).Msg("Failed to get triggers")
			return nil, errors.Wrapf(err, errors.ErrTriggerExecute,
				"failed to get triggers for pack %s", pack.Name)
		}

		actions, err := GetActions(triggers)
		if err != nil {
			logger.Error().Err(err).Str("pack", pack.Name).Msg("Failed to get actions")
			return nil, errors.Wrapf(err, errors.ErrActionCreate,
				"failed to get actions for pack %s", pack.Name)
		}

		// Get pack status
		displayPack, err := GetPackStatus(pack, actions, fs, paths)
		if err != nil {
			logger.Error().Err(err).Str("pack", pack.Name).Msg("Failed to get pack status")
			return nil, err
		}

		result.Packs = append(result.Packs, *displayPack)
	}

	logger.Info().
		Int("packCount", len(result.Packs)).
		Msg("Multi-pack status complete")

	return result, nil
}
