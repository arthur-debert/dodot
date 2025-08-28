package executor_test

import (
	"testing"

	"github.com/arthur-debert/dodot/pkg/executor"
	"github.com/arthur-debert/dodot/pkg/handlers"
	"github.com/arthur-debert/dodot/pkg/handlers/homebrew"
	"github.com/arthur-debert/dodot/pkg/handlers/install"
	"github.com/arthur-debert/dodot/pkg/handlers/path"
	"github.com/arthur-debert/dodot/pkg/handlers/symlink"
	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestClearInfrastructure_Integration(t *testing.T) {
	// This test verifies that the clear infrastructure works with real handlers

	tests := []struct {
		name        string
		handlerName string
		handler     handlers.Clearable
		handlerType types.HandlerType
		fullHandler handlers.Handler // Store the full handler to access Type()
	}{
		{
			name:        "symlink handler",
			handlerName: symlink.SymlinkHandlerName,
			handler:     symlink.NewSymlinkHandler(),
			handlerType: types.HandlerTypeConfiguration,
			fullHandler: symlink.NewSymlinkHandler(),
		},
		{
			name:        "path handler",
			handlerName: path.PathHandlerName,
			handler:     path.NewPathHandler(),
			handlerType: types.HandlerTypeConfiguration,
			fullHandler: path.NewPathHandler(),
		},
		{
			name:        "homebrew handler",
			handlerName: homebrew.HomebrewHandlerName,
			handler:     homebrew.NewHomebrewHandler(),
			handlerType: types.HandlerTypeCodeExecution,
			fullHandler: homebrew.NewHomebrewHandler(),
		},
		{
			name:        "install handler",
			handlerName: install.InstallHandlerName,
			handler:     install.NewInstallHandler(),
			handlerType: types.HandlerTypeCodeExecution,
			fullHandler: install.NewInstallHandler(),
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			// Handler is already types.Clearable in the test struct
			clearable := tt.handler

			// Verify handler has the correct type
			actualType := tt.fullHandler.Type()
			assert.Equal(t, tt.handlerType, actualType, "%s should have correct handler type", tt.handlerName)

			// Create a simple context to test Clear doesn't panic
			ctx := types.ClearContext{
				Pack: types.Pack{
					Name: "test",
					Path: "/test",
				},
				DataStore: &mockClearDataStore{},
				FS:        &mockFilterFS{existingDirs: make(map[string]bool)},
				Paths:     &mockFilterPaths{},
				DryRun:    true,
			}

			// Verify Clear can be called without panic
			_, err := clearable.Clear(ctx)
			// We don't check the error because handlers might fail without proper setup
			// The important thing is that they implement the interface correctly
			_ = err
		})
	}
}

func TestClearHelpers_WithRealHandlers(t *testing.T) {
	// Test GetClearableHandlersByMode with real handler setup

	// Test configuration handlers
	linkingHandlers, err := executor.GetClearableHandlersByType(types.HandlerTypeConfiguration)
	require.NoError(t, err)

	// We expect at least symlink and path handlers
	assert.GreaterOrEqual(t, len(linkingHandlers), 2, "Should have at least 2 linking handlers")

	// Verify specific handlers are present
	_, hasSymlink := linkingHandlers[symlink.SymlinkHandlerName]
	assert.True(t, hasSymlink, "Should have symlink handler")

	_, hasPath := linkingHandlers[path.PathHandlerName]
	assert.True(t, hasPath, "Should have path handler")

	// Test code execution handlers
	provisioningHandlers, err := executor.GetClearableHandlersByType(types.HandlerTypeCodeExecution)
	require.NoError(t, err)

	// We expect at least homebrew and provision handlers
	assert.GreaterOrEqual(t, len(provisioningHandlers), 2, "Should have at least 2 provisioning handlers")

	// Verify specific handlers are present
	_, hasHomebrew := provisioningHandlers[homebrew.HomebrewHandlerName]
	assert.True(t, hasHomebrew, "Should have homebrew handler")

	_, hasInstall := provisioningHandlers[install.InstallHandlerName]
	assert.True(t, hasInstall, "Should have install handler")

	// Test GetAllClearableHandlers
	allHandlers, err := executor.GetAllClearableHandlers()
	require.NoError(t, err)

	// Should have all handlers combined
	expectedTotal := len(linkingHandlers) + len(provisioningHandlers)
	assert.Equal(t, expectedTotal, len(allHandlers), "Should have all handlers combined")
}
