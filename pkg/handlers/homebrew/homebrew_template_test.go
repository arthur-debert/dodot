package homebrew

import (
	"strings"
	"testing"

	"github.com/stretchr/testify/assert"
)

func TestHomebrewHandler_GetTemplateContent(t *testing.T) {
	handler := NewHomebrewHandler()

	content := handler.GetTemplateContent()

	// Template should not be empty
	assert.NotEmpty(t, content)

	// Should contain key brewfile elements
	assert.Contains(t, content, "Homebrew dependencies")
	assert.Contains(t, content, "PACK_NAME")
	assert.Contains(t, content, "dodot install")
	assert.Contains(t, content, "brew \"")
	assert.Contains(t, content, "cask \"")

	// Should be valid Ruby syntax (basic check)
	assert.Contains(t, content, "#") // Comments

	// Should not have trailing newlines
	assert.Equal(t, strings.TrimSpace(content), content)
}
