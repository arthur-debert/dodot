package shell_test

import (
	"testing"

	"github.com/arthur-debert/dodot/pkg/handlers/lib/shell"
	"github.com/arthur-debert/dodot/pkg/operations"
	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/mock"
	"github.com/stretchr/testify/require"
)

type MockSimpleDataStore struct {
	mock.Mock
}

func (m *MockSimpleDataStore) CreateDataLink(pack, handlerName, sourceFile string) (string, error) {
	args := m.Called(pack, handlerName, sourceFile)
	return args.String(0), args.Error(1)
}

func (m *MockSimpleDataStore) CreateUserLink(datastorePath, userPath string) error {
	args := m.Called(datastorePath, userPath)
	return args.Error(0)
}

func (m *MockSimpleDataStore) RunAndRecord(pack, handlerName, command, sentinel string) error {
	args := m.Called(pack, handlerName, command, sentinel)
	return args.Error(0)
}

func (m *MockSimpleDataStore) HasSentinel(pack, handlerName, sentinel string) (bool, error) {
	args := m.Called(pack, handlerName, sentinel)
	return args.Bool(0), args.Error(1)
}

func (m *MockSimpleDataStore) RemoveState(pack, handlerName string) error {
	args := m.Called(pack, handlerName)
	return args.Error(0)
}

func (m *MockSimpleDataStore) HasHandlerState(pack, handlerName string) (bool, error) {
	args := m.Called(pack, handlerName)
	return args.Bool(0), args.Error(1)
}

func (m *MockSimpleDataStore) ListPackHandlers(pack string) ([]string, error) {
	args := m.Called(pack)
	return args.Get(0).([]string), args.Error(1)
}

func (m *MockSimpleDataStore) ListHandlerSentinels(pack, handlerName string) ([]string, error) {
	args := m.Called(pack, handlerName)
	return args.Get(0).([]string), args.Error(1)
}

func TestShellHandler_OperationIntegration(t *testing.T) {
	// This test verifies the shell handler works with the operation system

	// Create simplified handler
	handler := shell.NewHandler()

	// Create test matches
	matches := []operations.FileInput{
		{
			PackName:     "bash",
			RelativePath: "aliases.sh",
			SourcePath:   "/dotfiles/bash/aliases.sh",
		},
		{
			PackName:     "bash",
			RelativePath: "functions.sh",
			SourcePath:   "/dotfiles/bash/functions.sh",
		},
		{
			PackName:     "zsh",
			RelativePath: "config.zsh",
			SourcePath:   "/dotfiles/zsh/config.zsh",
		},
	}

	// Convert to operations
	ops, err := handler.ToOperations(matches, nil)
	require.NoError(t, err)
	assert.Len(t, ops, 3) // One operation per script

	// Verify operations
	for i, op := range ops {
		assert.Equal(t, operations.CreateDataLink, op.Type)
		assert.Equal(t, "shell", op.Handler)
		assert.Equal(t, matches[i].PackName, op.Pack)
		assert.Equal(t, matches[i].SourcePath, op.Source)
	}

	// Test with executor in dry-run mode
	store := new(MockSimpleDataStore)
	executor := operations.NewExecutor(store, nil, true)

	// Execute operations
	results, err := executor.Execute(ops, handler)
	require.NoError(t, err)
	assert.Len(t, results, 3)

	// All should be successful in dry run
	for _, result := range results {
		assert.True(t, result.Success)
		assert.Contains(t, result.Message, "Would create data link")
	}
}

func TestShellHandler_ExecuteWithDataStore(t *testing.T) {
	// This test verifies actual execution with the datastore

	handler := shell.NewHandler()

	matches := []operations.FileInput{
		{
			PackName:     "bash",
			RelativePath: "aliases.sh",
			SourcePath:   "/dotfiles/bash/aliases.sh",
		},
	}

	ops, err := handler.ToOperations(matches, nil)
	require.NoError(t, err)

	// Create mock store and set expectations
	store := new(MockSimpleDataStore)

	// Expect CreateDataLink to be called
	store.On("CreateDataLink", "bash", "shell", "/dotfiles/bash/aliases.sh").
		Return("/datastore/bash/shell/aliases.sh", nil)

	// Execute with real mode (not dry-run)
	executor := operations.NewExecutor(store, nil, false)
	results, err := executor.Execute(ops, handler)

	require.NoError(t, err)
	assert.Len(t, results, 1)

	// Operation should succeed
	assert.True(t, results[0].Success)
	assert.Contains(t, results[0].Message, "Created data link")

	// Verify all expectations were met
	store.AssertExpectations(t)
}

func TestShellHandler_ClearIntegration(t *testing.T) {
	// Test clear functionality
	handler := shell.NewHandler()

	// Create mock store and executor
	store := new(MockSimpleDataStore)
	executor := operations.NewExecutor(store, nil, false)

	// Clear context
	ctx := operations.ClearContext{
		Pack: types.Pack{
			Name: "bash",
		},
		DryRun: true,
	}

	// Execute clear
	clearedItems, err := executor.ExecuteClear(handler, ctx)
	require.NoError(t, err)
	assert.Len(t, clearedItems, 1)

	// Check cleared item
	item := clearedItems[0]
	assert.Equal(t, "shell_state", item.Type)
	assert.Contains(t, item.Path, "bash/shell")
	assert.Equal(t, "Would remove shell state", item.Description)
}
