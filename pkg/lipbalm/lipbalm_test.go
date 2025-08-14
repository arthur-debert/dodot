package lipbalm_test

import (
	"bytes"
	"io"
	"testing"

	"github.com/charmbracelet/lipgloss"
	"github.com/muesli/termenv"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	"github.com/arthur-debert/dodot/pkg/lipbalm"
)

var testStyles = lipbalm.StyleMap{
	"title": lipgloss.NewStyle().Bold(true),
	"date":  lipgloss.NewStyle().Foreground(lipgloss.Color("240")),
	"body":  lipgloss.NewStyle().Italic(true),
}

func TestRender(t *testing.T) {
	// a buffer to render to
	var buf bytes.Buffer
	renderer := lipgloss.NewRenderer(&buf)
	lipbalm.SetDefaultRenderer(renderer)

	t.Run("go template expansion", func(t *testing.T) {
		renderer.SetColorProfile(termenv.TrueColor)
		template := `<title>{{.Title}}</title>`
		data := struct{ Title string }{Title: "My Title"}
		expected := testStyles["title"].Render("My Title")
		actual, err := lipbalm.Render(template, data, testStyles)
		require.NoError(t, err)
		assert.Equal(t, expected, actual)
	})

	t.Run("invalid go template", func(t *testing.T) {
		renderer.SetColorProfile(termenv.TrueColor)
		template := `<title>{{.Title</title>`
		_, err := lipbalm.Render(template, nil, testStyles)
		assert.Error(t, err)
	})
}

func TestExpandTags(t *testing.T) {
	// a buffer to render to
	var buf bytes.Buffer
	renderer := lipgloss.NewRenderer(&buf)
	lipbalm.SetDefaultRenderer(renderer)

	t.Run("simple styled tag", func(t *testing.T) {
		renderer.SetColorProfile(termenv.TrueColor)
		template := `<title>Hello</title>`
		expected := testStyles["title"].Render("Hello")
		actual, err := lipbalm.ExpandTags(template, testStyles)
		require.NoError(t, err)
		assert.Equal(t, expected, actual)
	})

	t.Run("nested tags", func(t *testing.T) {
		renderer.SetColorProfile(termenv.TrueColor)
		template := `<title>Hello <date>World</date></title>`
		expected := testStyles["title"].Render("Hello " + testStyles["date"].Render("World"))
		actual, err := lipbalm.ExpandTags(template, testStyles)
		require.NoError(t, err)
		assert.Equal(t, expected, actual)
	})

	t.Run("no-format tag", func(t *testing.T) {
		t.Run("with color", func(t *testing.T) {
			renderer.SetColorProfile(termenv.TrueColor)
			template := `<title>Status</title><no-format> ✓</no-format>`
			expected := testStyles["title"].Render("Status")
			actual, err := lipbalm.ExpandTags(template, testStyles)
			require.NoError(t, err)
			assert.Equal(t, expected, actual)
		})

		t.Run("without color", func(t *testing.T) {
			renderer.SetColorProfile(termenv.Ascii)
			template := `<title>Status</title><no-format> ✓</no-format>`
			expected := "Status ✓"
			actual, err := lipbalm.ExpandTags(template, testStyles)
			require.NoError(t, err)
			assert.Equal(t, expected, actual)
		})
	})

	t.Run("no lipbalm tags", func(t *testing.T) {
		renderer.SetColorProfile(termenv.TrueColor)
		template := `Just some text.`
		expected := `Just some text.`
		actual, err := lipbalm.ExpandTags(template, testStyles)
		require.NoError(t, err)
		assert.Equal(t, expected, actual)
	})

	t.Run("unclosed lipbalm tag", func(t *testing.T) {
		renderer.SetColorProfile(termenv.TrueColor)
		template := `<title>Hello`
		expected := `<title>Hello`
		actual, err := lipbalm.ExpandTags(template, testStyles)
		require.NoError(t, err)
		assert.Equal(t, expected, actual)
	})
}

func TestMain(m *testing.M) {
	// set a dummy renderer for all tests
	lipgloss.SetDefaultRenderer(lipgloss.NewRenderer(io.Discard))
	m.Run()
}
