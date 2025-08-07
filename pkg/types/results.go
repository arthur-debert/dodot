package types

import "time"

// OperationStatus defines the state of an operation/action execution
type OperationStatus string

const (
	StatusReady    OperationStatus = "ready"
	StatusSkipped  OperationStatus = "skipped"
	StatusConflict OperationStatus = "conflict"
	StatusError    OperationStatus = "error"
	StatusUnknown  OperationStatus = "unknown"
)

// ListPacksResult holds the result of the 'list' command.
type ListPacksResult struct {
	Packs []PackInfo `json:"packs"`
}

// PackInfo contains summary information about a single pack.
type PackInfo struct {
	Name string `json:"name"`
	Path string `json:"path"`
}

// DisplayResult is the top-level structure for commands that produce rich output.
// Used by status, deploy, and install commands for consistent display formatting.
type DisplayResult struct {
	Command   string        `json:"command"` // "status", "deploy", "install"
	Packs     []DisplayPack `json:"packs"`
	DryRun    bool          `json:"dryRun"` // For deploy/install commands
	Timestamp time.Time     `json:"timestamp"`
}

// DisplayPack represents a single pack for display.
type DisplayPack struct {
	Name      string        `json:"name"`
	Status    string        `json:"status"` // Aggregated: "alert", "success", "queue"
	Files     []DisplayFile `json:"files"`
	HasConfig bool          `json:"hasConfig"` // Pack has .dodot.toml
	IsIgnored bool          `json:"isIgnored"` // Pack has .dodotignore
}

// DisplayFile represents a single file within a pack for display.
type DisplayFile struct {
	PowerUp      string     `json:"powerUp"`
	Path         string     `json:"path"`
	Status       string     `json:"status"` // File-level: "success", "error", "queue", "config", "ignored"
	Message      string     `json:"message"`
	IsOverride   bool       `json:"isOverride"`   // File power-up was overridden in .dodot.toml
	LastExecuted *time.Time `json:"lastExecuted"` // When operation was last executed
}

// GetPackStatus determines the pack-level status based on its files.
// Following the aggregation rules from the design:
// - If ANY file has ERROR status → Pack status is "alert"
// - If ALL files have SUCCESS status → Pack status is "success"
// - Empty pack or mixed states → Pack status is "queue"
func (dp *DisplayPack) GetPackStatus() string {
	if len(dp.Files) == 0 {
		return "queue"
	}

	hasError := false
	allSuccess := true

	for _, file := range dp.Files {
		// Skip config files in status calculation
		if file.Status == "config" {
			continue
		}

		if file.Status == "error" {
			hasError = true
		}
		if file.Status != "success" {
			allSuccess = false
		}
	}

	if hasError {
		return "alert" // Will be displayed with ALERT styling
	}
	if allSuccess {
		return "success"
	}
	return "queue"
}

// FillResult holds the result of the 'fill' command.
type FillResult struct {
	PackName     string   `json:"packName"`
	FilesCreated []string `json:"filesCreated"`
	// Operations field removed - part of Operation layer elimination
}

// ActionResult represents the execution result of a single Action.
// Contains timing, status, and error information for action execution tracking.
type ActionResult struct {
	// Action contains the action that was executed
	Action Action `json:"action"`

	// Status is the execution status
	Status OperationStatus `json:"status"`

	// Error contains any error that occurred during execution
	Error error `json:"error,omitempty"`

	// StartTime is when execution began
	StartTime time.Time `json:"startTime"`

	// EndTime is when execution completed
	EndTime time.Time `json:"endTime"`

	// Message provides additional context about the execution
	Message string `json:"message,omitempty"`

	// SynthfsOperationIDs tracks the synthfs operations that were executed for this action
	// This is useful for debugging and correlation with synthfs results
	SynthfsOperationIDs []string `json:"synthfsOperationIds,omitempty"`
}

// Duration returns the time taken to execute the action
func (ar *ActionResult) Duration() time.Duration {
	return ar.EndTime.Sub(ar.StartTime)
}

// InitResult holds the result of the 'init' command.
type InitResult struct {
	PackName     string   `json:"packName"`
	Path         string   `json:"path"`
	FilesCreated []string `json:"filesCreated"`
	// Operations field removed - part of Operation layer elimination
}
