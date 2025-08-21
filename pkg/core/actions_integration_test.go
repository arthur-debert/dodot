package core_test

import (
	"testing"

	"github.com/arthur-debert/dodot/pkg/core"
	"github.com/arthur-debert/dodot/pkg/registry"
	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/stretchr/testify/assert"

	// Import power-ups to ensure they're registered
	_ "github.com/arthur-debert/dodot/pkg/handlers/symlink"
)

func TestGetActions_CrossPackSymlinkConflict(t *testing.T) {
	// Ensure symlink power-up is registered
	_, err := registry.GetHandlerFactory("symlink")
	assert.NoError(t, err, "symlink power-up should be registered")

	// Create matches from two different packs that would create conflicting symlinks
	matches := []types.TriggerMatch{
		{
			TriggerName:  "filename",
			Pack:         "tool-1",
			Path:         "config.toml",
			AbsolutePath: "/dotfiles/tool-1/config.toml",
			HandlerName:  "symlink",
			Priority:     50,
		},
		{
			TriggerName:  "filename",
			Pack:         "tool-2",
			Path:         "config.toml", // Same filename - will conflict
			AbsolutePath: "/dotfiles/tool-2/config.toml",
			HandlerName:  "symlink",
			Priority:     50,
		},
	}

	// This should detect the conflict and return an error
	actions, err := core.GetActions(matches)

	// The test should fail initially because cross-pack conflict detection is not implemented
	assert.Error(t, err, "Expected error for cross-pack symlink conflict")
	assert.Nil(t, actions)
	if err != nil {
		assert.Contains(t, err.Error(), "conflict")
		assert.Contains(t, err.Error(), "tool-1")
		assert.Contains(t, err.Error(), "tool-2")
		assert.Contains(t, err.Error(), "config.toml")
	}
}

func TestGetActions_CrossPackSymlinkNoConflict(t *testing.T) {
	// Ensure symlink power-up is registered
	_, err := registry.GetHandlerFactory("symlink")
	assert.NoError(t, err, "symlink power-up should be registered")

	// Create matches from two different packs with different filenames - no conflict
	matches := []types.TriggerMatch{
		{
			TriggerName:  "filename",
			Pack:         "tool-1",
			Path:         "config1.toml",
			AbsolutePath: "/dotfiles/tool-1/config1.toml",
			HandlerName:  "symlink",
			Priority:     50,
		},
		{
			TriggerName:  "filename",
			Pack:         "tool-2",
			Path:         "config2.toml", // Different filename - no conflict
			AbsolutePath: "/dotfiles/tool-2/config2.toml",
			HandlerName:  "symlink",
			Priority:     50,
		},
	}

	// This should work without errors
	actions, err := core.GetActions(matches)
	assert.NoError(t, err)
	assert.NotNil(t, actions)
	assert.Len(t, actions, 2) // Two link actions
}

func TestGetActions_CrossPackSymlinkConflictWithNestedPaths(t *testing.T) {
	// Ensure symlink power-up is registered
	_, err := registry.GetHandlerFactory("symlink")
	assert.NoError(t, err, "symlink power-up should be registered")

	// Create matches that would NOT conflict due to different paths
	matches := []types.TriggerMatch{
		{
			TriggerName:  "filename",
			Pack:         "tool-1",
			Path:         ".config/app1/config.toml",
			AbsolutePath: "/dotfiles/tool-1/.config/app1/config.toml",
			HandlerName:  "symlink",
			Priority:     50,
		},
		{
			TriggerName:  "filename",
			Pack:         "tool-2",
			Path:         ".config/app2/config.toml", // Different path - no conflict
			AbsolutePath: "/dotfiles/tool-2/.config/app2/config.toml",
			HandlerName:  "symlink",
			Priority:     50,
		},
	}

	// This should work without errors
	actions, err := core.GetActions(matches)
	assert.NoError(t, err)
	assert.NotNil(t, actions)
	assert.Len(t, actions, 2) // Two link actions
}
