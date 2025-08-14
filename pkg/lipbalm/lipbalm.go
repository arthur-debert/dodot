package lipbalm

import (
	"bytes"
	"io"
	"os"
	"text/template"

	"github.com/beevik/etree"
	"github.com/charmbracelet/lipgloss"
	"github.com/muesli/termenv"
)

// StyleMap defines a map of tag names to lipgloss styles.
type StyleMap map[string]lipgloss.Style

var defaultRenderer = lipgloss.NewRenderer(os.Stdout)

// SetDefaultRenderer sets the default renderer for the package.
// This is useful for testing.
func SetDefaultRenderer(renderer *lipgloss.Renderer) {
	defaultRenderer = renderer
}

// Render processes a template string, first with Go's text/template engine,
// and then applies lipgloss styles based on the XML-like tags.
func Render(templateString string, data interface{}, styles StyleMap) (string, error) {
	// Phase 1: Go template expansion
	tmpl, err := template.New("lipbalm").Parse(templateString)
	if err != nil {
		return "", err
	}

	var buf bytes.Buffer
	if err := tmpl.Execute(&buf, data); err != nil {
		return "", err
	}
	expandedTemplate := buf.String()

	// Phase 2: Lipbaml expansion
	return ExpandTags(expandedTemplate, styles)
}

// ExpandTags applies lipgloss styles based on the XML-like tags.
func ExpandTags(s string, styles StyleMap) (string, error) {
	hasColor := defaultRenderer.ColorProfile() != termenv.Ascii

	doc := etree.NewDocument()
	// We need a single root element for the XML parser.
	if err := doc.ReadFromString("<root>" + s + "</root>"); err != nil {
		// If parsing fails, it might be because the template is not valid XML.
		// In this case, we'll return the raw expanded template.
		// This is a fallback for templates that don't use lipbalm tags.
		return s, nil
	}

	var result bytes.Buffer
	root := doc.SelectElement("root")
	for _, token := range root.Child {
		processToken(token, &result, styles, hasColor)
	}

	return result.String(), nil
}

func processToken(token etree.Token, w io.Writer, styles StyleMap, hasColor bool) {
	switch t := token.(type) {
	case *etree.Element:
		if t.Tag == "no-format" {
			if !hasColor {
				for _, child := range t.Child {
					processToken(child, w, styles, hasColor)
				}
			}
			return
		}

		style, styleExists := styles[t.Tag]

		if hasColor && styleExists {
			var innerContent bytes.Buffer
			for _, child := range t.Child {
				processToken(child, &innerContent, styles, hasColor)
			}
			_, _ = w.Write([]byte(style.Render(innerContent.String())))
		} else {
			for _, child := range t.Child {
				processToken(child, w, styles, hasColor)
			}
		}
	case *etree.CharData:
		_, _ = w.Write([]byte(t.Data))
	}
}
