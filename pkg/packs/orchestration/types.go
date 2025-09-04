// Package orchestration provides orchestration for executing commands across multiple packs.
// It owns the outer loop: discover packs → execute command → aggregate results.
package orchestration

import (
	"github.com/arthur-debert/dodot/pkg/types"
)

// Command defines the interface for pack-level commands.
// Each command implements how to process a single pack.
type Command interface {
	// ExecuteForPack executes the command for a single pack.
	// This is called by the pack pipeline for each discovered pack.
	ExecuteForPack(pack types.Pack, opts Options) (*PackResult, error)

	// Name returns the command name for logging and identification
	Name() string
}

// Options contains execution options for the pack pipeline.
type Options struct {
	// DotfilesRoot is the path to the root of the dotfiles directory
	DotfilesRoot string

	// DryRun specifies whether to perform a dry run without making changes
	DryRun bool

	// FileSystem to use (optional, defaults to OS filesystem)
	FileSystem types.FS

	// Additional command-specific options can be embedded
	CommandOptions interface{}
}

// PackResult contains the execution result for a single pack.
type PackResult struct {
	// Pack that was processed
	Pack types.Pack

	// Success indicates if the command succeeded for this pack
	Success bool

	// Error if the command failed
	Error error

	// CommandSpecificResult allows commands to return additional data
	CommandSpecificResult interface{}
}

// Result contains the aggregated results from executing a command across multiple packs.
type Result struct {
	// Command that was executed
	Command string

	// TotalPacks is the number of packs discovered
	TotalPacks int

	// SuccessfulPacks is the number of packs successfully processed
	SuccessfulPacks int

	// FailedPacks is the number of packs that failed
	FailedPacks int

	// PackResults contains individual results for each pack
	PackResults []PackResult

	// Error if the entire pipeline failed
	Error error
}
