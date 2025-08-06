package status

import (
	"path/filepath"
	"time"

	"github.com/arthur-debert/dodot/pkg/config"
	"github.com/arthur-debert/dodot/pkg/core"
	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/paths"
	"github.com/arthur-debert/dodot/pkg/types"
)

// StatusPacksNew checks the deployment status of the specified packs
// using the core pipeline and returning the new DisplayResult format.
// This will replace the existing StatusPacks function.
func StatusPacksNew(opts StatusPacksOptions) (*types.DisplayResult, error) {
	log := logging.GetLogger("core.commands")
	log.Debug().Str("command", "StatusPacksNew").Msg("Executing command")

	// Initialize Paths instance
	pathsInstance, err := paths.New(opts.DotfilesRoot)
	if err != nil {
		return nil, err
	}

	// 1. Get all packs using the core pipeline
	candidates, err := core.GetPackCandidates(opts.DotfilesRoot)
	if err != nil {
		return nil, err
	}
	allPacks, err := core.GetPacks(candidates)
	if err != nil {
		return nil, err
	}

	// 2. Filter to selected packs
	selectedPacks, err := core.SelectPacks(allPacks, opts.PackNames)
	if err != nil {
		return nil, err
	}

	// 3. Use the core pipeline to get triggers and actions
	result := &types.DisplayResult{
		Command:   "status",
		Timestamp: time.Now(),
		Packs:     make([]types.DisplayPack, 0, len(selectedPacks)),
	}

	for _, pack := range selectedPacks {
		displayPack := types.DisplayPack{
			Name:  pack.Name,
			Files: make([]types.DisplayFile, 0),
		}

		// Check for config file
		configPath := filepath.Join(pack.Path, ".dodot.toml")
		if config.FileExists(configPath) {
			displayPack.HasConfig = true
			displayPack.Files = append(displayPack.Files, types.DisplayFile{
				Path:    ".dodot.toml",
				PowerUp: "config",
				Status:  types.DisplayStatusConfig,
				Message: "dodot config file found",
			})
		}

		// Check for .dodotignore
		ignorePath := filepath.Join(pack.Path, ".dodotignore")
		if config.FileExists(ignorePath) {
			displayPack.IsIgnored = true
			// If pack is ignored, we only show the ignore file
			displayPack.Files = []types.DisplayFile{
				{
					Path:    ".dodotignore",
					PowerUp: "",
					Status:  types.DisplayStatusIgnored,
					Message: "dodot is ignoring this dir",
				},
			}
			displayPack.Status = types.DisplayStatusIgnored
			result.Packs = append(result.Packs, displayPack)
			continue
		}

		// Get firing triggers for this pack using the core pipeline
		triggerMatches, err := core.GetFiringTriggers([]types.Pack{pack})
		if err != nil {
			log.Warn().Err(err).Str("pack", pack.Name).Msg("Failed to get triggers")
			continue
		}

		// Get actions from triggers
		actions, err := core.GetActions(triggerMatches)
		if err != nil {
			log.Warn().Err(err).Str("pack", pack.Name).Msg("Failed to get actions")
			continue
		}

		// Convert actions to operations
		ctx := core.NewExecutionContext(false, pathsInstance)
		operations, err := core.ConvertActionsToOperationsWithContext(actions, ctx)
		if err != nil {
			log.Warn().Err(err).Str("pack", pack.Name).Msg("Failed to get operations")
			continue
		}

		// Convert operations to display files
		filesMap := make(map[string]*types.DisplayFile)

		for _, op := range operations {
			// Get the file path relative to pack
			filePath := getRelativeFilePath(op, pack.Path)

			// Create or update display file
			if existingFile, exists := filesMap[filePath]; exists {
				// Update existing file if needed
				updateDisplayFile(existingFile, op, *pathsInstance)
			} else {
				displayFile := convertOperationToDisplayFile(op, filePath, *pathsInstance)
				filesMap[filePath] = &displayFile
			}
		}

		// Add all files to the pack
		for _, file := range filesMap {
			displayPack.Files = append(displayPack.Files, *file)
		}

		// Calculate pack status based on files
		displayPack.Status = displayPack.GetPackStatus()

		result.Packs = append(result.Packs, displayPack)
	}

	log.Info().Str("command", "StatusPacksNew").Int("packCount", len(result.Packs)).Msg("Command finished")
	return result, nil
}

// convertOperationToDisplayFile converts an operation to a display file
func convertOperationToDisplayFile(op types.Operation, filePath string, paths paths.Paths) types.DisplayFile {
	displayFile := types.DisplayFile{
		Path:    filePath,
		PowerUp: op.PowerUp,
	}

	// Mark overrides
	if op.TriggerInfo != nil && op.TriggerInfo.TriggerName == "override-rule" {
		displayFile.IsOverride = true
		displayFile.Path = "*" + displayFile.Path
	}

	// Check status based on operation type and current state
	status, message, lastExecuted := checkOperationStatus(op, paths)
	displayFile.Status = status
	displayFile.Message = message
	displayFile.LastExecuted = lastExecuted

	return displayFile
}

// checkOperationStatus determines the status of an operation
func checkOperationStatus(op types.Operation, paths paths.Paths) (types.DisplayStatus, string, *time.Time) {
	// Check run-once status for install and homebrew
	if op.PowerUp == "install" || op.PowerUp == "homebrew" {
		status, err := core.GetRunOnceStatus(filepath.Dir(op.Source), op.PowerUp, &paths)
		if err == nil && status != nil && status.Executed {
			if status.Changed {
				return types.DisplayStatusQueue, "to be executed (changed)", nil
			}
			return types.DisplayStatusSuccess, "executed", &status.ExecutedAt
		}
		return types.DisplayStatusQueue, "to be executed", nil
	}

	// For symlinks, check if they exist and are valid
	if op.Type == types.OperationCreateSymlink {
		// Check if symlink exists and points to correct target
		// This is simplified - real implementation would check actual filesystem
		return types.DisplayStatusQueue, "will be linked to target", nil
	}

	// Default status for other operations
	return types.DisplayStatusQueue, "to be processed", nil
}

// getRelativeFilePath extracts the file path relative to the pack
func getRelativeFilePath(op types.Operation, packPath string) string {
	if op.TriggerInfo != nil && op.TriggerInfo.OriginalPath != "" {
		return op.TriggerInfo.OriginalPath
	}

	// Try to extract from source path
	if op.Source != "" {
		relPath, err := filepath.Rel(packPath, op.Source)
		if err == nil {
			return relPath
		}
	}

	// Fallback to filename
	if op.Source != "" {
		return filepath.Base(op.Source)
	}
	if op.Target != "" {
		return filepath.Base(op.Target)
	}

	return "unknown"
}

// updateDisplayFile updates an existing display file with new operation info
func updateDisplayFile(file *types.DisplayFile, op types.Operation, paths paths.Paths) {
	// If this operation has a different power-up, it might be a conflict
	if file.PowerUp != op.PowerUp {
		file.Status = types.DisplayStatusError
		file.Message = "Multiple power-ups for same file"
		return
	}

	// Otherwise update status if needed
	status, message, lastExecuted := checkOperationStatus(op, paths)
	file.Status = status
	file.Message = message
	if lastExecuted != nil {
		file.LastExecuted = lastExecuted
	}
}
