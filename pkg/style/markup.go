package style

import (
	"regexp"
	"strings"

	"github.com/charmbracelet/lipgloss"
)

// MarkupParser handles parsing and rendering of markup tags
type MarkupParser struct {
	styles map[string]lipgloss.Style
}

// NewMarkupParser creates a new markup parser with default styles
func NewMarkupParser() *MarkupParser {
	return &MarkupParser{
		styles: map[string]lipgloss.Style{
			"title":     TitleStyle,
			"subtitle":  SubtitleStyle,
			"success":   SuccessStyle,
			"error":     ErrorStyle,
			"warning":   WarningStyle,
			"info":      InfoStyle,
			"code":      CodeStyle,
			"path":      PathStyle,
			"muted":     MutedStyle,
			"bold":      lipgloss.NewStyle().Bold(true),
			"italic":    lipgloss.NewStyle().Italic(true),
			"underline": lipgloss.NewStyle().Underline(true),

			// PowerUp tags
			"symlink":        SymlinkStyle,
			"profile":        ProfileStyle,
			"install_script": InstallScriptStyle,
			"homebrew":       HomebrewStyle,
		},
	}
}

// Render processes markup text and returns styled output
func (p *MarkupParser) Render(text string) string {
	// We'll process tags in a loop to handle nested tags properly
	result := text
	changed := true

	// Keep processing until no more changes are made
	for changed {
		changed = false
		oldResult := result

		// Find and process each tag type
		for tag, style := range p.styles {
			// Create pattern for this specific tag
			pattern := regexp.MustCompile(`\[` + tag + `\](.*?)\[/` + tag + `\]`)

			result = pattern.ReplaceAllStringFunc(result, func(match string) string {
				// Extract content between tags
				submatch := pattern.FindStringSubmatch(match)
				if len(submatch) != 2 {
					return match
				}

				content := submatch[1]
				// Apply the style
				changed = true
				return style.Render(content)
			})
		}

		// If nothing changed, we're done
		if result == oldResult {
			break
		}
	}

	return result
}

// AddStyle allows adding custom styles
func (p *MarkupParser) AddStyle(tag string, style lipgloss.Style) {
	p.styles[tag] = style
}

// RenderTemplate renders a template with variable substitution and markup
func (p *MarkupParser) RenderTemplate(template string, vars map[string]string) string {
	// First, substitute variables
	result := template
	for key, value := range vars {
		placeholder := "{{" + key + "}}"
		result = strings.ReplaceAll(result, placeholder, value)
	}

	// Then render markup
	return p.Render(result)
}

// Global parser instance
var defaultParser = NewMarkupParser()

// Render is a convenience function using the default parser
func Render(text string) string {
	return defaultParser.Render(text)
}

// RenderTemplate is a convenience function using the default parser
func RenderTemplate(template string, vars map[string]string) string {
	return defaultParser.RenderTemplate(template, vars)
}
