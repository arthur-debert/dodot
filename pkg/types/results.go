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
type ExecutionResult struct {
	Packs      []string    `json:"packs"`
	Operations []Operation `json:"operations"`
	DryRun     bool        `json:"dryRun"`
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
