package internal

import (
	"github.com/arthur-debert/dodot/pkg/types"
)

// CommandMode represents which types of handlers should be executed
type CommandMode string

const (
	// CommandModeConfiguration runs only configuration handlers (symlinks, shell, path)
	CommandModeConfiguration CommandMode = "configuration"
	// CommandModeAll runs all handlers (both configuration and code execution)
	CommandModeAll CommandMode = "all"
)

// PipelineOptions contains options for running the execution pipeline
type PipelineOptions struct {
	DotfilesRoot       string
	PackNames          []string
	DryRun             bool
	CommandMode        CommandMode // Which types of handlers to execute
	Force              bool
	EnableHomeSymlinks bool
	UseSimplifiedRules bool // Use new rule-based system instead of matchers
}

// RunPipeline executes the core pipeline: GetPacks -> GetTriggers -> GetActions -> Execute
// Phase 3: Always uses the operation-based pipeline
func RunPipeline(opts PipelineOptions) (*types.ExecutionContext, error) {
	// Phase 3: Operations are now the default
	return RunPipelineWithOperations(opts)
}

// getCommandFromMode converts a CommandMode to a command string
func getCommandFromMode(mode CommandMode) string {
	switch mode {
	case CommandModeConfiguration:
		return "link"
	case CommandModeAll:
		return "link" // provision is separate
	default:
		return "link"
	}
}
