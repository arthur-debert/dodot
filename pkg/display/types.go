package display

import (
	"time"

	"github.com/arthur-debert/dodot/pkg/types"
)

// CommandResult represents the complete result of a dodot command execution,
// formatted for display purposes. This is the top-level structure that
// encompasses all packs and their operations.
type CommandResult struct {
	// Command is the command that was executed (deploy, install, etc.)
	Command string

	// Packs contains the results for each pack, preserving pack grouping
	Packs []PackResult

	// Summary provides overall statistics
	Summary Summary

	// DryRun indicates if this was a dry run
	DryRun bool

	// Duration is the total execution time
	Duration time.Duration
}

// PackResult represents the execution results for a single pack,
// grouping all operations within that pack for cohesive display
type PackResult struct {
	// Name is the pack name
	Name string

	// Description is an optional pack description
	Description string

	// Files contains all file operations in this pack
	Files []FileResult

	// Status is the aggregated status for this pack
	Status types.ExecutionStatus

	// Statistics for this pack
	TotalOperations     int
	CompletedOperations int
	FailedOperations    int
	SkippedOperations   int
}

// FileResult represents a single file operation result,
// containing all information needed for the three-column display
type FileResult struct {
	// Column 1: Action verb based on PowerUp type
	Action string

	// Column 2: File path (relative when possible)
	Path string

	// Column 3: Status/outcome
	Status      types.OperationStatus
	Message     string
	IsNewChange bool // Indicates if this is a new change in this run

	// Additional context preserved from the operation
	PowerUp     string
	Pack        string
	GroupID     string
	Error       error
	Output      string // For command execution results
	LastApplied time.Time

	// Metadata from status checking or operation execution
	Metadata map[string]interface{}
}

// Summary provides overall command execution statistics
type Summary struct {
	// Timing information
	StartTime time.Time
	EndTime   time.Time
	Duration  time.Duration

	// Overall counts
	TotalPacks          int
	TotalOperations     int
	CompletedOperations int
	FailedOperations    int
	SkippedOperations   int

	// Status breakdown by pack
	SuccessfulPacks int
	PartialPacks    int
	FailedPacks     int
	SkippedPacks    int
}

// GetOverallStatus returns the overall command status based on pack results
func (cr *CommandResult) GetOverallStatus() types.ExecutionStatus {
	if cr.Summary.FailedOperations == cr.Summary.TotalOperations && cr.Summary.TotalOperations > 0 {
		return types.ExecutionStatusError
	}
	if cr.Summary.SkippedOperations == cr.Summary.TotalOperations && cr.Summary.TotalOperations > 0 {
		return types.ExecutionStatusSkipped
	}
	if cr.Summary.FailedOperations > 0 {
		return types.ExecutionStatusPartial
	}
	if cr.Summary.CompletedOperations > 0 {
		return types.ExecutionStatusSuccess
	}
	return types.ExecutionStatusPending
}

// GroupFilesByPowerUp groups file results by their PowerUp for organized display
func (pr *PackResult) GroupFilesByPowerUp() map[string][]FileResult {
	groups := make(map[string][]FileResult)
	for _, file := range pr.Files {
		powerUp := file.PowerUp
		if powerUp == "" {
			powerUp = "unknown"
		}
		groups[powerUp] = append(groups[powerUp], file)
	}
	return groups
}

// IsSuccess returns true if the file operation was successful
func (fr *FileResult) IsSuccess() bool {
	return fr.Status == types.StatusReady || fr.Status == types.StatusSkipped
}

// HasOutput returns true if the file result has command output to display
func (fr *FileResult) HasOutput() bool {
	return fr.Output != ""
}
