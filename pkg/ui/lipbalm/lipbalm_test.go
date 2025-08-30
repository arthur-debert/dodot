package lipbalm_test

import (
	"bytes"
	"io"
	"testing"

	"github.com/arthur-debert/dodot/pkg/ui/lipbalm"
	"github.com/charmbracelet/lipgloss"
	"github.com/muesli/termenv"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestMain(m *testing.M) {
	// Set a dummy renderer for all tests to ensure consistent behavior
	lipgloss.SetDefaultRenderer(lipgloss.NewRenderer(io.Discard))
	m.Run()
}

func TestRender(t *testing.T) {
	// Create test styles
	testStyles := lipbalm.StyleMap{
		"title": lipgloss.NewStyle().Bold(true),
		"date":  lipgloss.NewStyle().Foreground(lipgloss.Color("240")),
		"body":  lipgloss.NewStyle().Italic(true),
	}

	// Create a buffer renderer for consistent testing
	var buf bytes.Buffer
	renderer := lipgloss.NewRenderer(&buf)
	lipbalm.SetDefaultRenderer(renderer)

	t.Run("go template expansion with styling", func(t *testing.T) {
		renderer.SetColorProfile(termenv.TrueColor)

		template := `<title>{{.Title}}</title>`
		data := struct{ Title string }{Title: "My Title"}

		result, err := lipbalm.Render(template, data, testStyles)
		require.NoError(t, err)

		// Should expand template and apply styles
		expected := testStyles["title"].Render("My Title")
		assert.Equal(t, expected, result)
	})

	t.Run("multiple template variables", func(t *testing.T) {
		renderer.SetColorProfile(termenv.TrueColor)

		template := `<title>{{.Title}}</title> by <date>{{.Author}}</date>`
		data := struct {
			Title  string
			Author string
		}{
			Title:  "Article",
			Author: "John Doe",
		}

		result, err := lipbalm.Render(template, data, testStyles)
		require.NoError(t, err)

		expected := testStyles["title"].Render("Article") + " by " + testStyles["date"].Render("John Doe")
		assert.Equal(t, expected, result)
	})

	t.Run("invalid go template syntax", func(t *testing.T) {
		template := `<title>{{.Title</title>`
		_, err := lipbalm.Render(template, nil, testStyles)
		assert.Error(t, err)
		assert.Contains(t, err.Error(), "template")
	})

	t.Run("template execution error", func(t *testing.T) {
		template := `<title>{{.NonExistentField}}</title>`
		data := struct{ Title string }{Title: "Test"}
		_, err := lipbalm.Render(template, data, testStyles)
		assert.Error(t, err)
	})
}

func TestExpandTags(t *testing.T) {
	// Create test styles
	testStyles := lipbalm.StyleMap{
		"title":   lipgloss.NewStyle().Bold(true),
		"date":    lipgloss.NewStyle().Foreground(lipgloss.Color("240")),
		"body":    lipgloss.NewStyle().Italic(true),
		"success": lipgloss.NewStyle().Foreground(lipgloss.Color("green")),
		"error":   lipgloss.NewStyle().Foreground(lipgloss.Color("red")),
	}

	// Create a buffer renderer for consistent testing
	var buf bytes.Buffer
	renderer := lipgloss.NewRenderer(&buf)
	lipbalm.SetDefaultRenderer(renderer)

	t.Run("simple styled tag", func(t *testing.T) {
		renderer.SetColorProfile(termenv.TrueColor)

		template := `<title>Hello World</title>`
		result, err := lipbalm.ExpandTags(template, testStyles)
		require.NoError(t, err)

		expected := testStyles["title"].Render("Hello World")
		assert.Equal(t, expected, result)
	})

	t.Run("multiple styled tags", func(t *testing.T) {
		renderer.SetColorProfile(termenv.TrueColor)

		template := `<title>Title</title> and <body>Body</body>`
		result, err := lipbalm.ExpandTags(template, testStyles)
		require.NoError(t, err)

		expected := testStyles["title"].Render("Title") + " and " + testStyles["body"].Render("Body")
		assert.Equal(t, expected, result)
	})

	t.Run("nested tags", func(t *testing.T) {
		renderer.SetColorProfile(termenv.TrueColor)

		template := `<title>Hello <date>2024</date></title>`
		result, err := lipbalm.ExpandTags(template, testStyles)
		require.NoError(t, err)

		expected := testStyles["title"].Render("Hello " + testStyles["date"].Render("2024"))
		assert.Equal(t, expected, result)
	})

	t.Run("unknown tag ignored", func(t *testing.T) {
		renderer.SetColorProfile(termenv.TrueColor)

		template := `<unknown>Text</unknown>`
		result, err := lipbalm.ExpandTags(template, testStyles)
		require.NoError(t, err)
		assert.Equal(t, "Text", result)
	})

	t.Run("no-format tag with color enabled", func(t *testing.T) {
		renderer.SetColorProfile(termenv.TrueColor)

		template := `<title>Status</title><no-format> ✓</no-format>`
		result, err := lipbalm.ExpandTags(template, testStyles)
		require.NoError(t, err)

		// no-format content should be excluded when color is enabled
		expected := testStyles["title"].Render("Status")
		assert.Equal(t, expected, result)
	})

	t.Run("no-format tag with color disabled", func(t *testing.T) {
		renderer.SetColorProfile(termenv.Ascii)

		template := `<title>Status</title><no-format> ✓</no-format>`
		result, err := lipbalm.ExpandTags(template, testStyles)
		require.NoError(t, err)

		// no-format content should be included when color is disabled
		assert.Equal(t, "Status ✓", result)
	})

	t.Run("plain text without tags", func(t *testing.T) {
		renderer.SetColorProfile(termenv.TrueColor)

		template := `Just plain text without any tags.`
		result, err := lipbalm.ExpandTags(template, testStyles)
		require.NoError(t, err)
		assert.Equal(t, template, result)
	})

	t.Run("invalid XML returns original", func(t *testing.T) {
		renderer.SetColorProfile(termenv.TrueColor)

		template := `<title>Unclosed tag`
		result, err := lipbalm.ExpandTags(template, testStyles)
		require.NoError(t, err)
		assert.Equal(t, template, result)
	})

	t.Run("empty string", func(t *testing.T) {
		result, err := lipbalm.ExpandTags("", testStyles)
		require.NoError(t, err)
		assert.Equal(t, "", result)
	})

	t.Run("styles applied only with color support", func(t *testing.T) {
		// Test with color disabled
		renderer.SetColorProfile(termenv.Ascii)

		template := `<title>Hello</title> <success>OK</success>`
		result, err := lipbalm.ExpandTags(template, testStyles)
		require.NoError(t, err)
		assert.Equal(t, "Hello OK", result)
	})
}

func TestStripTags(t *testing.T) {
	tests := []struct {
		name     string
		input    string
		expected string
	}{
		{
			name:     "strips simple tags",
			input:    "<Bold>Hello</Bold> <Italic>World</Italic>",
			expected: "Hello World",
		},
		{
			name:     "strips nested tags",
			input:    "<Header><Bold>Title</Bold> <Italic>Subtitle</Italic></Header>",
			expected: "Title Subtitle",
		},
		{
			name:     "preserves plain text",
			input:    "Plain text without any tags",
			expected: "Plain text without any tags",
		},
		{
			name:     "handles empty tags",
			input:    "<Empty></Empty>Text",
			expected: "Text",
		},
		{
			name:     "preserves newlines",
			input:    "<Line1>First</Line1>\n<Line2>Second</Line2>",
			expected: "First\nSecond",
		},
		{
			name:     "strips no-format tags",
			input:    "<Bold>Styled</Bold> <no-format>Plain</no-format>",
			expected: "Styled Plain",
		},
		{
			name:     "handles self-closing tags",
			input:    "Before<br/>After",
			expected: "BeforeAfter",
		},
		{
			name:     "handles mixed content",
			input:    "Start <tag1>middle</tag1> end",
			expected: "Start middle end",
		},
		{
			name:     "handles invalid XML gracefully",
			input:    "Not <valid XML",
			expected: "Not <valid XML",
		},
		{
			name:     "empty string",
			input:    "",
			expected: "",
		},
		{
			name:     "deeply nested tags",
			input:    "<a><b><c><d>Deep</d></c></b></a>",
			expected: "Deep",
		},
		{
			name:     "tags with spaces in content",
			input:    "<tag>  spaced  content  </tag>",
			expected: "  spaced  content  ",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result := lipbalm.StripTags(tt.input)
			assert.Equal(t, tt.expected, result)
		})
	}
}

func TestEdgeCases(t *testing.T) {
	testStyles := lipbalm.StyleMap{
		"test": lipgloss.NewStyle().Bold(true),
	}

	var buf bytes.Buffer
	renderer := lipgloss.NewRenderer(&buf)
	lipbalm.SetDefaultRenderer(renderer)

	t.Run("nil data in template", func(t *testing.T) {
		template := `<test>Static content</test>`
		result, err := lipbalm.Render(template, nil, testStyles)
		require.NoError(t, err)
		assert.NotEmpty(t, result)
	})

	t.Run("empty style map", func(t *testing.T) {
		template := `<unknown>Text</unknown>`
		result, err := lipbalm.ExpandTags(template, lipbalm.StyleMap{})
		require.NoError(t, err)
		assert.Equal(t, "Text", result)
	})

	t.Run("special characters in content", func(t *testing.T) {
		renderer.SetColorProfile(termenv.TrueColor)

		// Special characters break XML parsing, so it returns the original
		template := `<test>Special: & < > " '</test>`
		result, err := lipbalm.ExpandTags(template, testStyles)
		require.NoError(t, err)

		// When XML parsing fails due to special chars, original string is returned
		assert.Equal(t, template, result)
	})

	t.Run("escaped special characters work", func(t *testing.T) {
		renderer.SetColorProfile(termenv.TrueColor)

		template := `<test>Special: &amp; &lt; &gt;</test>`
		result, err := lipbalm.ExpandTags(template, testStyles)
		require.NoError(t, err)

		expected := testStyles["test"].Render("Special: & < >")
		assert.Equal(t, expected, result)
	})
}
