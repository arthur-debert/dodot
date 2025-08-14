package output

import (
	"bytes"
	"embed"
	"fmt"
	"io"
	"os"
	"text/template"

	"github.com/arthur-debert/dodot/pkg/lipbalm"
	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/output/styles"
	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/charmbracelet/lipgloss"
)

//go:embed templates/*.tmpl
var templatesFS embed.FS

// Renderer handles the rendering of CLI output using lipbalm and templates
type Renderer struct {
	templates *template.Template
	writer    io.Writer
	noColor   bool
}

// NewRenderer creates a new Renderer instance
func NewRenderer(w io.Writer, noColor bool) (*Renderer, error) {
	log := logging.GetLogger("output.Renderer")

	// Log color detection info
	envNoColor := os.Getenv("NO_COLOR")
	log.Debug().
		Bool("noColor", noColor).
		Str("NO_COLOR_env", envNoColor).
		Str("TERM", os.Getenv("TERM")).
		Msg("Creating renderer with color settings")

	// Set up lipbalm renderer for proper color detection
	if !noColor {
		// Create a lipgloss renderer for the output writer
		renderer := lipgloss.NewRenderer(w)
		lipbalm.SetDefaultRenderer(renderer)

		profile := renderer.ColorProfile()
		log.Debug().
			Str("colorProfile", fmt.Sprintf("%v", profile)).
			Msg("Lipgloss renderer created")
	} else {
		log.Debug().Msg("No color mode - skipping lipgloss renderer setup")
	}

	// Parse all template files
	tmpl, err := template.ParseFS(templatesFS, "templates/*.tmpl")
	if err != nil {
		return nil, fmt.Errorf("failed to parse templates: %w", err)
	}

	return &Renderer{
		templates: tmpl,
		writer:    w,
		noColor:   noColor,
	}, nil
}

// Render renders a DisplayResult using the appropriate template
func (r *Renderer) Render(result *types.DisplayResult) error {
	log := logging.GetLogger("output.Renderer")

	// Execute the main result template
	var buf bytes.Buffer
	if err := r.templates.ExecuteTemplate(&buf, "result.tmpl", result); err != nil {
		return fmt.Errorf("failed to execute template: %w", err)
	}

	templateOutput := buf.String()
	log.Trace().
		Str("templateOutput", templateOutput).
		Msg("Template executed")

	// Apply styling through lipbalm
	var output string
	var err error
	if r.noColor {
		// Strip all style tags for no-color mode
		output = lipbalm.StripTags(templateOutput)
		log.Debug().Msg("Stripped tags for no-color mode")
	} else {
		// Apply lipgloss styles
		output, err = lipbalm.ExpandTags(templateOutput, styles.StyleRegistry)
		if err != nil {
			return fmt.Errorf("failed to expand tags: %w", err)
		}
		log.Debug().
			Bool("hasANSI", output != templateOutput).
			Msg("Expanded tags with lipgloss styles")
	}

	// Write to output
	_, writeErr := fmt.Fprintln(r.writer, output)
	return writeErr
}

// RenderExecutionContext is a convenience method that transforms ExecutionContext and renders it
func (r *Renderer) RenderExecutionContext(ctx *types.ExecutionContext) error {
	result := ctx.ToDisplayResult()
	return r.Render(result)
}

// RenderError renders an error message with appropriate styling
func (r *Renderer) RenderError(err error) error {
	tmpl := `<Error>Error:</Error> {{.Error}}`

	var buf bytes.Buffer
	t := template.Must(template.New("error").Parse(tmpl))
	if execErr := t.Execute(&buf, map[string]string{"Error": err.Error()}); execErr != nil {
		return execErr
	}

	var output string
	var expandErr error
	if r.noColor {
		output = lipbalm.StripTags(buf.String())
	} else {
		output, expandErr = lipbalm.ExpandTags(buf.String(), styles.StyleRegistry)
		if expandErr != nil {
			return expandErr
		}
	}

	_, writeErr := fmt.Fprintln(r.writer, output)
	return writeErr
}

// RenderMessage renders a simple message with optional styling
func (r *Renderer) RenderMessage(style, message string) error {
	tmpl := fmt.Sprintf(`<%s>%s</%s>`, style, message, style)

	var output string
	var err error
	if r.noColor {
		output = message
	} else {
		output, err = lipbalm.ExpandTags(tmpl, styles.StyleRegistry)
		if err != nil {
			return err
		}
	}

	_, writeErr := fmt.Fprintln(r.writer, output)
	return writeErr
}
