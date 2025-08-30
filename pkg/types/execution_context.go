package types

import (
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"time"
)

// ExecutionStatus represents the overall status of a pack's execution
type ExecutionStatus string

const (
	// ExecutionStatusSuccess means all actions succeeded
	ExecutionStatusSuccess ExecutionStatus = "success"

	// ExecutionStatusPartial means some actions succeeded, some failed
	ExecutionStatusPartial ExecutionStatus = "partial"

	// ExecutionStatusError means all actions failed
	ExecutionStatusError ExecutionStatus = "error"

	// ExecutionStatusSkipped means all actions were skipped
	ExecutionStatusSkipped ExecutionStatus = "skipped"

	// ExecutionStatusPending means execution hasn't started
	ExecutionStatusPending ExecutionStatus = "pending"
)

// ExecutionContext tracks the complete context and results of a command execution
type ExecutionContext struct {
	// Command is the command being executed (deploy, install, etc.)
	Command string

	// PackResults contains results organized by pack
	PackResults map[string]*PackExecutionResult

	// StartTime is when execution began
	StartTime time.Time

	// EndTime is when execution completed
	EndTime time.Time

	// DryRun indicates if this was a dry run
	DryRun bool

	// TotalHandlers is the total count of handlers across all packs
	TotalHandlers int

	// CompletedHandlers is the count of successfully completed handlers
	CompletedHandlers int

	// FailedHandlers is the count of failed handlers
	FailedHandlers int

	// SkippedHandlers is the count of skipped handlers
	SkippedHandlers int
}

// PackExecutionResult contains the execution results for a single pack
type PackExecutionResult struct {
	// Pack is the pack being executed
	Pack *Pack

	// HandlerResults contains all Handler results and their status
	HandlerResults []*HandlerResult

	// Status is the aggregated status for this pack
	Status ExecutionStatus

	// StartTime is when this pack's execution began
	StartTime time.Time

	// EndTime is when this pack's execution completed
	EndTime time.Time

	// TotalHandlers in this pack
	TotalHandlers int

	// CompletedHandlers in this pack
	CompletedHandlers int

	// FailedHandlers in this pack
	FailedHandlers int

	// SkippedHandlers in this pack
	SkippedHandlers int
}

// HandlerResult tracks the result of a single Handler execution
// This is the atomic unit - if ANY action in a Handler fails, the Handler fails
type HandlerResult struct {
	// HandlerName is the name of the Handler (symlink, homebrew, etc.)
	HandlerName string

	// Files are the files processed by this Handler
	Files []string

	// Status is the final status after execution
	Status OperationStatus

	// Error contains any error that occurred
	Error error

	// StartTime is when the Handler execution began
	StartTime time.Time

	// EndTime is when the Handler execution completed
	EndTime time.Time

	// Message provides additional context
	Message string

	// Pack is the pack this Handler belongs to
	Pack string

	// Operations are the operations that were executed
	// TODO: Type this properly when operations package is available
	Operations []interface{}
}

// NewExecutionContext creates a new execution context
func NewExecutionContext(command string, dryRun bool) *ExecutionContext {
	return &ExecutionContext{
		Command:     command,
		PackResults: make(map[string]*PackExecutionResult),
		StartTime:   time.Now(),
		DryRun:      dryRun,
	}
}

// AddPackResult adds or updates a pack result
func (ec *ExecutionContext) AddPackResult(packName string, result *PackExecutionResult) {
	ec.PackResults[packName] = result

	// Update totals based on Handlers, not Operations
	ec.TotalHandlers = 0
	ec.CompletedHandlers = 0
	ec.FailedHandlers = 0
	ec.SkippedHandlers = 0

	for _, pr := range ec.PackResults {
		ec.TotalHandlers += pr.TotalHandlers
		ec.CompletedHandlers += pr.CompletedHandlers
		ec.FailedHandlers += pr.FailedHandlers
		ec.SkippedHandlers += pr.SkippedHandlers
	}
}

// GetPackResult retrieves a pack result by name
func (ec *ExecutionContext) GetPackResult(packName string) (*PackExecutionResult, bool) {
	result, ok := ec.PackResults[packName]
	return result, ok
}

// Complete marks the execution as complete
func (ec *ExecutionContext) Complete() {
	ec.EndTime = time.Now()
}

// NewPackExecutionResult creates a new pack execution result
func NewPackExecutionResult(pack *Pack) *PackExecutionResult {
	return &PackExecutionResult{
		Pack:           pack,
		HandlerResults: make([]*HandlerResult, 0),
		Status:         ExecutionStatusPending,
		StartTime:      time.Now(),
	}
}

// AddHandlerResult adds a Handler result and updates statistics
func (per *PackExecutionResult) AddHandlerResult(result *HandlerResult) {
	per.HandlerResults = append(per.HandlerResults, result)
	per.TotalHandlers++

	switch result.Status {
	case StatusReady:
		per.CompletedHandlers++
	case StatusSkipped:
		per.SkippedHandlers++
	case StatusError, StatusConflict:
		per.FailedHandlers++
	}

	// Update pack status
	per.updateStatus()
}

// updateStatus recalculates the pack's aggregated status
func (per *PackExecutionResult) updateStatus() {
	if per.TotalHandlers == 0 {
		per.Status = ExecutionStatusPending
		return
	}

	if per.FailedHandlers == per.TotalHandlers {
		per.Status = ExecutionStatusError
	} else if per.SkippedHandlers == per.TotalHandlers {
		per.Status = ExecutionStatusSkipped
	} else if per.FailedHandlers > 0 {
		per.Status = ExecutionStatusPartial
	} else {
		per.Status = ExecutionStatusSuccess
	}
}

// Complete marks the pack execution as complete
func (per *PackExecutionResult) Complete() {
	per.EndTime = time.Now()
	per.updateStatus()
}

// ToDisplayResult transforms the ExecutionContext into a DisplayResult suitable for rendering
func (ec *ExecutionContext) ToDisplayResult() *DisplayResult {
	displayPacks := make([]DisplayPack, 0, len(ec.PackResults))

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

		displayPack := DisplayPack{
			Name:      packName,
			Files:     make([]DisplayFile, 0),
			HasConfig: hasConfig,
			IsIgnored: isIgnored,
		}

		// Add config files as display items (per display.txxt spec)
		if hasConfig {
			displayPack.Files = append(displayPack.Files, DisplayFile{
				Handler: "config",
				Path:    ".dodot.toml",
				Status:  "config",
				Message: "dodot config file found",
			})
		}
		if isIgnored {
			displayPack.Files = append(displayPack.Files, DisplayFile{
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
				if pur.Status == StatusReady && !pur.EndTime.IsZero() {
					lastExecuted = &pur.EndTime
				}

				// Generate Handler-aware display message
				displayStatus := mapOperationStatusToDisplayStatus(pur.Status)
				displayMessage := generateHandlerMessage(pur.HandlerName, filePath, displayStatus, lastExecuted)

				// Get additional info based on Handler type and action data
				additionalInfo := GetHandlerAdditionalInfo(pur.HandlerName)

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

				displayFile := DisplayFile{
					Handler:        pur.HandlerName,
					Path:           filePath,
					Status:         displayStatus,
					Message:        displayMessage,
					IsOverride:     isOverride,
					LastExecuted:   lastExecuted,
					HandlerSymbol:  GetHandlerSymbol(pur.HandlerName),
					AdditionalInfo: additionalInfo,
				}
				displayPack.Files = append(displayPack.Files, displayFile)
			}
		}

		// Set pack status based on aggregation rules
		displayPack.Status = displayPack.GetPackStatus()
		displayPacks = append(displayPacks, displayPack)
	}

	return &DisplayResult{
		Command:   ec.Command,
		Packs:     displayPacks,
		DryRun:    ec.DryRun,
		Timestamp: ec.EndTime,
	}
}

// mapOperationStatusToDisplayStatus converts internal OperationStatus to display status string
func mapOperationStatusToDisplayStatus(status OperationStatus) string {
	switch status {
	case StatusReady:
		return "success"
	case StatusError:
		return "error"
	case StatusSkipped:
		return "queue"
	case StatusConflict:
		return "error"
	default:
		return "queue"
	}
}

// checkPackConfiguration checks for .dodot.toml and .dodotignore files in the pack directory
func checkPackConfiguration(pack *Pack) (hasConfig bool, isIgnored bool) {
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
