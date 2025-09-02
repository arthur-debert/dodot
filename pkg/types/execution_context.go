package types

import (
	"time"

	"github.com/arthur-debert/dodot/pkg/execution"
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

	// Messages contains any user-facing messages to display after execution
	Messages []string
}

// PackExecutionResult contains the execution results for a single pack
type PackExecutionResult struct {
	// Pack is the pack being executed
	Pack *Pack

	// HandlerResults contains all Handler results and their status
	HandlerResults []*HandlerResult

	// Status is the aggregated status for this pack
	Status execution.ExecutionStatus

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
// This is the atomic unit - if ANY operation in a Handler fails, the Handler fails
type HandlerResult struct {
	// HandlerName is the name of the Handler (symlink, homebrew, etc.)
	HandlerName string

	// Files are the files processed by this Handler
	Files []string

	// Status is the final status after execution
	Status execution.OperationStatus

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
		Status:         execution.ExecutionStatusPending,
		StartTime:      time.Now(),
	}
}

// AddHandlerResult adds a Handler result and updates statistics
func (per *PackExecutionResult) AddHandlerResult(result *HandlerResult) {
	per.HandlerResults = append(per.HandlerResults, result)
	per.TotalHandlers++

	switch result.Status {
	case execution.StatusReady:
		per.CompletedHandlers++
	case execution.StatusSkipped:
		per.SkippedHandlers++
	case execution.StatusError, execution.StatusConflict:
		per.FailedHandlers++
	}

	// Update pack status
	per.updateStatus()
}

// updateStatus recalculates the pack's aggregated status
func (per *PackExecutionResult) updateStatus() {
	if per.TotalHandlers == 0 {
		per.Status = execution.ExecutionStatusPending
		return
	}

	if per.FailedHandlers == per.TotalHandlers {
		per.Status = execution.ExecutionStatusError
	} else if per.SkippedHandlers == per.TotalHandlers {
		per.Status = execution.ExecutionStatusSkipped
	} else if per.FailedHandlers > 0 {
		per.Status = execution.ExecutionStatusPartial
	} else {
		per.Status = execution.ExecutionStatusSuccess
	}
}

// Complete marks the pack execution as complete
func (per *PackExecutionResult) Complete() {
	per.EndTime = time.Now()
	per.updateStatus()
}
