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
