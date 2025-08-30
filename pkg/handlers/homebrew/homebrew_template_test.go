package homebrew

import (
	"strings"
	"testing"
)

func TestHomebrewHandler_GetTemplateContent(t *testing.T) {
	handler := NewHandler()

	content := handler.GetTemplateContent()

	// Template should not be empty
	if content == "" {
		t.Error("GetTemplateContent() returned empty string")
	}

	// Should contain key brewfile elements
	expectedContents := []string{
		"Homebrew dependencies",
		"PACK_NAME",
		"dodot install",
		"brew '",
		"cask '",
	}

	// Check each expected content
	for _, expected := range expectedContents {
		if !strings.Contains(content, expected) {
			t.Errorf("Template should contain %q", expected)
		}
	}

	// Should be valid Ruby syntax (basic check)
	if !strings.Contains(content, "#") {
		t.Error("Template should contain comments (starting with #)")
	}

	// Should not have trailing newlines
	trimmed := strings.TrimSpace(content)
	if trimmed != content {
		t.Error("Template should not have leading or trailing whitespace")
	}
}
