package homebrew

import (
	"strings"
	"testing"

	"github.com/arthur-debert/dodot/pkg/handlers/path"
	"github.com/arthur-debert/dodot/pkg/handlers/provision"
	"github.com/arthur-debert/dodot/pkg/handlers/shell_add_path"
	"github.com/arthur-debert/dodot/pkg/handlers/shell_profile"
	"github.com/arthur-debert/dodot/pkg/handlers/symlink"
	"github.com/arthur-debert/dodot/pkg/types"
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
	assert.Contains(t, content, "brew '")
	assert.Contains(t, content, "cask '")

	// Should be valid Ruby syntax (basic check)
	assert.Contains(t, content, "#") // Comments

	// Should not have trailing newlines
	assert.Equal(t, strings.TrimSpace(content), content)
}

func TestOtherHandlers_GetTemplateContent(t *testing.T) {
	tests := []struct {
		name            string
		handler         types.Handler
		hasTemplate     bool
		expectedContent []string // content that should be present if hasTemplate is true
	}{
		{"SymlinkHandler", symlink.NewSymlinkHandler(), false, nil},
		{"PathHandler", path.NewPathHandler(), false, nil},
		{"ProvisionScriptHandler", provision.NewProvisionScriptHandler(), true, []string{"#!/usr/bin/env bash", "dodot install", "PACK_NAME"}},
		{"ShellAddPathHandler", shell_add_path.NewShellAddPathHandler(), true, []string{"#!/usr/bin/env sh", "PATH modifications", "PACK_NAME"}},
		{"ShellProfileHandler", shell_profile.NewShellProfileHandler(), true, []string{"#!/usr/bin/env sh", "Shell aliases", "PACK_NAME"}},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			content := tt.handler.GetTemplateContent()

			if tt.hasTemplate {
				assert.NotEmpty(t, content, "%s should have template content", tt.name)
				for _, expected := range tt.expectedContent {
					assert.Contains(t, content, expected, "%s template should contain %q", tt.name, expected)
				}
			} else {
				assert.Empty(t, content, "%s should return empty template", tt.name)
			}
		})
	}
}
