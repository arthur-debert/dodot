package unlink

import (
	"fmt"
)

// UnlinkPacksOptions contains options for the off command
type UnlinkPacksOptions struct {
	// DotfilesRoot is the path to the dotfiles directory
	DotfilesRoot string

	// DataDir is the dodot data directory
	DataDir string

	// PackNames is the list of pack names to turn off (empty = all)
	PackNames []string

	// Force skips confirmation prompts
	Force bool

	// DryRun shows what would be removed without actually removing
	DryRun bool
}

// UnlinkResult contains the result of the off command
type UnlinkResult struct {
	// Packs that were processed
	Packs []PackUnlinkResult `json:"packs"`

	// Total number of items removed
	TotalRemoved int `json:"totalRemoved"`

	// Whether this was a dry run
	DryRun bool `json:"dryRun"`
}

// PackUnlinkResult contains the result for a single pack
type PackUnlinkResult struct {
	// Name of the pack
	Name string `json:"name"`

	// Items that were removed
	RemovedItems []RemovedItem `json:"removedItems"`

	// Any errors encountered
	Errors []string `json:"errors,omitempty"`
}

// RemovedItem represents a single removed deployment
type RemovedItem struct {
	// Type of item (symlink, path, shell_profile, etc.)
	Type string `json:"type"`

	// Path that was removed
	Path string `json:"path"`

	// Target it pointed to (for symlinks)
	Target string `json:"target,omitempty"`

	// Whether removal succeeded
	Success bool `json:"success"`

	// Error if removal failed
	Error string `json:"error,omitempty"`
}

// UnlinkPacks removes deployments for the specified packs
func UnlinkPacks(opts UnlinkPacksOptions) (*UnlinkResult, error) {
	// TODO: Update unlink command to work with V2 actions
	// Temporarily disabled while migrating to V2 system
	return nil, fmt.Errorf("unlink command temporarily disabled during V2 migration")
}
