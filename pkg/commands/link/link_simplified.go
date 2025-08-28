package link

import (
	"github.com/arthur-debert/dodot/pkg/commands/internal"
	"github.com/arthur-debert/dodot/pkg/types"
)

// LinkPacksSimplified runs the linking logic using the simplified rule-based system
// This is an experimental function to test the new system
func LinkPacksSimplified(opts LinkPacksOptions) (*types.ExecutionContext, error) {
	return internal.RunSimplifiedPipeline(internal.PipelineOptions{
		DotfilesRoot:       opts.DotfilesRoot,
		PackNames:          opts.PackNames,
		DryRun:             opts.DryRun,
		RunMode:            types.RunModeLinking,
		Force:              false,
		EnableHomeSymlinks: opts.EnableHomeSymlinks,
		UseSimplifiedRules: true,
	})
}