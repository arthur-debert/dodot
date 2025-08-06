package types

import "time"

// DisplayResult represents the complete result of a command for display purposes.
// This is the unified structure used by status, deploy, install, and other commands
// to provide consistent output aligned with the display design.
type DisplayResult struct {
	// Command that was executed (e.g., "status", "deploy", "install")
	Command string `json:"command"`

	// Packs contains the display information for each pack
	Packs []DisplayPack `json:"packs"`

	// DryRun indicates if this was a dry run (for deploy/install)
	DryRun bool `json:"dryRun"`

	// Timestamp when the command was executed
	Timestamp time.Time `json:"timestamp"`
}

// DisplayPack represents a single pack's display information.
// This is the pack-level container in the "pack → file → status" hierarchy.
type DisplayPack struct {
	// Name of the pack
	Name string `json:"name"`

	// Status aggregated from all files in the pack
	Status DisplayStatus `json:"status"`

	// Files contains all files in this pack with their status
	Files []DisplayFile `json:"files"`

	// HasConfig indicates if this pack has a .dodot.toml file
	HasConfig bool `json:"hasConfig"`

	// IsIgnored indicates if this pack contains .dodotignore
	IsIgnored bool `json:"isIgnored"`
}

// DisplayFile represents a single file's display information.
// This is the file-centric model that shows: powerup : filepath : status
type DisplayFile struct {
	// Path relative to pack (e.g., "vimrc", "config/init.vim")
	Path string `json:"path"`

	// PowerUp that handles this file (e.g., "symlink", "homebrew")
	PowerUp string `json:"powerUp"`

	// Status of this file
	Status DisplayStatus `json:"status"`

	// Message providing details about the status
	Message string `json:"message"`

	// IsOverride indicates if this file's power-up was overridden in .dodot.toml
	IsOverride bool `json:"isOverride"`

	// LastExecuted for tracking when operations were performed
	LastExecuted *time.Time `json:"lastExecuted,omitempty"`
}

// DisplayStatus represents the status of a file or pack for display purposes
type DisplayStatus string

const (
	// DisplayStatusSuccess - File has been deployed/installed successfully
	DisplayStatusSuccess DisplayStatus = "success"

	// DisplayStatusError - Deploy/install attempt has failed
	DisplayStatusError DisplayStatus = "error"

	// DisplayStatusQueue - File is pending deployment/installation
	DisplayStatusQueue DisplayStatus = "queue"

	// DisplayStatusIgnored - Directory is explicitly ignored via .dodotignore
	DisplayStatusIgnored DisplayStatus = "ignored"

	// DisplayStatusConfig - Configuration file indicator
	DisplayStatusConfig DisplayStatus = "config"
)

// GetPackStatus determines the pack-level status based on its files
// Following the aggregation rules from the design:
// - If ANY file has ERROR status → Pack status is ERROR (displayed as ALERT)
// - If ALL files have SUCCESS status → Pack status is SUCCESS
// - If ALL files have QUEUE status → Pack status is QUEUE
// - Mixed SUCCESS and QUEUE → Pack status is QUEUE (conservative default)
// - Empty pack (no files) → Pack status is QUEUE
func (dp *DisplayPack) GetPackStatus() DisplayStatus {
	if len(dp.Files) == 0 {
		return DisplayStatusQueue
	}

	hasError := false
	allSuccess := true

	for _, file := range dp.Files {
		// Skip config files in status calculation
		if file.Status == DisplayStatusConfig {
			continue
		}

		if file.Status == DisplayStatusError {
			hasError = true
		}
		if file.Status != DisplayStatusSuccess {
			allSuccess = false
		}
	}

	// Priority order: Error > Queue > Success
	if hasError {
		return DisplayStatusError // Will be displayed with ALERT styling
	}
	if allSuccess {
		return DisplayStatusSuccess
	}
	// Default to queue for mixed states or all queue
	return DisplayStatusQueue
}
