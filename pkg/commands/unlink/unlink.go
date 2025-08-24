package unlink

import ()

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
//
// This command undoes the effects of linking handlers (symlink, path, shell_profile)
// but deliberately leaves provisioning handlers (provision, homebrew) untouched.
//
// This implementation uses the clearable infrastructure to ensure consistency
// with other clear operations.
func UnlinkPacks(opts UnlinkPacksOptions) (*UnlinkResult, error) {
	// Use the new clearable-based implementation
	v2Opts := UnlinkPacksV2Options{
		DotfilesRoot: opts.DotfilesRoot,
		PackNames:    opts.PackNames,
		Force:        opts.Force,
		DryRun:       opts.DryRun,
	}
	return UnlinkPacksV2(v2Opts)
}
