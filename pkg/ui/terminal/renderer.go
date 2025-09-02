// Package terminal provides rich terminal output with colors and styling
package terminal

import (
	"fmt"
	"io"

	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/arthur-debert/dodot/pkg/ui/display"
	"github.com/arthur-debert/dodot/pkg/ui/output"
)

// Renderer provides rich terminal output using templates and styling
type Renderer struct {
	output         io.Writer
	legacyRenderer *output.Renderer
}

// New creates a new terminal renderer
func New(w io.Writer) (*Renderer, error) {
	// Create the legacy renderer for now
	legacy, err := output.NewRenderer(w, false)
	if err != nil {
		return nil, fmt.Errorf("failed to create legacy renderer: %w", err)
	}

	return &Renderer{
		output:         w,
		legacyRenderer: legacy,
	}, nil
}

// RenderResult renders any result type with rich terminal formatting
func (r *Renderer) RenderResult(result interface{}) error {
	// For now, delegate to the legacy renderer based on type
	switch v := result.(type) {
	case *display.PackCommandResult:
		// Convert PackCommandResult to DisplayResult for rendering
		displayResult := &display.DisplayResult{
			Command:   v.Command,
			Packs:     v.Packs,
			DryRun:    v.DryRun,
			Timestamp: v.Timestamp,
		}

		// Render the optional message first
		if v.Message != "" {
			if err := r.legacyRenderer.RenderMessage("Info", v.Message); err != nil {
				return err
			}
			// Add a blank line between message and pack status
			if _, err := fmt.Fprintln(r.output); err != nil {
				return err
			}
		}
		// Then render the pack status
		return r.legacyRenderer.Render(displayResult)
	case *display.CommandResult:
		// Legacy CommandResult support
		// Render the optional message first
		if v.Message != "" {
			if err := r.legacyRenderer.RenderMessage("Info", v.Message); err != nil {
				return err
			}
			// Add a blank line between message and pack status
			if _, err := fmt.Fprintln(r.output); err != nil {
				return err
			}
		}
		// Then render the pack status
		if v.Result != nil {
			return r.legacyRenderer.Render(v.Result)
		}
		return nil
	case *types.ExecutionContext:
		return r.legacyRenderer.RenderExecutionContext(v)
	case *display.DisplayResult:
		return r.legacyRenderer.Render(v)
	default:
		// For unknown types, just print them
		_, err := fmt.Fprintf(r.output, "%+v\n", result)
		return err
	}
}

// RenderError renders an error with appropriate formatting
func (r *Renderer) RenderError(err error) error {
	return r.legacyRenderer.RenderError(err)
}

// RenderMessage renders a simple message
func (r *Renderer) RenderMessage(msg string) error {
	return r.legacyRenderer.RenderMessage("Info", msg)
}
