package types

import "time"

// StatusState represents the state of a deployment
type StatusState string

const (
	// StatusStatePending indicates the action has not been executed yet
	StatusStatePending StatusState = "pending"

	// StatusStateSuccess indicates the action was executed successfully
	StatusStateSuccess StatusState = "success"

	// StatusStateError indicates the action failed or is broken
	StatusStateError StatusState = "error"

	// StatusStateIgnored indicates the item is explicitly ignored
	StatusStateIgnored StatusState = "ignored"

	// StatusStateConfig indicates this is a configuration file
	StatusStateConfig StatusState = "config"
)

// Status represents the deployment status of an action
type Status struct {
	// State is the current status state
	State StatusState

	// Message is a human-readable status message
	Message string

	// Timestamp is when the action was last executed (optional)
	Timestamp *time.Time
}
