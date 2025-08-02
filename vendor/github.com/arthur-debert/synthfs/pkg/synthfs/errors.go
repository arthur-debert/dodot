package synthfs

import (
	"fmt"
)

// --- Error Types ---

// DependencyError represents an error with operation dependencies.
type DependencyError struct {
	Operation    Operation
	Dependencies []OperationID
	Missing      []OperationID
}

func (e *DependencyError) Error() string {
	return fmt.Sprintf("dependency error for operation %s: missing dependencies %v (required: %v)",
		e.Operation.ID(), e.Missing, e.Dependencies)
}

// ConflictError represents an error when operations conflict with each other.
type ConflictError struct {
	Operation Operation
	Conflicts []OperationID
}

func (e *ConflictError) Error() string {
	return fmt.Sprintf("conflict error for operation %s: conflicts with operations %v",
		e.Operation.ID(), e.Conflicts)
}
