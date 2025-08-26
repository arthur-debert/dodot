package executor_test

import (
	"errors"
	"testing"

	"github.com/arthur-debert/dodot/pkg/executor"
	"github.com/arthur-debert/dodot/pkg/handlers"
	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

// Mock handler for testing
type mockClearableHandler struct {
	name         string
	clearItems   []types.ClearedItem
	clearError   error
	clearCalled  bool
	clearContext types.ClearContext
}

func (h *mockClearableHandler) Clear(ctx types.ClearContext) ([]types.ClearedItem, error) {
	h.clearCalled = true
	h.clearContext = ctx
	return h.clearItems, h.clearError
}

// Mock DataStore for testing
type mockClearDataStore struct {
	deleteProvisioningStateCalls []deleteCall
	deleteError                  error
}

type deleteCall struct {
	packName    string
	handlerName string
}

func (m *mockClearDataStore) DeleteProvisioningState(packName, handlerName string) error {
	m.deleteProvisioningStateCalls = append(m.deleteProvisioningStateCalls, deleteCall{
		packName:    packName,
		handlerName: handlerName,
	})
	return m.deleteError
}

// Implement remaining DataStore methods
func (m *mockClearDataStore) Link(pack, sourceFile string) (string, error)    { return "", nil }
func (m *mockClearDataStore) Unlink(pack, sourceFile string) error            { return nil }
func (m *mockClearDataStore) AddToPath(pack, dirPath string) error            { return nil }
func (m *mockClearDataStore) AddToShellProfile(pack, scriptPath string) error { return nil }
func (m *mockClearDataStore) RecordProvisioning(pack, sentinelName, checksum string) error {
	return nil
}
func (m *mockClearDataStore) NeedsProvisioning(pack, sentinelName, checksum string) (bool, error) {
	return false, nil
}
func (m *mockClearDataStore) GetStatus(pack, sourceFile string) (types.Status, error) {
	return types.Status{}, nil
}
func (m *mockClearDataStore) GetSymlinkStatus(pack, sourceFile string) (types.Status, error) {
	return types.Status{}, nil
}
func (m *mockClearDataStore) GetPathStatus(pack, dirPath string) (types.Status, error) {
	return types.Status{}, nil
}
func (m *mockClearDataStore) GetShellProfileStatus(pack, scriptPath string) (types.Status, error) {
	return types.Status{}, nil
}
func (m *mockClearDataStore) GetProvisioningStatus(pack, sentinelName, currentChecksum string) (types.Status, error) {
	return types.Status{}, nil
}
func (m *mockClearDataStore) GetBrewStatus(pack, brewfilePath, currentChecksum string) (types.Status, error) {
	return types.Status{}, nil
}
func (m *mockClearDataStore) GetProvisioningHandlers(packName string) ([]string, error) {
	return []string{}, nil
}
func (m *mockClearDataStore) ListProvisioningState(packName string) (map[string][]string, error) {
	return map[string][]string{}, nil
}

func TestClearHandler_Success(t *testing.T) {
	// Setup
	handler := &mockClearableHandler{
		name: "test-handler",
		clearItems: []types.ClearedItem{
			{Type: "test", Path: "/test/path", Description: "Test item"},
		},
	}

	dataStore := &mockClearDataStore{}

	ctx := types.ClearContext{
		Pack: types.Pack{
			Name: "testpack",
			Path: "/test/pack",
		},
		DataStore: dataStore,
		DryRun:    false,
	}

	// Execute
	result, err := executor.ClearHandler(ctx, handler, "test-handler")

	// Verify
	require.NoError(t, err)
	assert.True(t, handler.clearCalled, "Handler Clear should be called")
	assert.Equal(t, ctx, handler.clearContext, "Context should be passed correctly")

	assert.Equal(t, "test-handler", result.HandlerName)
	assert.Len(t, result.ClearedItems, 1)
	assert.Equal(t, "Test item", result.ClearedItems[0].Description)
	assert.True(t, result.StateRemoved)
	assert.NoError(t, result.Error)

	// Verify state deletion was called
	assert.Len(t, dataStore.deleteProvisioningStateCalls, 1)
	assert.Equal(t, "testpack", dataStore.deleteProvisioningStateCalls[0].packName)
	assert.Equal(t, "test-handler", dataStore.deleteProvisioningStateCalls[0].handlerName)
}

func TestClearHandler_DryRun(t *testing.T) {
	// Setup
	handler := &mockClearableHandler{
		name: "test-handler",
		clearItems: []types.ClearedItem{
			{Type: "test", Path: "/test/path", Description: "Would remove test item"},
		},
	}

	dataStore := &mockClearDataStore{}

	ctx := types.ClearContext{
		Pack: types.Pack{
			Name: "testpack",
			Path: "/test/pack",
		},
		DataStore: dataStore,
		DryRun:    true, // Dry run mode
	}

	// Execute
	result, err := executor.ClearHandler(ctx, handler, "test-handler")

	// Verify
	require.NoError(t, err)
	assert.True(t, handler.clearCalled, "Handler Clear should be called even in dry run")

	assert.Equal(t, "test-handler", result.HandlerName)
	assert.Len(t, result.ClearedItems, 1)
	assert.False(t, result.StateRemoved, "State should not be removed in dry run")

	// Verify state deletion was NOT called
	assert.Empty(t, dataStore.deleteProvisioningStateCalls, "DeleteProvisioningState should not be called in dry run")
}

func TestClearHandler_HandlerError(t *testing.T) {
	// Setup
	handler := &mockClearableHandler{
		name:       "test-handler",
		clearError: errors.New("handler failed"),
	}

	dataStore := &mockClearDataStore{}

	ctx := types.ClearContext{
		Pack: types.Pack{
			Name: "testpack",
			Path: "/test/pack",
		},
		DataStore: dataStore,
		DryRun:    false,
	}

	// Execute
	result, err := executor.ClearHandler(ctx, handler, "test-handler")

	// Verify
	require.Error(t, err)
	assert.Contains(t, err.Error(), "handler clear failed")
	assert.True(t, handler.clearCalled)

	assert.Equal(t, "test-handler", result.HandlerName)
	assert.Empty(t, result.ClearedItems)
	assert.False(t, result.StateRemoved, "State should not be removed if handler fails")
	assert.Error(t, result.Error)

	// Verify state deletion was NOT called
	assert.Empty(t, dataStore.deleteProvisioningStateCalls, "DeleteProvisioningState should not be called if handler fails")
}

func TestClearHandler_StateRemovalError(t *testing.T) {
	// Setup
	handler := &mockClearableHandler{
		name: "test-handler",
		clearItems: []types.ClearedItem{
			{Type: "test", Path: "/test/path", Description: "Test item"},
		},
	}

	dataStore := &mockClearDataStore{
		deleteError: errors.New("failed to delete state"),
	}

	ctx := types.ClearContext{
		Pack: types.Pack{
			Name: "testpack",
			Path: "/test/pack",
		},
		DataStore: dataStore,
		DryRun:    false,
	}

	// Execute
	result, err := executor.ClearHandler(ctx, handler, "test-handler")

	// Verify
	require.Error(t, err)
	assert.Contains(t, err.Error(), "failed to remove state directory")

	assert.Equal(t, "test-handler", result.HandlerName)
	assert.Len(t, result.ClearedItems, 1, "Handler items should still be recorded")
	assert.False(t, result.StateRemoved, "State removal should be false on error")
	assert.Error(t, result.Error)

	// Verify state deletion was attempted
	assert.Len(t, dataStore.deleteProvisioningStateCalls, 1)
}

func TestClearHandlers_Multiple(t *testing.T) {
	// Setup
	handler1 := &mockClearableHandler{
		name: "handler1",
		clearItems: []types.ClearedItem{
			{Type: "type1", Path: "/path1", Description: "Handler 1 item"},
		},
	}

	handler2 := &mockClearableHandler{
		name: "handler2",
		clearItems: []types.ClearedItem{
			{Type: "type2", Path: "/path2", Description: "Handler 2 item"},
		},
	}

	handlers := map[string]handlers.Clearable{
		"handler1": handler1,
		"handler2": handler2,
	}

	dataStore := &mockClearDataStore{}

	ctx := types.ClearContext{
		Pack: types.Pack{
			Name: "testpack",
			Path: "/test/pack",
		},
		DataStore: dataStore,
		DryRun:    false,
	}

	// Execute
	results, err := executor.ClearHandlers(ctx, handlers)

	// Verify
	require.NoError(t, err)
	assert.Len(t, results, 2)

	// Check handler1 result
	result1 := results["handler1"]
	assert.NotNil(t, result1)
	assert.Equal(t, "handler1", result1.HandlerName)
	assert.Len(t, result1.ClearedItems, 1)
	assert.True(t, result1.StateRemoved)

	// Check handler2 result
	result2 := results["handler2"]
	assert.NotNil(t, result2)
	assert.Equal(t, "handler2", result2.HandlerName)
	assert.Len(t, result2.ClearedItems, 1)
	assert.True(t, result2.StateRemoved)

	// Verify both handlers were called
	assert.True(t, handler1.clearCalled)
	assert.True(t, handler2.clearCalled)

	// Verify state deletion was called for both
	assert.Len(t, dataStore.deleteProvisioningStateCalls, 2)
}

func TestClearHandlers_PartialFailure(t *testing.T) {
	// Setup
	handler1 := &mockClearableHandler{
		name: "handler1",
		clearItems: []types.ClearedItem{
			{Type: "type1", Path: "/path1", Description: "Handler 1 item"},
		},
	}

	handler2 := &mockClearableHandler{
		name:       "handler2",
		clearError: errors.New("handler2 failed"),
	}

	handlers := map[string]handlers.Clearable{
		"handler1": handler1,
		"handler2": handler2,
	}

	dataStore := &mockClearDataStore{}

	ctx := types.ClearContext{
		Pack: types.Pack{
			Name: "testpack",
			Path: "/test/pack",
		},
		DataStore: dataStore,
		DryRun:    false,
	}

	// Execute
	results, err := executor.ClearHandlers(ctx, handlers)

	// Verify
	require.Error(t, err, "Should return error if any handler fails")
	assert.Len(t, results, 2, "Should still have results for all handlers")

	// Check handler1 succeeded
	result1 := results["handler1"]
	assert.NotNil(t, result1)
	assert.NoError(t, result1.Error)
	assert.True(t, result1.StateRemoved)

	// Check handler2 failed
	result2 := results["handler2"]
	assert.NotNil(t, result2)
	assert.Error(t, result2.Error)
	assert.False(t, result2.StateRemoved)

	// Verify state deletion was only called for successful handler
	assert.Len(t, dataStore.deleteProvisioningStateCalls, 1)
	assert.Equal(t, "handler1", dataStore.deleteProvisioningStateCalls[0].handlerName)
}
