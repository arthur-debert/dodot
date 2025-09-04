package context

import (
	"time"

	"github.com/arthur-debert/dodot/pkg/execution"
	"github.com/arthur-debert/dodot/pkg/types"
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
	Pack *types.Pack

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

// GetPackResult retrieves a pack result by name
func (ec *ExecutionContext) GetPackResult(packName string) (*PackExecutionResult, bool) {
	result, ok := ec.PackResults[packName]
	return result, ok
}
