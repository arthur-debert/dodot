// pkg/types/pack_provisioning_test.go
// TEST TYPE: Unit Tests
// DEPENDENCIES: pkg/testutil
// PURPOSE: Test Pack provisioning-related methods

package types_test

import (
	"testing"

	"github.com/arthur-debert/dodot/pkg/testutil"
	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/stretchr/testify/assert"
)

func TestPackIsHandlerProvisioned(t *testing.T) {
	tests := []struct {
		name        string
		pack        *types.Pack
		handler     string
		setupFunc   func(*testutil.MockDataStore)
		expected    bool
		expectError bool
	}{
		{
			name:    "provisioned handler returns true",
			pack:    &types.Pack{Name: "vim"},
			handler: "homebrew",
			setupFunc: func(ds *testutil.MockDataStore) {
				ds.SetSentinel("vim", "homebrew", "test-sentinel", true)
			},
			expected: true,
		},
		{
			name:    "unprovisioned handler returns false",
			pack:    &types.Pack{Name: "vim"},
			handler: "install",
			setupFunc: func(ds *testutil.MockDataStore) {
				// No sentinels set means no state
			},
			expected: false,
		},
		{
			name:        "non-existent handler returns false",
			pack:        &types.Pack{Name: "vim"},
			handler:     "nonexistent",
			setupFunc:   func(ds *testutil.MockDataStore) {},
			expected:    false,
			expectError: false,
		},
		{
			name:    "error from datastore propagates",
			pack:    &types.Pack{Name: "vim"},
			handler: "homebrew",
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

			// Test
			result, err := tt.pack.IsHandlerProvisioned(ds, tt.handler)

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

func TestPackGetProvisionedHandlers(t *testing.T) {
	tests := []struct {
		name        string
		pack        *types.Pack
		setupFunc   func(*testutil.MockDataStore)
		expected    []string
		expectError bool
	}{
		{
			name: "returns only handlers with state",
			pack: &types.Pack{Name: "vim"},
			setupFunc: func(ds *testutil.MockDataStore) {
				ds.SetSentinel("vim", "symlink", "test1", true)
				ds.SetSentinel("vim", "homebrew", "test2", true)
				// install handler has no sentinels (no state)
				ds.SetSentinel("vim", "shell", "test3", true)
			},
			expected: []string{"symlink", "homebrew", "shell"},
		},
		{
			name: "pack with no provisioned handlers",
			pack: &types.Pack{Name: "tmux"},
			setupFunc: func(ds *testutil.MockDataStore) {
				// No sentinels set for any handlers
			},
			expected: []string{},
		},
		{
			name:      "non-existent pack returns empty list",
			pack:      &types.Pack{Name: "nonexistent"},
			setupFunc: func(ds *testutil.MockDataStore) {},
			expected:  []string{},
		},
		{
			name: "error from ListPackHandlers propagates",
			pack: &types.Pack{Name: "vim"},
			setupFunc: func(ds *testutil.MockDataStore) {
				ds.WithError("ListPackHandlers", assert.AnError)
			},
			expected:    nil,
			expectError: true,
		},
		{
			name: "mixed state handlers",
			pack: &types.Pack{Name: "git"},
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

			// Test
			result, err := tt.pack.GetProvisionedHandlers(ds)

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
