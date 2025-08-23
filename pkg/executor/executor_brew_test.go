package executor_test

import (
	"testing"

	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/stretchr/testify/assert"
)

func TestExecutor_Execute_BrewAction(t *testing.T) {
	tests := []struct {
		name                string
		action              *types.BrewAction
		needsProvisioning   bool
		needsError          error
		recordError         error
		dryRun              bool
		expectedSuccess     bool
		expectedSkipped     bool
		expectedError       string
		expectedDescription string
	}{
		{
			name: "successful brew action",
			action: &types.BrewAction{
				PackName:     "tools",
				BrewfilePath: "/path/to/Brewfile",
				Checksum:     "sha256:abcd1234",
			},
			needsProvisioning:   true,
			expectedSuccess:     true,
			expectedDescription: "Install Homebrew packages from /path/to/Brewfile",
		},
		{
			name: "already provisioned",
			action: &types.BrewAction{
				PackName:     "tools",
				BrewfilePath: "/path/to/Brewfile",
				Checksum:     "sha256:abcd1234",
			},
			needsProvisioning:   false,
			expectedSuccess:     true,
			expectedDescription: "Install Homebrew packages from /path/to/Brewfile",
		},
		{
			name: "dry run",
			action: &types.BrewAction{
				PackName:     "tools",
				BrewfilePath: "/path/to/Brewfile",
				Checksum:     "sha256:abcd1234",
			},
			dryRun:              true,
			expectedSuccess:     true,
			expectedSkipped:     true,
			expectedDescription: "Install Homebrew packages from /path/to/Brewfile",
		},
		{
			name: "needs provisioning check error",
			action: &types.BrewAction{
				PackName:     "tools",
				BrewfilePath: "/path/to/Brewfile",
				Checksum:     "sha256:abcd1234",
			},
			needsError:          assert.AnError,
			expectedSuccess:     false,
			expectedError:       "failed to check provisioning status",
			expectedDescription: "Install Homebrew packages from /path/to/Brewfile",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			mockStore := new(MockDataStore)

			// Setup expectations for non-dry-run cases
			if !tt.dryRun {
				sentinelName := "homebrew-" + tt.action.PackName + ".sentinel"
				mockStore.On("NeedsProvisioning", tt.action.PackName, sentinelName, tt.action.Checksum).
					Return(tt.needsProvisioning, tt.needsError).
					Maybe()

				if tt.needsError == nil && tt.needsProvisioning {
					mockStore.On("RecordProvisioning", tt.action.PackName, sentinelName, tt.action.Checksum).
						Return(tt.recordError).
						Maybe()
				}
			}

			// Note: We're not testing the actual brew command execution here
			// In a real test environment, we'd need to mock exec.Command or skip this test

			// For this test, we'll just verify the action properties
			assert.Equal(t, tt.expectedDescription, tt.action.Description())
			assert.Equal(t, tt.action.PackName, tt.action.Pack())

			// Test the action's Execute method
			if !tt.dryRun {
				err := tt.action.Execute(mockStore)
				if tt.expectedError != "" {
					assert.Error(t, err)
					assert.Contains(t, err.Error(), tt.expectedError)
				} else {
					assert.NoError(t, err)
				}
			}

			mockStore.AssertExpectations(t)
		})
	}
}

// MockDataStore is defined in executor_test.go
