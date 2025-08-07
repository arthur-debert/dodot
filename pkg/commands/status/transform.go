package status

import (
	"path/filepath"
	"strings"
	"time"

	"github.com/arthur-debert/dodot/pkg/config"
	"github.com/arthur-debert/dodot/pkg/types"
)

// CreateDisplayResultFromOperations transforms a list of operations into a DisplayResult.
// This is the core transformation that creates the file-centric display model from operations.
func CreateDisplayResultFromOperations(operations []types.Operation, packs []types.Pack, command string) *types.DisplayResult {
	result := &types.DisplayResult{
		Command:   command,
		Timestamp: time.Now(),
		Packs:     make([]types.DisplayPack, 0),
	}

	// Create a map to organize operations by pack
	packOpsMap := make(map[string][]types.Operation)
	packMap := make(map[string]types.Pack)

	// Also track which packs we've seen
	for _, pack := range packs {
		packMap[pack.Name] = pack
		packOpsMap[pack.Name] = []types.Operation{} // Initialize even if no operations
	}

	// Group operations by pack
	for _, op := range operations {
		if op.Pack != "" {
			packOpsMap[op.Pack] = append(packOpsMap[op.Pack], op)
		}
	}

	// Process each pack
	for packName, pack := range packMap {
		displayPack := types.DisplayPack{
			Name:  packName,
			Files: make([]types.DisplayFile, 0),
		}

		// Check for config file
		configPath := filepath.Join(pack.Path, ".dodot.toml")
		if config.FileExists(configPath) {
			displayPack.HasConfig = true
			displayPack.Files = append(displayPack.Files, types.DisplayFile{
				Path:    ".dodot.toml",
				PowerUp: "config",
				Status:  "config",
				Message: "dodot config file found",
			})
		}

		// Check for .dodotignore
		ignorePath := filepath.Join(pack.Path, ".dodotignore")
		if config.FileExists(ignorePath) {
			displayPack.IsIgnored = true
			// If pack is ignored, only show the ignore file
			displayPack.Files = []types.DisplayFile{
				{
					Path:    ".dodotignore",
					PowerUp: "",
					Status:  "ignored",
					Message: "dodot is ignoring this dir",
				},
			}
			displayPack.Status = "ignored"
			result.Packs = append(result.Packs, displayPack)
			continue
		}

		// Convert operations to display files
		filesMap := make(map[string]*types.DisplayFile)

		for _, op := range packOpsMap[packName] {
			filePath := getRelativeFilePath(op, pack.Path)

			// Skip if we already have this file
			if existingFile, exists := filesMap[filePath]; exists {
				// If multiple operations affect the same file, that's an error
				if existingFile.PowerUp != op.PowerUp {
					existingFile.Status = "error"
					existingFile.Message = "Multiple power-ups for same file"
				}
				continue
			}

			displayFile := convertOperationToDisplayFile(op, filePath)
			filesMap[filePath] = &displayFile
		}

		// Add all files to the pack
		for _, file := range filesMap {
			displayPack.Files = append(displayPack.Files, *file)
		}

		// Calculate and set pack status
		displayPack.Status = displayPack.GetPackStatus()

		result.Packs = append(result.Packs, displayPack)
	}

	return result
}

// CreateDisplayResultFromActions transforms a list of actions into a DisplayResult.
// This is similar to CreateDisplayResultFromOperations but works with Actions directly.
func CreateDisplayResultFromActions(actions []types.Action, packs []types.Pack, command string) *types.DisplayResult {
	result := &types.DisplayResult{
		Command:   command,
		Timestamp: time.Now(),
		Packs:     make([]types.DisplayPack, 0),
	}

	// Create a map to organize actions by pack
	packActionsMap := make(map[string][]types.Action)
	packMap := make(map[string]types.Pack)

	// Also track which packs we've seen
	for _, pack := range packs {
		packMap[pack.Name] = pack
		packActionsMap[pack.Name] = []types.Action{} // Initialize even if no actions
	}

	// Group actions by pack
	for _, action := range actions {
		if action.Pack != "" {
			packActionsMap[action.Pack] = append(packActionsMap[action.Pack], action)
		}
	}

	// Process each pack
	for packName, pack := range packMap {
		displayPack := types.DisplayPack{
			Name:  packName,
			Files: make([]types.DisplayFile, 0),
		}

		// Check for config file
		configPath := filepath.Join(pack.Path, ".dodot.toml")
		if config.FileExists(configPath) {
			displayPack.HasConfig = true
			displayPack.Files = append(displayPack.Files, types.DisplayFile{
				Path:    ".dodot.toml",
				PowerUp: "config",
				Status:  "config",
				Message: "dodot config file found",
			})
		}

		// Check for .dodotignore
		ignorePath := filepath.Join(pack.Path, ".dodotignore")
		if config.FileExists(ignorePath) {
			displayPack.IsIgnored = true
			// If pack is ignored, only show the ignore file
			displayPack.Files = []types.DisplayFile{
				{
					Path:    ".dodotignore",
					PowerUp: "",
					Status:  "ignored",
					Message: "dodot is ignoring this dir",
				},
			}
			displayPack.Status = "ignored"
			result.Packs = append(result.Packs, displayPack)
			continue
		}

		// Convert actions to display files
		filesMap := make(map[string]*types.DisplayFile)

		for _, action := range packActionsMap[packName] {
			filePath := getRelativeFilePathFromAction(action, pack.Path)

			// Skip if we already have this file
			if existingFile, exists := filesMap[filePath]; exists {
				// If multiple actions affect the same file, that's an error
				if existingFile.PowerUp != action.PowerUpName {
					existingFile.Status = "error"
					existingFile.Message = "Multiple power-ups for same file"
				}
				continue
			}

			displayFile := convertActionToDisplayFile(action, filePath)
			filesMap[filePath] = &displayFile
		}

		// Add all files to the pack
		for _, file := range filesMap {
			displayPack.Files = append(displayPack.Files, *file)
		}

		// Calculate and set pack status
		displayPack.Status = displayPack.GetPackStatus()

		result.Packs = append(result.Packs, displayPack)
	}

	return result
}

// convertActionToDisplayFile converts a single action to a display file
func convertActionToDisplayFile(action types.Action, filePath string) types.DisplayFile {
	displayFile := types.DisplayFile{
		Path:    filePath,
		PowerUp: action.PowerUpName,
	}

	// Mark overrides based on metadata (if available)
	if action.Metadata != nil {
		if triggerName, ok := action.Metadata["trigger_name"].(string); ok && triggerName == "override-rule" {
			displayFile.IsOverride = true
			displayFile.Path = "*" + displayFile.Path
		}
	}

	// Actions are always ready to execute, unlike Operations which have different statuses
	displayFile.Status = "queue"
	displayFile.Message = getVerbForPowerUp(action.PowerUpName, false) // future tense

	return displayFile
}

// getRelativeFilePathFromAction extracts the file path relative to the pack from an action
func getRelativeFilePathFromAction(action types.Action, packPath string) string {
	// Use original path from metadata if available (most accurate)
	if action.Metadata != nil {
		if originalPath, ok := action.Metadata["original_path"].(string); ok && originalPath != "" {
			return originalPath
		}
	}

	// Try to extract from source path
	if action.Source != "" {
		relPath, err := filepath.Rel(packPath, action.Source)
		if err == nil && !filepath.IsAbs(relPath) && relPath != "." && !strings.HasPrefix(relPath, "..") {
			return relPath
		}
		// Fallback to base name
		return filepath.Base(action.Source)
	}

	// For actions without source, use target
	if action.Target != "" {
		return filepath.Base(action.Target)
	}

	return "unknown"
}

// convertOperationToDisplayFile converts a single operation to a display file
func convertOperationToDisplayFile(op types.Operation, filePath string) types.DisplayFile {
	displayFile := types.DisplayFile{
		Path:    filePath,
		PowerUp: op.PowerUp,
	}

	// Mark overrides
	if op.TriggerInfo != nil && op.TriggerInfo.TriggerName == "override-rule" {
		displayFile.IsOverride = true
		displayFile.Path = "*" + displayFile.Path
	}

	// Set status and message based on operation status
	switch op.Status {
	case types.StatusReady:
		displayFile.Status = "queue"
		displayFile.Message = getVerbForPowerUp(op.PowerUp, false) // future tense
	case types.StatusSkipped:
		displayFile.Status = "success"
		displayFile.Message = getVerbForPowerUp(op.PowerUp, true) // past tense
	case types.StatusError:
		displayFile.Status = "error"
		displayFile.Message = "Failed"
		if op.Description != "" {
			displayFile.Message = op.Description
		}
	case types.StatusConflict:
		displayFile.Status = "error"
		displayFile.Message = "Conflict detected"
	default:
		displayFile.Status = "queue"
		displayFile.Message = "To be processed"
	}

	return displayFile
}

// getRelativeFilePath extracts the file path relative to the pack
func getRelativeFilePath(op types.Operation, packPath string) string {
	// Use TriggerInfo if available (most accurate)
	if op.TriggerInfo != nil && op.TriggerInfo.OriginalPath != "" {
		return op.TriggerInfo.OriginalPath
	}

	// Try to extract from source path
	if op.Source != "" {
		relPath, err := filepath.Rel(packPath, op.Source)
		if err == nil && !filepath.IsAbs(relPath) && relPath != "." && !strings.HasPrefix(relPath, "..") {
			return relPath
		}
		// Fallback to base name
		return filepath.Base(op.Source)
	}

	// For operations without source, use target
	if op.Target != "" {
		return filepath.Base(op.Target)
	}

	return "unknown"
}

// getVerbForPowerUp returns the appropriate verb for a power-up type
func getVerbForPowerUp(powerUp string, past bool) string {
	verbs := map[string]struct{ past, future string }{
		"symlink":       {past: "linked to target", future: "will be linked to target"},
		"shell_profile": {past: "included in shell", future: "to be included in shell"},
		"homebrew":      {past: "executed", future: "to be installed"},
		"add_path":      {past: "added to $PATH", future: "to be added to $PATH"},
		"install":       {past: "executed", future: "to be executed"},
		"template":      {past: "generated from template", future: "to be generated"},
		"config":        {past: "found", future: "found"},
	}

	if verb, ok := verbs[powerUp]; ok {
		if past {
			return verb.past
		}
		return verb.future
	}

	// Default
	if past {
		return "processed"
	}
	return "to be processed"
}
