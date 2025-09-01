package topics

import (
	"github.com/charmbracelet/glamour"
)

// GlamourRenderer uses the glamour library for rich markdown rendering
type GlamourRenderer struct {
	Style string // Style name: "dark", "light", "notty", "auto", or path to custom style
	Width int    // Terminal width (0 = auto-detect)
}

// NewGlamourRenderer creates a markdown renderer using glamour with auto-detection
func NewGlamourRenderer() *GlamourRenderer {
	return &GlamourRenderer{
		Style: "auto", // Auto-detect based on terminal
		Width: 0,      // Auto-detect width
	}
}

// Render converts markdown to beautiful terminal output
func (r *GlamourRenderer) Render(content string, format string) string {
	// Only process markdown files
	if format != ".md" {
		return content
	}

	// Configure glamour options
	var options []glamour.TermRendererOption

	// Set style
	if r.Style != "" && r.Style != "auto" {
		options = append(options, glamour.WithStylePath(r.Style))
	} else {
		// Use auto style detection
		options = append(options, glamour.WithAutoStyle())
	}

	// Set width if specified
	if r.Width > 0 {
		options = append(options, glamour.WithWordWrap(r.Width))
	}

	// Create renderer
	renderer, err := glamour.NewTermRenderer(options...)
	if err != nil {
		// Fallback to plain text on error
		return content
	}

	// Render markdown
	rendered, err := renderer.Render(content)
	if err != nil {
		// Fallback to plain text on error
		return content
	}

	return rendered
}
