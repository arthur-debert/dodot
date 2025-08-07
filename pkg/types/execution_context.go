package types

import "time"

// ExecutionStatus represents the overall status of a pack's execution
type ExecutionStatus string

const (
	// ExecutionStatusSuccess means all operations succeeded
	ExecutionStatusSuccess ExecutionStatus = "success"

	// ExecutionStatusPartial means some operations succeeded, some failed
	ExecutionStatusPartial ExecutionStatus = "partial"

	// ExecutionStatusError means all operations failed
	ExecutionStatusError ExecutionStatus = "error"

	// ExecutionStatusSkipped means all operations were skipped
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

	// TotalOperations is the total count of operations across all packs
	TotalOperations int

	// CompletedOperations is the count of successfully completed operations
	CompletedOperations int

	// FailedOperations is the count of failed operations
	FailedOperations int

	// SkippedOperations is the count of skipped operations
	SkippedOperations int
}

// PackExecutionResult contains the execution results for a single pack
type PackExecutionResult struct {
	// Pack is the pack being executed
	Pack *Pack

	// Operations contains all operations and their results
	Operations []*OperationResult

	// Status is the aggregated status for this pack
	Status ExecutionStatus

	// StartTime is when this pack's execution began
	StartTime time.Time

	// EndTime is when this pack's execution completed
	EndTime time.Time

	// TotalOperations in this pack
	TotalOperations int

	// CompletedOperations in this pack
	CompletedOperations int

	// FailedOperations in this pack
	FailedOperations int

	// SkippedOperations in this pack
	SkippedOperations int
}

// FIXME: ARCHITECTURAL PROBLEM - We should NOT track individual operation results!
// The atomic unit is PowerUp, not Operation. Execution system should:
// 1. Execute all operations for a PowerUp
// 2. Roll up status: if ANY operation fails, PowerUp fails
// 3. Store PowerUpResult (not OperationResult)
// 4. UI displays PowerUp status: "vim: symlink .vimrc -> failed" (user-level info)
// NOT operation details: "CreateDir: success, CreateSymlink: failed" (implementation details)
// OperationResult tracks the result of a single operation execution
type OperationResult struct {
	// Operation that was executed
	Operation *Operation

	// Status is the final status after execution
	Status OperationStatus

	// Error contains any error that occurred
	Error error

	// StartTime is when the operation began
	StartTime time.Time

	// EndTime is when the operation completed
	EndTime time.Time

	// Output contains any output from the operation (for execute operations)
	Output string

	// Metadata can contain additional execution information
	Metadata map[string]interface{}
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

	// Update totals
	ec.TotalOperations = 0
	ec.CompletedOperations = 0
	ec.FailedOperations = 0
	ec.SkippedOperations = 0

	for _, pr := range ec.PackResults {
		ec.TotalOperations += pr.TotalOperations
		ec.CompletedOperations += pr.CompletedOperations
		ec.FailedOperations += pr.FailedOperations
		ec.SkippedOperations += pr.SkippedOperations
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
		Pack:       pack,
		Operations: make([]*OperationResult, 0),
		Status:     ExecutionStatusPending,
		StartTime:  time.Now(),
	}
}

// AddOperationResult adds an operation result and updates statistics
func (per *PackExecutionResult) AddOperationResult(result *OperationResult) {
	per.Operations = append(per.Operations, result)
	per.TotalOperations++

	switch result.Status {
	case StatusReady:
		per.CompletedOperations++
	case StatusSkipped:
		per.SkippedOperations++
	case StatusError, StatusConflict:
		per.FailedOperations++
	}

	// Update pack status
	per.updateStatus()
}

// updateStatus recalculates the pack's aggregated status
func (per *PackExecutionResult) updateStatus() {
	if per.TotalOperations == 0 {
		per.Status = ExecutionStatusPending
		return
	}

	if per.FailedOperations == per.TotalOperations {
		per.Status = ExecutionStatusError
	} else if per.SkippedOperations == per.TotalOperations {
		per.Status = ExecutionStatusSkipped
	} else if per.FailedOperations > 0 {
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

// GroupOperationsByPowerUp groups operations by their PowerUp for display
func (per *PackExecutionResult) GroupOperationsByPowerUp() map[string][]*OperationResult {
	groups := make(map[string][]*OperationResult)

	for _, opResult := range per.Operations {
		powerUp := opResult.Operation.PowerUp
		if powerUp == "" {
			powerUp = "unknown"
		}
		groups[powerUp] = append(groups[powerUp], opResult)
	}

	return groups
}

// GroupOperationsByGroupID groups operations by their GroupID
func (per *PackExecutionResult) GroupOperationsByGroupID() map[string][]*OperationResult {
	groups := make(map[string][]*OperationResult)

	for _, opResult := range per.Operations {
		groupID := opResult.Operation.GroupID
		if groupID == "" {
			groupID = "ungrouped"
		}
		groups[groupID] = append(groups[groupID], opResult)
	}

	return groups
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
