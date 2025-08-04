package synthfs

import (
	"fmt"
	"strings"

	"github.com/arthur-debert/synthfs/pkg/synthfs/core"
)

// OperationError provides rich context about operation failures
type OperationError struct {
	Op      string                 // Operation type (e.g., "create-dir", "write-file")
	ID      core.OperationID       // Operation ID
	Path    string                 // Primary path being operated on
	Action  string                 // Human-readable action (e.g., "create directory", "write file")
	Err     error                  // Underlying error
	Context map[string]interface{} // Additional context
}

// Error returns a formatted error message
func (e *OperationError) Error() string {
	// Format: "failed to <action> '<path>': <reason> (operation: <type>-<id>)"
	msg := fmt.Sprintf("failed to %s '%s': %v (operation: %s)",
		e.Action, e.Path, e.Err, e.ID)

	// Add context if available
	if len(e.Context) > 0 {
		var contextParts []string
		for k, v := range e.Context {
			contextParts = append(contextParts, fmt.Sprintf("%s=%v", k, v))
		}
		msg += fmt.Sprintf(" [%s]", strings.Join(contextParts, ", "))
	}

	return msg
}

// Unwrap returns the underlying error
func (e *OperationError) Unwrap() error {
	return e.Err
}

// WithContext adds additional context to the error
func (e *OperationError) WithContext(key string, value interface{}) *OperationError {
	if e.Context == nil {
		e.Context = make(map[string]interface{})
	}
	e.Context[key] = value
	return e
}

// PipelineError represents a failure during pipeline execution
type PipelineError struct {
	FailedOp      Operation          // The operation that failed
	FailedIndex   int                // Index of failed operation (1-based)
	TotalOps      int                // Total operations in pipeline
	Err           error              // The actual error
	SuccessfulOps []core.OperationID // IDs of operations that succeeded before failure
}

// Error returns a formatted pipeline error message
func (e *PipelineError) Error() string {
	msg := fmt.Sprintf("Pipeline execution failed at operation %d of %d:\n  %v",
		e.FailedIndex, e.TotalOps, e.Err)

	if len(e.SuccessfulOps) > 0 {
		msg += "\n\nPrevious successful operations:"
		for i, id := range e.SuccessfulOps {
			msg += fmt.Sprintf("\n  %d. %s", i+1, id)
		}
	}

	return msg
}

// Unwrap returns the underlying error
func (e *PipelineError) Unwrap() error {
	return e.Err
}

// RollbackError represents failures during rollback
type RollbackError struct {
	OriginalErr  error                      // The original operation error
	RollbackErrs map[core.OperationID]error // Rollback errors by operation ID
}

// Error returns a formatted rollback error message
func (e *RollbackError) Error() string {
	msg := fmt.Sprintf("Operation failed: %v", e.OriginalErr)

	if len(e.RollbackErrs) > 0 {
		msg += "\n\nRollback also failed:"
		for id, err := range e.RollbackErrs {
			msg += fmt.Sprintf("\n  - %s: %v", id, err)
		}
	}

	return msg
}

// Unwrap returns the original error
func (e *RollbackError) Unwrap() error {
	return e.OriginalErr
}

// WrapOperationError wraps an error with operation context
func WrapOperationError(op Operation, action string, err error) error {
	if err == nil {
		return nil
	}

	// If it's already an OperationError, add more context
	if opErr, ok := err.(*OperationError); ok {
		return opErr
	}

	desc := op.Describe()
	return &OperationError{
		Op:     desc.Type,
		ID:     op.ID(),
		Path:   desc.Path,
		Action: action,
		Err:    err,
	}
}

// getOperationAction returns a human-readable action for an operation type
func getOperationAction(opType string) string {
	switch opType {
	case "create_file":
		return "create file"
	case "create_directory":
		return "create directory"
	case "create_symlink":
		return "create symlink"
	case "delete":
		return "delete"
	case "copy":
		return "copy"
	case "move":
		return "move"
	case "write_file":
		return "write file"
	case "mkdir":
		return "create directory"
	default:
		return opType
	}
}
