package types

import (
	"os"
	"path/filepath"
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

	// TotalActions is the total count of actions across all packs
	TotalActions int

	// CompletedActions is the count of successfully completed actions
	CompletedActions int

	// FailedActions is the count of failed actions
	FailedActions int

	// SkippedActions is the count of skipped actions
	SkippedActions int
}

// PackExecutionResult contains the execution results for a single pack
type PackExecutionResult struct {
	// Pack is the pack being executed
	Pack *Pack

	// PowerUpResults contains all PowerUp results and their status
	PowerUpResults []*PowerUpResult

	// Status is the aggregated status for this pack
	Status ExecutionStatus

	// StartTime is when this pack's execution began
	StartTime time.Time

	// EndTime is when this pack's execution completed
	EndTime time.Time

	// TotalPowerUps in this pack
	TotalPowerUps int

	// CompletedPowerUps in this pack
	CompletedPowerUps int

	// FailedPowerUps in this pack
	FailedPowerUps int

	// SkippedPowerUps in this pack
	SkippedPowerUps int
}

// PowerUpResult tracks the result of a single PowerUp execution
// This is the atomic unit - if ANY action in a PowerUp fails, the PowerUp fails
type PowerUpResult struct {
	// PowerUpName is the name of the PowerUp (symlink, homebrew, etc.)
	PowerUpName string

	// Files are the files processed by this PowerUp
	Files []string

	// Status is the final status after execution
	Status OperationStatus

	// Error contains any error that occurred
	Error error

	// StartTime is when the PowerUp execution began
	StartTime time.Time

	// EndTime is when the PowerUp execution completed
	EndTime time.Time

	// Message provides additional context
	Message string

	// Pack is the pack this PowerUp belongs to
	Pack string
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

	// Update totals based on PowerUps, not Operations
	ec.TotalActions = 0
	ec.CompletedActions = 0
	ec.FailedActions = 0
	ec.SkippedActions = 0

	for _, pr := range ec.PackResults {
		ec.TotalActions += pr.TotalPowerUps
		ec.CompletedActions += pr.CompletedPowerUps
		ec.FailedActions += pr.FailedPowerUps
		ec.SkippedActions += pr.SkippedPowerUps
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
		PowerUpResults: make([]*PowerUpResult, 0),
		Status:         ExecutionStatusPending,
		StartTime:      time.Now(),
	}
}

// AddPowerUpResult adds a PowerUp result and updates statistics
func (per *PackExecutionResult) AddPowerUpResult(result *PowerUpResult) {
	per.PowerUpResults = append(per.PowerUpResults, result)
	per.TotalPowerUps++

	switch result.Status {
	case StatusReady:
		per.CompletedPowerUps++
	case StatusSkipped:
		per.SkippedPowerUps++
	case StatusError, StatusConflict:
		per.FailedPowerUps++
	}

	// Update pack status
	per.updateStatus()
}

// updateStatus recalculates the pack's aggregated status
func (per *PackExecutionResult) updateStatus() {
	if per.TotalPowerUps == 0 {
		per.Status = ExecutionStatusPending
		return
	}

	if per.FailedPowerUps == per.TotalPowerUps {
		per.Status = ExecutionStatusError
	} else if per.SkippedPowerUps == per.TotalPowerUps {
		per.Status = ExecutionStatusSkipped
	} else if per.FailedPowerUps > 0 {
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

// FileStatus represents the current status of a file managed by dodot
type FileStatus struct {
	// Path is the file or directory path
	Path string

	// PowerUp is the power-up that manages this file
	PowerUp string

	// Status is the current status of the file
	Status OperationStatus

	// Message provides additional context about the status
	Message string

	// LastApplied is when the file was last successfully applied
	LastApplied time.Time

	// Metadata contains power-up specific status information
	// For example:
	// - Symlinks: target path, whether link is valid
	// - Profiles: which shell files contain entries
	// - PATH: whether directory is in PATH
	// - Homebrew: package version, installation status
	Metadata map[string]interface{}
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
				PowerUp: "config",
				Path:    ".dodot.toml",
				Status:  "config",
				Message: "dodot config file found",
			})
		}
		if isIgnored {
			displayPack.Files = append(displayPack.Files, DisplayFile{
				PowerUp: ".dodotignore",
				Path:    "",
				Status:  "ignored",
				Message: "dodot is ignoring this dir",
			})
		}

		// Transform PowerUpResults to DisplayFiles
		for _, pur := range packResult.PowerUpResults {
			// Create a DisplayFile for each file in the PowerUpResult
			for _, filePath := range pur.Files {
				displayFile := DisplayFile{
					PowerUp: pur.PowerUpName,
					Path:    filePath,
					Status:  mapOperationStatusToDisplayStatus(pur.Status),
					Message: pur.Message,
					// TODO: Add IsOverride and LastExecuted when available
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
