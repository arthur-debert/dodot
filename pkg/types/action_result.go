package types

import (
	"time"
)

// ActionResult represents the outcome of executing an action
type ActionResult struct {
	// Action that was executed
	Action Action

	// Success indicates whether the action completed successfully
	Success bool

	// Error contains any error that occurred during execution
	Error error

	// Message provides additional information about the result
	Message string

	// Duration is how long the action took to execute
	Duration time.Duration

	// Skipped indicates if the action was skipped (e.g., already provisioned)
	Skipped bool
}
