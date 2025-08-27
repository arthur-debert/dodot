// Package text provides plain text output without any styling
package text

import (
	"fmt"
	"io"

	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/arthur-debert/dodot/pkg/ui/display"
)

// Renderer provides plain text output without colors or styling
type Renderer struct {
	output         io.Writer
	legacyRenderer *display.TextRenderer
}

// New creates a new text renderer
func New(output io.Writer) (*Renderer, error) {
	return &Renderer{
		output:         output,
		legacyRenderer: display.NewTextRenderer(output),
	}, nil
}

// RenderResult renders any result type as plain text
func (r *Renderer) RenderResult(result interface{}) error {
	// For now, delegate to the legacy renderer based on type
	switch v := result.(type) {
	case *types.CommandResult:
		// Render the optional message first
		if v.Message != "" {
			if _, err := fmt.Fprintln(r.output, v.Message); err != nil {
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
		// Convert to DisplayResult first
		displayResult := v.ToDisplayResult()
		return r.legacyRenderer.Render(displayResult)
	case *types.DisplayResult:
		return r.legacyRenderer.Render(v)
	default:
		// For unknown types, just print them
		_, err := fmt.Fprintf(r.output, "%+v\n", result)
		return err
	}
}

// RenderError renders an error as plain text
func (r *Renderer) RenderError(err error) error {
	_, err2 := fmt.Fprintf(r.output, "Error: %v\n", err)
	return err2
}

// RenderMessage renders a simple message
func (r *Renderer) RenderMessage(msg string) error {
	_, err := fmt.Fprintln(r.output, msg)
	return err
}
