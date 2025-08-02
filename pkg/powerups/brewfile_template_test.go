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
	assert.Contains(t, content, "#")  // Comments
	
	// Should not have trailing newlines
	assert.Equal(t, strings.TrimSpace(content), content)
}

func TestOtherPowerUps_GetTemplateContent(t *testing.T) {
	tests := []struct {
		name     string
		powerup  types.PowerUp
		expected string
	}{
		{"SymlinkPowerUp", NewSymlinkPowerUp(), ""},
		{"BinPowerUp", NewBinPowerUp(), ""},
		{"InstallScriptPowerUp", NewInstallScriptPowerUp(), ""},
		{"ShellAddPathPowerUp", NewShellAddPathPowerUp(), ""},
		{"ShellProfilePowerUp", NewShellProfilePowerUp(), ""},
		{"TemplatePowerUp", NewTemplatePowerUp(), ""},
	}
	
	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			content := tt.powerup.GetTemplateContent()
			assert.Equal(t, tt.expected, content, "%s should return empty template", tt.name)
		})
	}
}