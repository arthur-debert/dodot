package internal

import (
	"github.com/arthur-debert/dodot/pkg/types"
)

// RunSimplifiedPipeline runs the pipeline using the new simplified rule system
// This creates a copy of PipelineOptions with SimplifiedRules set to true
func RunSimplifiedPipeline(opts PipelineOptions) (*types.ExecutionContext, error) {
	simplifiedOpts := opts
	simplifiedOpts.UseSimplifiedRules = true
	return RunPipeline(simplifiedOpts)
}
