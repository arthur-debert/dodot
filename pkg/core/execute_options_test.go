package core

import (
	"testing"

	"github.com/arthur-debert/dodot/pkg/handlers"
	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/stretchr/testify/assert"
)

func TestFilterOutProvisioningHandlers(t *testing.T) {
	matches := []types.RuleMatch{
		{HandlerName: "symlink", Pack: "vim"},
		{HandlerName: "homebrew", Pack: "vim"},
		{HandlerName: "shell", Pack: "vim"},
		{HandlerName: "install", Pack: "vim"},
		{HandlerName: "path", Pack: "vim"},
	}

	filtered := filterOutProvisioningHandlers(matches)

	// Should only have configuration handlers (symlink, shell, path)
	assert.Len(t, filtered, 3)

	for _, match := range filtered {
		assert.True(t, handlers.HandlerRegistry.IsConfigurationHandler(match.HandlerName),
			"handler %s should be a configuration handler", match.HandlerName)
	}
}

func TestBuildProvisioningSkipMessage(t *testing.T) {
	tests := []struct {
		name     string
		packs    []string
		expected string
	}{
		{
			name:     "single pack",
			packs:    []string{"vim"},
			expected: "Pack vim has already been provisioned. To re-run provisioning, use --provision-rerun",
		},
		{
			name:     "multiple packs",
			packs:    []string{"vim", "tmux", "git"},
			expected: "Packs vim, tmux, git have already been provisioned. To re-run provisioning, use --provision-rerun",
		},
		{
			name:     "empty list",
			packs:    []string{},
			expected: "",
		},
		{
			name:     "nil list",
			packs:    nil,
			expected: "",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result := buildProvisioningSkipMessage(tt.packs)
			assert.Equal(t, tt.expected, result)
		})
	}
}
