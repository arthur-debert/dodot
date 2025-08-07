package synthfs

import (
	"context"
)

// prevalidatedPipeline wraps a pipeline and marks it as already validated
type prevalidatedPipeline struct {
	Pipeline
	validated bool
}

// newPrevalidatedPipeline creates a pipeline wrapper that skips validation
func newPrevalidatedPipeline(p Pipeline) Pipeline {
	return &prevalidatedPipeline{
		Pipeline:  p,
		validated: true,
	}
}

// Validate returns nil since operations are already validated
func (p *prevalidatedPipeline) Validate(ctx context.Context, fs FileSystem) error {
	// Skip validation - operations were already validated with projected state
	return nil
}
