package types

import "time"

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
// This replaces the old PackStatusResult and is used by status, deploy, and install commands.
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

// InitResult holds the result of the 'init' command.
type InitResult struct {
	PackName     string   `json:"packName"`
	Path         string   `json:"path"`
	FilesCreated []string `json:"filesCreated"`
	// Operations field removed - part of Operation layer elimination
}
