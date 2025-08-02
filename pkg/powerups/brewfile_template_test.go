package powerups

import (
	"strings"
	"testing"

	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/stretchr/testify/assert"
)

func TestBrewfilePowerUp_GetTemplateContent(t *testing.T) {
	powerup := NewBrewfilePowerUp()

	content := powerup.GetTemplateContent()

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

func TestOtherPowerUps_GetTemplateContent(t *testing.T) {
	tests := []struct {
		name            string
		powerup         types.PowerUp
		hasTemplate     bool
		expectedContent []string // content that should be present if hasTemplate is true
	}{
		{"SymlinkPowerUp", NewSymlinkPowerUp(), false, nil},
		{"BinPowerUp", NewBinPowerUp(), false, nil},
		{"InstallScriptPowerUp", NewInstallScriptPowerUp(), true, []string{"#!/usr/bin/env bash", "dodot install", "PACK_NAME"}},
		{"ShellAddPathPowerUp", NewShellAddPathPowerUp(), true, []string{"#!/usr/bin/env sh", "PATH modifications", "PACK_NAME"}},
		{"ShellProfilePowerUp", NewShellProfilePowerUp(), true, []string{"#!/usr/bin/env sh", "Shell aliases", "PACK_NAME"}},
		{"TemplatePowerUp", NewTemplatePowerUp(), false, nil},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			content := tt.powerup.GetTemplateContent()

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
