// pkg/packcommands/provisioning_test.go
// TEST TYPE: Unit Tests
// DEPENDENCIES: pkg/testutil
// PURPOSE: Test provisioning query functions

package commands_test

import (
	"testing"

	"github.com/arthur-debert/dodot/pkg/packs/commands"
	"github.com/arthur-debert/dodot/pkg/testutil"
	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/stretchr/testify/assert"
)

func TestIsPackHandlerProvisioned(t *testing.T) {
	tests := []struct {
		name        string
		packName    string
		handler     string
		setupFunc   func(*testutil.MockDataStore)
		expected    bool
		expectError bool
	}{
		{
			name:     "provisioned handler returns true",
			packName: "vim",
			handler:  "homebrew",
			setupFunc: func(ds *testutil.MockDataStore) {
				ds.SetSentinel("vim", "homebrew", "test-sentinel", true)
			},
			expected: true,
		},
		{
			name:     "unprovisioned handler returns false",
			packName: "vim",
			handler:  "install",
			setupFunc: func(ds *testutil.MockDataStore) {
				// No sentinels set means no state
			},
			expected: false,
		},
		{
			name:        "non-existent handler returns false",
			packName:    "vim",
			handler:     "nonexistent",
			setupFunc:   func(ds *testutil.MockDataStore) {},
			expected:    false,
			expectError: false,
		},
		{
			name:     "error from datastore propagates",
			packName: "vim",
			handler:  "homebrew",
			setupFunc: func(ds *testutil.MockDataStore) {
				ds.WithError("HasHandlerState", assert.AnError)
			},
			expected:    false,
			expectError: true,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			// Setup
			ds := testutil.NewMockDataStore()
			if tt.setupFunc != nil {
				tt.setupFunc(ds)
			}

			// Create minimal pack - only needs Name
			pack := &types.Pack{Name: tt.packName}

			// Test
			result, err := commands.IsPackHandlerProvisioned(pack, ds, tt.handler)

			// Verify
			if tt.expectError {
				assert.Error(t, err)
			} else {
				assert.NoError(t, err)
			}
			assert.Equal(t, tt.expected, result)
		})
	}
}

func TestGetPackProvisionedHandlers(t *testing.T) {
	tests := []struct {
		name        string
		packName    string
		setupFunc   func(*testutil.MockDataStore)
		expected    []string
		expectError bool
	}{
		{
			name:     "returns only handlers with state",
			packName: "vim",
			setupFunc: func(ds *testutil.MockDataStore) {
				ds.SetSentinel("vim", "symlink", "test1", true)
				ds.SetSentinel("vim", "homebrew", "test2", true)
				// install handler has no sentinels (no state)
				ds.SetSentinel("vim", "shell", "test3", true)
			},
			expected: []string{"symlink", "homebrew", "shell"},
		},
		{
			name:     "pack with no provisioned handlers",
			packName: "tmux",
			setupFunc: func(ds *testutil.MockDataStore) {
				// No sentinels set for any handlers
			},
			expected: []string{},
		},
		{
			name:      "non-existent pack returns empty list",
			packName:  "nonexistent",
			setupFunc: func(ds *testutil.MockDataStore) {},
			expected:  []string{},
		},
		{
			name:     "error from ListPackHandlers propagates",
			packName: "vim",
			setupFunc: func(ds *testutil.MockDataStore) {
				ds.WithError("ListPackHandlers", assert.AnError)
			},
			expected:    nil,
			expectError: true,
		},
		{
			name:     "mixed state handlers",
			packName: "git",
			setupFunc: func(ds *testutil.MockDataStore) {
				ds.SetSentinel("git", "symlink", "test1", true)
				// shell handler has no sentinels (no state)
				ds.SetSentinel("git", "path", "test2", true)
				// homebrew handler has no sentinels (no state)
			},
			expected: []string{"symlink", "path"},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			// Setup
			ds := testutil.NewMockDataStore()
			if tt.setupFunc != nil {
				tt.setupFunc(ds)
			}

			// Create minimal pack - only needs Name
			pack := &types.Pack{Name: tt.packName}

			// Test
			result, err := commands.GetPackProvisionedHandlers(pack, ds)

			// Verify
			if tt.expectError {
				assert.Error(t, err)
				if tt.expected == nil {
					assert.Nil(t, result)
				}
			} else {
				assert.NoError(t, err)
				assert.ElementsMatch(t, tt.expected, result)
			}
		})
	}
}