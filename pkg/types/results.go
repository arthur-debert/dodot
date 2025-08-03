package types

// ListPacksResult holds the result of the 'list' command.
type ListPacksResult struct {
	Packs []PackInfo `json:"packs"`
}

// PackInfo contains summary information about a single pack.
type PackInfo struct {
	Name string `json:"name"`
	Path string `json:"path"`
}

// PackStatusResult holds the status of one or more packs.
type PackStatusResult struct {
	Packs []PackStatus `json:"packs"`
}

// PackStatus represents the detailed status of a single pack.
type PackStatus struct {
	Name         string          `json:"name"`
	PowerUpState []PowerUpStatus `json:"powerUpState"`
}

// PowerUpStatus describes the state of a single power-up within a pack.
type PowerUpStatus struct {
	Name        string `json:"name"`
	State       string `json:"state"` // e.g., "Installed", "Not Installed", "Changed"
	Description string `json:"description"`
}

// ExecutionResult holds the outcome of an 'install' or 'deploy' command.
// It details the operations that were/would be performed.
// IMPORTANT: This contains PLANNED operations, not executed ones.
// The operations must still be executed using an appropriate executor
// (e.g., CombinedExecutor for operations that include OperationExecute).
type ExecutionResult struct {
	Packs      []string    `json:"packs"`      // Packs that were processed
	Operations []Operation `json:"operations"` // Operations planned (not yet executed)
	DryRun     bool        `json:"dryRun"`     // Whether this was a dry run
}

// FillResult holds the result of the 'fill' command.
type FillResult struct {
	PackName     string      `json:"packName"`
	FilesCreated []string    `json:"filesCreated"`
	Operations   []Operation `json:"operations"`
}

// InitResult holds the result of the 'init' command.
type InitResult struct {
	PackName     string      `json:"packName"`
	Path         string      `json:"path"`
	FilesCreated []string    `json:"filesCreated"`
	Operations   []Operation `json:"operations"`
}
