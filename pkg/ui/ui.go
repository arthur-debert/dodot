// Package ui provides a unified interface for rendering output in different formats.
// It supports terminal (rich), text (plain), and JSON output formats.
package ui

import (
	"fmt"
	"io"
	"os"

	"github.com/arthur-debert/dodot/pkg/ui/json"
	"github.com/arthur-debert/dodot/pkg/ui/terminal"
	"github.com/arthur-debert/dodot/pkg/ui/text"
)

// Renderer is the common interface for all output renderers.
// It provides methods for rendering different types of data and messages.
type Renderer interface {
	// RenderResult renders any result type (command results, execution contexts, etc.)
	RenderResult(result interface{}) error

	// RenderError renders an error with appropriate formatting
	RenderError(err error) error

	// RenderMessage renders a simple message
	RenderMessage(msg string) error
}

// NewRenderer creates a new renderer based on the specified format.
// It automatically detects terminal capabilities when format is Auto.
func NewRenderer(format Format, output io.Writer) (Renderer, error) {
	switch format {
	case FormatAuto:
		// Detect terminal capabilities and choose appropriate format
		if file, ok := output.(*os.File); ok {
			detectedFormat := DetectFormat(file)
			return NewRenderer(detectedFormat, output)
		}
		// If not a file, default to terminal format
		return NewRenderer(FormatTerminal, output)
	case FormatTerminal:
		return terminal.New(output)
	case FormatText:
		return text.New(output)
	case FormatJSON:
		return json.New(output)
	default:
		return nil, fmt.Errorf("unknown format: %v", format)
	}
}
