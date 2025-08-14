/*
Package lipbalm provides a simple template engine for rich terminal rendering.

It offers two main functions:
  - `Render`: Processes a string with Go's text/template engine and then expands
    XML-like tags into styled terminal output.
  - `ExpandTags`: Skips the Go template processing and only expands the XML-like
    tags into styled output.

Usage with Go templating:

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

Usage for tag expansion only:

	styles := lipbalm.StyleMap{ "title": lipgloss.NewStyle().Bold(true) }
	input := `<title>Hello, World!</title>`
	output, err := lipbalm.ExpandTags(input, styles)
	fmt.Println(output)

Tags:

Tags are used to apply styles. The tag name must correspond to a key in the
StyleMap passed to the Render or ExpandTags function.

	<my-style>This text will be styled.</my-style>

<no-format> Tag:

The <no-format> tag is a special tag that is only rendered when the terminal
does not support color. This is useful for providing fallbacks for styled
content.

	<status>Status</status><no-format> ✓</no-format>

In the example above, the "✓" will only be rendered in plain text mode.
*/
package lipbalm
