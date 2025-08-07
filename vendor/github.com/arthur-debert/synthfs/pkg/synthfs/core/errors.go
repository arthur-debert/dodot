package core

import "fmt"

// ValidationError represents an error during operation validation.
// This is moved from the main package to break circular dependencies.
type ValidationError struct {
	OperationID   OperationID
	OperationDesc OperationDesc
	Reason        string
	Cause         error
}

func (e *ValidationError) Error() string {
	if e.Cause != nil {
		return fmt.Sprintf("validation error for operation %s (%s): %s: %v",
			e.OperationID, e.OperationDesc.Path, e.Reason, e.Cause)
	}
	return fmt.Sprintf("validation error for operation %s (%s): %s",
		e.OperationID, e.OperationDesc.Path, e.Reason)
}

func (e *ValidationError) Unwrap() error {
	return e.Cause
}

// RollbackError represents an error that occurs during a rollback,
// wrapping the original execution error.
type RollbackError struct {
	OriginalErr  error
	RollbackErrs []error
}

func (e *RollbackError) Error() string {
	return fmt.Sprintf("operation failed with rollback errors: original error: %v, rollback errors: %v", e.OriginalErr, e.RollbackErrs)
}

func (e *RollbackError) Unwrap() error {
	return e.OriginalErr
}
