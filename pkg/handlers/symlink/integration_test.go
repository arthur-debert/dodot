package symlink_test

import (
	"os"
	"testing"

	"github.com/arthur-debert/dodot/pkg/handlers/symlink"
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

func TestSymlinkHandler_OperationIntegration(t *testing.T) {
	// This test verifies the symlink handler works with the operation system

	// Set HOME for consistent tests
	oldHome := os.Getenv("HOME")
	_ = os.Setenv("HOME", "/home/testuser")
	defer func() { _ = os.Setenv("HOME", oldHome) }()

	// Create simplified handler
	handler := symlink.NewHandler()

	// Create test matches
	matches := []types.RuleMatch{
		{
			Pack:         "vim",
			Path:         ".vimrc",
			AbsolutePath: "/dotfiles/vim/.vimrc",
			HandlerName:  "symlink",
		},
		{
			Pack:         "vim",
			Path:         ".vim/colors/theme.vim",
			AbsolutePath: "/dotfiles/vim/.vim/colors/theme.vim",
			HandlerName:  "symlink",
		},
	}

	// Convert to operations
	ops, err := handler.ToOperations(matches)
	require.NoError(t, err)
	assert.Len(t, ops, 4) // 2 operations per file

	// Verify operations
	assert.Equal(t, operations.CreateDataLink, ops[0].Type)
	assert.Equal(t, operations.CreateUserLink, ops[1].Type)
	assert.Equal(t, operations.CreateDataLink, ops[2].Type)
	assert.Equal(t, operations.CreateUserLink, ops[3].Type)

	// Test with executor in dry-run mode
	store := new(MockSimpleDataStore)
	executor := operations.NewExecutor(store, nil, true)

	// Execute operations
	results, err := executor.Execute(ops, handler)
	require.NoError(t, err)
	assert.Len(t, results, 4)

	// All should be successful in dry run
	for _, result := range results {
		assert.True(t, result.Success)
		assert.Contains(t, result.Message, "Would")
	}
}

func TestSymlinkHandler_ExecuteWithDataStore(t *testing.T) {
	// This test verifies actual execution with the datastore

	// Set HOME for consistent tests
	oldHome := os.Getenv("HOME")
	_ = os.Setenv("HOME", "/home/testuser")
	defer func() { _ = os.Setenv("HOME", oldHome) }()

	handler := symlink.NewHandler()

	matches := []types.RuleMatch{
		{
			Pack:         "vim",
			Path:         ".vimrc",
			AbsolutePath: "/dotfiles/vim/.vimrc",
			HandlerName:  "symlink",
		},
	}

	ops, err := handler.ToOperations(matches)
	require.NoError(t, err)

	// Create mock store and set expectations
	store := new(MockSimpleDataStore)

	// Expect CreateDataLink to be called
	store.On("CreateDataLink", "vim", "symlink", "/dotfiles/vim/.vimrc").
		Return("/datastore/vim/symlinks/.vimrc", nil)

	// Expect CreateUserLink to be called
	// NOTE: In the current implementation, the executor passes op.Source
	// directly rather than the datastore path. This is a known issue
	// that will be resolved in Phase 3 when we remove the adapters.
	store.On("CreateUserLink", "/dotfiles/vim/.vimrc", "/home/testuser/.vimrc").
		Return(nil)

	// Execute with real mode (not dry-run)
	executor := operations.NewExecutor(store, nil, false)
	results, err := executor.Execute(ops, handler)

	require.NoError(t, err)
	assert.Len(t, results, 2)

	// Both operations should succeed
	assert.True(t, results[0].Success)
	assert.Contains(t, results[0].Message, "Created data link")
	assert.True(t, results[1].Success)
	assert.Contains(t, results[1].Message, "Created link")

	// Verify all expectations were met
	store.AssertExpectations(t)
}

func TestSymlinkHandler_Clear(t *testing.T) {
	// Test clear functionality
	handler := symlink.NewHandler()

	// Create mock store and executor
	store := new(MockSimpleDataStore)
	executor := operations.NewExecutor(store, nil, false)

	// Clear context
	ctx := operations.ClearContext{
		Pack: types.Pack{
			Name: "vim",
		},
		DryRun: true,
	}

	// Execute clear
	clearedItems, err := executor.ExecuteClear(handler, ctx)
	require.NoError(t, err)
	assert.Len(t, clearedItems, 1)

	// Check cleared item
	item := clearedItems[0]
	assert.Equal(t, "symlink_state", item.Type)
	assert.Contains(t, item.Path, "vim/symlink")
	assert.Contains(t, item.Description, "Would remove symlink")
}
