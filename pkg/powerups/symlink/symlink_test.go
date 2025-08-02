package symlink

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/arthur-debert/dodot/pkg/registry"
	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestSymlinkPowerUp_BasicFunctionality(t *testing.T) {
	powerUp := NewSymlinkPowerUp()

	// Test basic properties
	assert.Equal(t, SymlinkPowerUpName, powerUp.Name())
	assert.Equal(t, "Creates symbolic links from dotfiles to target locations", powerUp.Description())

	// Test with no matches
	actions, err := powerUp.Process([]types.TriggerMatch{})
	require.NoError(t, err)
	assert.Empty(t, actions)
}

func TestSymlinkPowerUp_ProcessMatches(t *testing.T) {
	homeDir := t.TempDir()
	oldHome := os.Getenv("HOME")
	require.NoError(t, os.Setenv("HOME", homeDir))
	defer func() {
		if oldHome != "" {
			require.NoError(t, os.Setenv("HOME", oldHome))
		} else {
			require.NoError(t, os.Unsetenv("HOME"))
		}
	}()

	powerUp := NewSymlinkPowerUp()

	matches := []types.TriggerMatch{
		{
			TriggerName:  "filename",
			Pack:         "vim",
			Path:         ".vimrc",
			AbsolutePath: "/dotfiles/vim/.vimrc",
			PowerUpName:  "symlink",
		},
		{
			TriggerName:  "filename",
			Pack:         "bash",
			Path:         ".bashrc",
			AbsolutePath: "/dotfiles/bash/.bashrc",
			PowerUpName:  "symlink",
		},
	}

	actions, err := powerUp.Process(matches)
	require.NoError(t, err)
	require.Len(t, actions, 2)

	// Check first action
	assert.Equal(t, types.ActionTypeLink, actions[0].Type)
	assert.Equal(t, "Symlink .vimrc -> "+filepath.Join(homeDir, ".vimrc"), actions[0].Description)
	assert.Equal(t, "/dotfiles/vim/.vimrc", actions[0].Source)
	assert.Equal(t, filepath.Join(homeDir, ".vimrc"), actions[0].Target)
	assert.Equal(t, "vim", actions[0].Pack)
	assert.Equal(t, SymlinkPowerUpName, actions[0].PowerUpName)
	assert.Equal(t, SymlinkPowerUpPriority, actions[0].Priority)

	// Check second action
	assert.Equal(t, types.ActionTypeLink, actions[1].Type)
	assert.Equal(t, "/dotfiles/bash/.bashrc", actions[1].Source)
	assert.Equal(t, filepath.Join(homeDir, ".bashrc"), actions[1].Target)
	assert.Equal(t, "bash", actions[1].Pack)
}

func TestSymlinkPowerUp_CustomTarget(t *testing.T) {
	powerUp := NewSymlinkPowerUp()
	customTarget := "/custom/target"

	matches := []types.TriggerMatch{
		{
			TriggerName:  "filename",
			Pack:         "config",
			Path:         "app.conf",
			AbsolutePath: "/dotfiles/config/app.conf",
			PowerUpName:  "symlink",
			PowerUpOptions: map[string]interface{}{
				"target": customTarget,
			},
		},
	}

	actions, err := powerUp.Process(matches)
	require.NoError(t, err)
	require.Len(t, actions, 1)

	assert.Equal(t, filepath.Join(customTarget, "app.conf"), actions[0].Target)
}

func TestSymlinkPowerUp_ConflictDetection(t *testing.T) {
	powerUp := NewSymlinkPowerUp()

	// Two different files want to symlink to the same target
	matches := []types.TriggerMatch{
		{
			TriggerName:  "filename",
			Pack:         "pack1",
			Path:         ".config",
			AbsolutePath: "/dotfiles/pack1/.config",
			PowerUpName:  "symlink",
		},
		{
			TriggerName:  "filename",
			Pack:         "pack2",
			Path:         ".config", // Same filename
			AbsolutePath: "/dotfiles/pack2/.config",
			PowerUpName:  "symlink",
		},
	}

	actions, err := powerUp.Process(matches)
	assert.Error(t, err)
	assert.Nil(t, actions)
	assert.Contains(t, err.Error(), "symlink conflict")
	assert.Contains(t, err.Error(), "/dotfiles/pack1/.config")
	assert.Contains(t, err.Error(), "/dotfiles/pack2/.config")
}

func TestSymlinkPowerUp_EnvironmentExpansion(t *testing.T) {
	oldCustomDir := os.Getenv("CUSTOM_DIR")
	require.NoError(t, os.Setenv("CUSTOM_DIR", "/expanded/path"))
	defer func() {
		if oldCustomDir != "" {
			require.NoError(t, os.Setenv("CUSTOM_DIR", oldCustomDir))
		} else {
			require.NoError(t, os.Unsetenv("CUSTOM_DIR"))
		}
	}()

	powerUp := NewSymlinkPowerUp()

	matches := []types.TriggerMatch{
		{
			TriggerName:  "filename",
			Pack:         "test",
			Path:         "file.txt",
			AbsolutePath: "/dotfiles/test/file.txt",
			PowerUpName:  "symlink",
			PowerUpOptions: map[string]interface{}{
				"target": "$CUSTOM_DIR/configs",
			},
		},
	}

	actions, err := powerUp.Process(matches)
	require.NoError(t, err)
	require.Len(t, actions, 1)

	assert.Equal(t, "/expanded/path/configs/file.txt", actions[0].Target)
}

func TestSymlinkPowerUp_ValidateOptions(t *testing.T) {
	powerUp := NewSymlinkPowerUp()

	tests := []struct {
		name    string
		options map[string]interface{}
		wantErr bool
		errMsg  string
	}{
		{
			name:    "nil options",
			options: nil,
			wantErr: false,
		},
		{
			name:    "empty options",
			options: map[string]interface{}{},
			wantErr: false,
		},
		{
			name: "valid target option",
			options: map[string]interface{}{
				"target": "/some/path",
			},
			wantErr: false,
		},
		{
			name: "invalid target type",
			options: map[string]interface{}{
				"target": 123,
			},
			wantErr: true,
			errMsg:  "target option must be a string",
		},
		{
			name: "unknown option",
			options: map[string]interface{}{
				"unknown": "value",
			},
			wantErr: true,
			errMsg:  "unknown option: unknown",
		},
		{
			name: "mixed valid and invalid",
			options: map[string]interface{}{
				"target":  "/path",
				"invalid": "option",
			},
			wantErr: true,
			errMsg:  "unknown option: invalid",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			err := powerUp.ValidateOptions(tt.options)
			if tt.wantErr {
				assert.Error(t, err)
				assert.Contains(t, err.Error(), tt.errMsg)
			} else {
				assert.NoError(t, err)
			}
		})
	}
}

func TestSymlinkPowerUp_MetadataInActions(t *testing.T) {
	powerUp := NewSymlinkPowerUp()

	matches := []types.TriggerMatch{
		{
			TriggerName:  "glob",
			Pack:         "configs",
			Path:         "config.yml",
			AbsolutePath: "/dotfiles/configs/config.yml",
			PowerUpName:  "symlink",
			Metadata: map[string]interface{}{
				"pattern": "*.yml",
			},
		},
	}

	actions, err := powerUp.Process(matches)
	require.NoError(t, err)
	require.Len(t, actions, 1)

	// Check that trigger name is preserved in action metadata
	assert.Equal(t, "glob", actions[0].Metadata["trigger"])
}

func TestSymlinkPowerUp_FactoryRegistration(t *testing.T) {
	// Test that the factory is registered
	factory, err := registry.GetPowerUpFactory(SymlinkPowerUpName)
	require.NoError(t, err)
	require.NotNil(t, factory)

	// Test factory creates power-up correctly
	powerUp, err := factory(nil)
	require.NoError(t, err)
	require.NotNil(t, powerUp)

	assert.Equal(t, SymlinkPowerUpName, powerUp.Name())
}
