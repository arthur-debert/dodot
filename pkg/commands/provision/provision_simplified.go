package provision

import (
	"github.com/arthur-debert/dodot/pkg/commands/internal"
	"github.com/arthur-debert/dodot/pkg/types"
)

// ProvisionPacksSimplified runs the provisioning logic using the simplified rule-based system
// This is an experimental function to test the new system
func ProvisionPacksSimplified(opts ProvisionPacksOptions) (*types.ExecutionContext, error) {
	return internal.RunSimplifiedPipeline(internal.PipelineOptions{
		DotfilesRoot:       opts.DotfilesRoot,
		PackNames:          opts.PackNames,
		DryRun:             opts.DryRun,
		CommandMode:        internal.CommandModeAll, // Run all handler types
		Force:              opts.Force,
		EnableHomeSymlinks: true,
		UseSimplifiedRules: true,
	})
}
