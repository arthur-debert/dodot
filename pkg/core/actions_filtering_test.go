package core_test

import (
	"testing"

	"github.com/arthur-debert/dodot/pkg/core"
	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

// Mock data store for testing
type mockDataStore struct {
	needs map[string]bool
	err   error
}

func (m *mockDataStore) NeedsProvisioning(pack, sentinel, checksum string) (bool, error) {
	if m.err != nil {
		return false, m.err
	}
	key := pack + ":" + sentinel
	return m.needs[key], nil
}

func (m *mockDataStore) SetProvisioned(pack, sentinel, checksum string) error {
	return nil
}

func (m *mockDataStore) GetProvisionedChecksum(pack, sentinel string) (string, error) {
	return "", nil
}

// Implement all required DataStore methods
func (m *mockDataStore) Link(pack, sourceFile string) (string, error)                 { return "", nil }
func (m *mockDataStore) Unlink(pack, sourceFile string) error                         { return nil }
func (m *mockDataStore) AddToPath(pack, dirPath string) error                         { return nil }
func (m *mockDataStore) AddToShellProfile(pack, scriptPath string) error              { return nil }
func (m *mockDataStore) RecordProvisioning(pack, sentinelName, checksum string) error { return nil }
func (m *mockDataStore) GetStatus(pack, sourceFile string) (types.Status, error) {
	return types.Status{}, nil
}
func (m *mockDataStore) GetSymlinkStatus(pack, sourceFile string) (types.Status, error) {
	return types.Status{}, nil
}
func (m *mockDataStore) GetPathStatus(pack, dirPath string) (types.Status, error) {
	return types.Status{}, nil
}
func (m *mockDataStore) GetShellProfileStatus(pack, scriptPath string) (types.Status, error) {
	return types.Status{}, nil
}
func (m *mockDataStore) GetProvisioningStatus(pack, sentinelName, currentChecksum string) (types.Status, error) {
	return types.Status{}, nil
}
func (m *mockDataStore) GetBrewStatus(pack, brewfilePath, currentChecksum string) (types.Status, error) {
	return types.Status{}, nil
}
func (m *mockDataStore) DeleteProvisioningState(packName, handlerName string) error { return nil }
func (m *mockDataStore) GetProvisioningHandlers(packName string) ([]string, error)  { return nil, nil }
func (m *mockDataStore) ListProvisioningState(packName string) (map[string][]string, error) {
	return nil, nil
}

// Tests for Action Filtering Logic

func TestFilterConfigurationActions(t *testing.T) {
	t.Run("filters out provisioning actions", func(t *testing.T) {
		// Setup
		linkAction := &types.LinkAction{
			PackName:   "vim",
			SourceFile: ".vimrc",
			TargetFile: "~/.vimrc",
		}
		scriptAction := &types.RunScriptAction{
			PackName:     "vim",
			ScriptPath:   "install.sh",
			SentinelName: "vim-installed",
		}
		brewAction := &types.BrewAction{
			PackName:     "tools",
			BrewfilePath: "Brewfile",
		}

		actions := []types.Action{linkAction, scriptAction, brewAction}

		// Execute
		filtered := core.FilterConfigurationActions(actions)

		// Verify - should only include link action
		assert.Len(t, filtered, 1)
		assert.IsType(t, &types.LinkAction{}, filtered[0])
	})

	t.Run("includes all linking actions", func(t *testing.T) {
		// Setup
		actions := []types.Action{
			&types.LinkAction{PackName: "vim", SourceFile: ".vimrc"},
			&types.AddToPathAction{PackName: "tools", DirPath: "bin"},
			&types.AddToShellProfileAction{PackName: "bash", ScriptPath: "init.sh"},
		}

		// Execute
		filtered := core.FilterConfigurationActions(actions)

		// Verify
		assert.Len(t, filtered, 3)
		for _, action := range filtered {
			assert.Implements(t, (*types.LinkingAction)(nil), action)
		}
	})

	t.Run("handles empty action list", func(t *testing.T) {
		// Execute
		filtered := core.FilterConfigurationActions([]types.Action{})

		// Verify
		assert.Empty(t, filtered)
	})
}

func TestFilterCodeExecutionActions(t *testing.T) {
	t.Run("filters out linking actions", func(t *testing.T) {
		// Setup
		linkAction := &types.LinkAction{
			PackName:   "vim",
			SourceFile: ".vimrc",
			TargetFile: "~/.vimrc",
		}
		scriptAction := &types.RunScriptAction{
			PackName:     "vim",
			ScriptPath:   "install.sh",
			SentinelName: "vim-installed",
		}
		brewAction := &types.BrewAction{
			PackName:     "tools",
			BrewfilePath: "Brewfile",
		}

		actions := []types.Action{linkAction, scriptAction, brewAction}

		// Execute
		filtered := core.FilterCodeExecutionActions(actions)

		// Verify - should include script and brew actions
		assert.Len(t, filtered, 2)
		assert.IsType(t, &types.RunScriptAction{}, filtered[0])
		assert.IsType(t, &types.BrewAction{}, filtered[1])
	})

	t.Run("includes all provisioning actions", func(t *testing.T) {
		// Setup
		actions := []types.Action{
			&types.RunScriptAction{PackName: "vim"},
			&types.BrewAction{PackName: "tools"},
			&types.RecordProvisioningAction{PackName: "bash"},
		}

		// Execute
		filtered := core.FilterCodeExecutionActions(actions)

		// Verify
		assert.Len(t, filtered, 3)
		for _, action := range filtered {
			assert.Implements(t, (*types.ProvisioningAction)(nil), action)
		}
	})

	t.Run("handles mixed action types", func(t *testing.T) {
		// Setup
		actions := []types.Action{
			&types.LinkAction{PackName: "vim"},
			&types.RunScriptAction{PackName: "vim"},
			&types.AddToPathAction{PackName: "tools"},
			&types.BrewAction{PackName: "tools"},
		}

		// Execute
		filtered := core.FilterCodeExecutionActions(actions)

		// Verify - should only have provisioning actions
		assert.Len(t, filtered, 2)
		assert.IsType(t, &types.RunScriptAction{}, filtered[0])
		assert.IsType(t, &types.BrewAction{}, filtered[1])
	})
}

func TestFilterProvisioningActions(t *testing.T) {
	t.Run("includes all actions when force is true", func(t *testing.T) {
		// Setup
		actions := []types.Action{
			&types.RunScriptAction{
				PackName:     "vim",
				ScriptPath:   "install.sh",
				SentinelName: "vim-installed",
				Checksum:     "abc123",
			},
			&types.BrewAction{
				PackName:     "tools",
				BrewfilePath: "Brewfile",
				Checksum:     "def456",
			},
			&types.LinkAction{
				PackName:   "vim",
				SourceFile: ".vimrc",
				TargetFile: "~/.vimrc",
			},
		}

		dataStore := &mockDataStore{
			needs: map[string]bool{
				"vim:vim-installed":             false,
				"tools:homebrew-tools.sentinel": false,
			},
		}

		// Execute with force=true
		filtered, err := core.FilterProvisioningActions(actions, true, dataStore)

		// Verify
		require.NoError(t, err)
		assert.Equal(t, actions, filtered)
	})

	t.Run("filters already provisioned actions when force is false", func(t *testing.T) {
		// Setup
		actions := []types.Action{
			&types.RunScriptAction{
				PackName:     "vim",
				ScriptPath:   "install.sh",
				SentinelName: "vim-installed",
				Checksum:     "abc123",
			},
			&types.BrewAction{
				PackName:     "tools",
				BrewfilePath: "Brewfile",
				Checksum:     "def456",
			},
			&types.LinkAction{
				PackName:   "vim",
				SourceFile: ".vimrc",
				TargetFile: "~/.vimrc",
			},
		}

		dataStore := &mockDataStore{
			needs: map[string]bool{
				"vim:vim-installed":             true,  // needs provisioning
				"tools:homebrew-tools.sentinel": false, // already provisioned
			},
		}

		// Execute with force=false
		filtered, err := core.FilterProvisioningActions(actions, false, dataStore)

		// Verify
		require.NoError(t, err)
		assert.Len(t, filtered, 2) // script action and link action
		assert.IsType(t, &types.RunScriptAction{}, filtered[0])
		assert.IsType(t, &types.LinkAction{}, filtered[1])
	})

	t.Run("includes all non-provisioning actions", func(t *testing.T) {
		// Setup
		actions := []types.Action{
			&types.LinkAction{PackName: "vim"},
			&types.AddToPathAction{PackName: "tools"},
		}

		dataStore := &mockDataStore{
			needs: map[string]bool{},
		}

		// Execute
		filtered, err := core.FilterProvisioningActions(actions, false, dataStore)

		// Verify
		require.NoError(t, err)
		assert.Equal(t, actions, filtered)
	})

	t.Run("handles data store errors", func(t *testing.T) {
		// Setup
		actions := []types.Action{
			&types.RunScriptAction{
				PackName:     "vim",
				ScriptPath:   "install.sh",
				SentinelName: "vim-installed",
				Checksum:     "abc123",
			},
		}

		dataStore := &mockDataStore{
			err: assert.AnError,
		}

		// Execute
		filtered, err := core.FilterProvisioningActions(actions, false, dataStore)

		// Verify
		assert.Error(t, err)
		assert.Contains(t, err.Error(), "failed to check provisioning status")
		assert.Nil(t, filtered)
	})
}

// Test groupMatchesByHandler helper function
func TestGroupMatchesByHandler(t *testing.T) {
	t.Run("groups matches by handler name", func(t *testing.T) {
		// Setup
		matches := []types.RuleMatch{
			{HandlerName: "symlink", Pack: "vim", Path: ".vimrc"},
			{HandlerName: "symlink", Pack: "bash", Path: ".bashrc"},
			{HandlerName: "homebrew", Pack: "tools", Path: "Brewfile"},
			{HandlerName: "install", Pack: "vim", Path: "install.sh"},
			{HandlerName: "install", Pack: "tools", Path: "setup.sh"},
		}

		// Execute
		result := core.GroupMatchesByHandler(matches)

		// Verify
		assert.Len(t, result, 3)
		assert.Len(t, result["symlink"], 2)
		assert.Len(t, result["install"], 2)
		assert.Len(t, result["homebrew"], 1)
	})

	t.Run("handles empty matches", func(t *testing.T) {
		// Execute
		result := core.GroupMatchesByHandler([]types.RuleMatch{})

		// Verify
		assert.Empty(t, result)
	})

	t.Run("ignores matches without handler name", func(t *testing.T) {
		// Setup
		matches := []types.RuleMatch{
			{HandlerName: "symlink", Pack: "vim", Path: ".vimrc"},
			{HandlerName: "", Pack: "bash", Path: ".bashrc"},
			{Pack: "tools", Path: "config"},
		}

		// Execute
		result := core.GroupMatchesByHandler(matches)

		// Verify
		assert.Len(t, result, 1)
		assert.Len(t, result["symlink"], 1)
	})
}
