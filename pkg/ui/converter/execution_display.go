package converter

import (
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"time"

	"github.com/arthur-debert/dodot/pkg/execution"
	"github.com/arthur-debert/dodot/pkg/types"
)

// ConvertToDisplay transforms the ExecutionContext into a DisplayResult suitable for rendering
func ConvertToDisplay(ec *types.ExecutionContext) *types.DisplayResult {
	displayPacks := make([]types.DisplayPack, 0, len(ec.PackResults))

	// Sort pack names for consistent output
	packNames := make([]string, 0, len(ec.PackResults))
	for name := range ec.PackResults {
		packNames = append(packNames, name)
	}
	// Simple sort - could enhance with natural sort later
	for i := 0; i < len(packNames); i++ {
		for j := i + 1; j < len(packNames); j++ {
			if packNames[i] > packNames[j] {
				packNames[i], packNames[j] = packNames[j], packNames[i]
			}
		}
	}

	// Transform each pack
	for _, packName := range packNames {
		packResult := ec.PackResults[packName]

		// Check for configuration files
		hasConfig, isIgnored := checkPackConfiguration(packResult.Pack)

		displayPack := types.DisplayPack{
			Name:      packName,
			Files:     make([]types.DisplayFile, 0),
			HasConfig: hasConfig,
			IsIgnored: isIgnored,
		}

		// Add config files as display items (per display.txxt spec)
		if hasConfig {
			displayPack.Files = append(displayPack.Files, types.DisplayFile{
				Handler: "config",
				Path:    ".dodot.toml",
				Status:  "config",
				Message: "dodot config file found",
			})
		}
		if isIgnored {
			displayPack.Files = append(displayPack.Files, types.DisplayFile{
				Handler: ".dodotignore",
				Path:    "",
				Status:  "ignored",
				Message: "dodot is ignoring this dir",
			})
		}

		// Transform HandlerResults to DisplayFiles
		for _, pur := range packResult.HandlerResults {
			// Create a DisplayFile for each file in the HandlerResult
			for _, filePath := range pur.Files {
				// Check if this file has a handler override in .dodot.toml
				fileName := filepath.Base(filePath)
				isOverride := false
				if packResult.Pack != nil {
					override := packResult.Pack.Config.FindOverride(fileName)
					isOverride = (override != nil)
				}

				// Use HandlerResult EndTime as LastExecuted if execution completed
				var lastExecuted *time.Time
				if pur.Status == execution.StatusReady && !pur.EndTime.IsZero() {
					lastExecuted = &pur.EndTime
				}

				// Generate Handler-aware display message
				displayStatus := mapOperationStatusToDisplayStatus(pur.Status)
				displayMessage := generateHandlerMessage(pur.HandlerName, filePath, displayStatus, lastExecuted)

				// Get additional info based on Handler type and operation data
				additionalInfo := types.GetHandlerAdditionalInfo(pur.HandlerName)

				// Extract handler-specific information based on handler type
				// This logic can be simplified once we have operations
				switch pur.HandlerName {
				case "symlink":
					// For symlinks, show the target path with ~ for home
					additionalInfo = fmt.Sprintf("→ ~/%s", filepath.Base(filePath))

				case "path":
					// For PATH entries, show the directory being added
					additionalInfo = fmt.Sprintf("→ $PATH/%s", filepath.Base(filePath))

				case "shell":
					// For shell profile entries, indicate the shell type if detectable
					fileName := filepath.Base(filePath)
					if strings.Contains(fileName, "bash") {
						additionalInfo = "→ bash profile"
					} else if strings.Contains(fileName, "zsh") {
						additionalInfo = "→ zsh profile"
					} else if strings.Contains(fileName, "fish") {
						additionalInfo = "→ fish config"
					} else {
						additionalInfo = "→ shell profile"
					}
				}

				displayFile := types.DisplayFile{
					Handler:        pur.HandlerName,
					Path:           filePath,
					Status:         displayStatus,
					Message:        displayMessage,
					IsOverride:     isOverride,
					LastExecuted:   lastExecuted,
					HandlerSymbol:  types.GetHandlerSymbol(pur.HandlerName),
					AdditionalInfo: additionalInfo,
				}
				displayPack.Files = append(displayPack.Files, displayFile)
			}
		}

		// Set pack status based on aggregation rules
		displayPack.Status = displayPack.GetPackStatus()
		displayPacks = append(displayPacks, displayPack)
	}

	return &types.DisplayResult{
		Command:   ec.Command,
		Packs:     displayPacks,
		DryRun:    ec.DryRun,
		Timestamp: ec.EndTime,
	}
}

// mapOperationStatusToDisplayStatus converts internal OperationStatus to display status string
func mapOperationStatusToDisplayStatus(status execution.OperationStatus) string {
	switch status {
	case execution.StatusReady:
		return "success"
	case execution.StatusError:
		return "error"
	case execution.StatusSkipped:
		return "queue"
	case execution.StatusConflict:
		return "error"
	default:
		return "queue"
	}
}

// checkPackConfiguration checks for .dodot.toml and .dodotignore files in the pack directory
func checkPackConfiguration(pack *types.Pack) (hasConfig bool, isIgnored bool) {
	if pack == nil || pack.Path == "" {
		return false, false
	}

	// Check for .dodot.toml file
	configPath := filepath.Join(pack.Path, ".dodot.toml")
	if _, err := os.Stat(configPath); err == nil {
		hasConfig = true
	}

	// Check for .dodotignore file
	ignorePath := filepath.Join(pack.Path, ".dodotignore")
	if _, err := os.Stat(ignorePath); err == nil {
		isIgnored = true
	}

	return hasConfig, isIgnored
}

// generateHandlerMessage creates Handler-specific display messages following display.txxt spec
func generateHandlerMessage(handlerName, filePath, status string, lastExecuted *time.Time) string {
	fileName := filepath.Base(filePath)

	switch handlerName {
	case "symlink":
		switch status {
		case "success":
			if lastExecuted != nil {
				return fmt.Sprintf("linked to $HOME/%s", fileName)
			}
			return fmt.Sprintf("linked to %s", fileName)
		case "error":
			return fmt.Sprintf("failed to link to $HOME/%s", fileName)
		default: // queue
			return fmt.Sprintf("will be linked to $HOME/%s", fileName)
		}

	case "shell", "shell_add_path":
		switch status {
		case "success":
			if lastExecuted != nil {
				return "included in shell profile"
			}
			return "added to shell profile"
		case "error":
			return "failed to add to shell profile"
		default: // queue
			return "to be included in shell profile"
		}

	case "homebrew":
		switch status {
		case "success":
			if lastExecuted != nil {
				return fmt.Sprintf("executed on %s", lastExecuted.Format("2006-01-02"))
			}
			return "packages installed"
		case "error":
			return "failed to install packages"
		default: // queue
			return "packages to be installed"
		}

	case "path":
		switch status {
		case "success":
			return fmt.Sprintf("added %s to $PATH", fileName)
		case "error":
			return fmt.Sprintf("failed to add %s to $PATH", fileName)
		default: // queue
			return fmt.Sprintf("%s to be added to $PATH", fileName)
		}

	case "provision":
		switch status {
		case "success":
			if lastExecuted != nil {
				return fmt.Sprintf("executed during installation on %s", lastExecuted.Format("2006-01-02"))
			}
			return "installation completed"
		case "error":
			return "installation failed"
		default: // queue
			return "to be executed during installation"
		}

	default:
		// Fallback for unknown Handler types
		switch status {
		case "success":
			return "completed successfully"
		case "error":
			return "execution failed"
		default: // queue
			return "pending execution"
		}
	}
}
