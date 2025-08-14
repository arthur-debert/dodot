/*
Package lipbalm provides a simple template engine for rich terminal rendering.

Lipbalm combines Go's text/template with lipgloss styling through XML-like tags,
enabling declarative terminal output that automatically adapts to terminal capabilities.

# Core Functions

The package offers three main functions:
  - Render: Processes Go templates then expands style tags
  - ExpandTags: Only expands style tags (no template processing)
  - StripTags: Removes all style tags for plain text output

# Usage with Go templating

	styles := lipbalm.StyleMap{
		"title": lipgloss.NewStyle().Bold(true),
		"date":  lipgloss.NewStyle().Foreground(lipgloss.Color("240")),
	}
	template := `<title>{{.Title}}</title> <date>{{.Date}}</date>`
	data := struct {
		Title string
		Date  string
	}{
		Title: "Hello, World!",
		Date:  "2025-08-15",
	}
	output, err := lipbalm.Render(template, data, styles)
	fmt.Println(output)

# Usage for tag expansion only

	styles := lipbalm.StyleMap{ "title": lipgloss.NewStyle().Bold(true) }
	input := `<title>Hello, World!</title>`
	output, err := lipbalm.ExpandTags(input, styles)
	fmt.Println(output)

# Plain text output

	input := `<title>Hello</title> <date>2025</date>`
	plain := lipbalm.StripTags(input)  // Returns: "Hello 2025"

# Tags

Tags are used to apply styles. The tag name must correspond to a key in the
StyleMap passed to the Render or ExpandTags function.

	<my-style>This text will be styled.</my-style>

# Special Tags

The <no-format> tag only renders when the terminal doesn't support color:

	<status>Status</status><no-format> ✓</no-format>

In the example above, the "✓" will only be rendered in plain text mode.

# Integration with dodot

In dodot's output pipeline, lipbalm is the final styling layer that converts
semantic tags from templates into terminal-appropriate output. It automatically:
  - Detects terminal color capabilities via termenv
  - Honors the NO_COLOR environment variable
  - Gracefully handles invalid XML by returning the original text

See pkg/output/doc.go for the complete rendering pipeline documentation.
*/
package lipbalm
