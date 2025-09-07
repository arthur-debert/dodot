package homebrew_test

import (
	"fmt"
	"os"
	"path/filepath"
	"testing"

	"github.com/arthur-debert/dodot/pkg/handlers/lib/homebrew"
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

func TestHomebrewHandler_OperationIntegration(t *testing.T) {
	// This test verifies the homebrew handler works with the operation system

	// Create test Brewfile
	tempDir := t.TempDir()
	brewfileContent := `# Test Brewfile
brew "git"
cask "visual-studio-code"
`
	brewfilePath := filepath.Join(tempDir, "Brewfile")
	err := os.WriteFile(brewfilePath, []byte(brewfileContent), 0644)
	require.NoError(t, err)

	// Create simplified handler
	handler := homebrew.NewHandler()

	// Create test matches
	matches := []operations.FileInput{
		{
			PackName:     "dev-tools",
			RelativePath: "Brewfile",
			SourcePath:   brewfilePath,
		},
	}

	// Convert to operations
	ops, err := handler.ToOperations(matches, nil)
	require.NoError(t, err)
	assert.Len(t, ops, 1)

	// Verify operation
	op := ops[0]
	assert.Equal(t, operations.RunCommand, op.Type)
	assert.Equal(t, "dev-tools", op.Pack)
	assert.Equal(t, "homebrew", op.Handler)
	assert.Contains(t, op.Command, "brew bundle")
	assert.Contains(t, op.Command, brewfilePath)
	assert.NotEmpty(t, op.Sentinel)

	// Test with executor in dry-run mode
	store := new(MockSimpleDataStore)
	executor := operations.NewExecutor(store, nil, true)

	// Execute operations
	results, err := executor.Execute(ops, handler)
	require.NoError(t, err)
	assert.Len(t, results, 1)

	// Should be successful in dry run
	assert.True(t, results[0].Success)
	assert.Contains(t, results[0].Message, "Would execute")
}

func TestHomebrewHandler_ExecuteWithDataStore(t *testing.T) {
	// This test verifies actual execution with the datastore

	// Create test Brewfile
	tempDir := t.TempDir()
	brewfileContent := `brew "git"`
	brewfilePath := filepath.Join(tempDir, "Brewfile")
	err := os.WriteFile(brewfilePath, []byte(brewfileContent), 0644)
	require.NoError(t, err)

	handler := homebrew.NewHandler()

	matches := []operations.FileInput{
		{
			PackName:     "tools",
			RelativePath: "Brewfile",
			SourcePath:   brewfilePath,
		},
	}

	ops, err := handler.ToOperations(matches, nil)
	require.NoError(t, err)

	// Create mock store and set expectations
	store := new(MockSimpleDataStore)

	// Expect RunAndRecord to be called with the correct parameters
	expectedCommand := fmt.Sprintf("brew bundle --file='%s'", brewfilePath)
	store.On("RunAndRecord", "tools", "homebrew", expectedCommand, mock.MatchedBy(func(s string) bool {
		// Sentinel should contain pack, filename and checksum
		return len(s) > 0
	})).Return(nil)

	// Execute with real mode (not dry-run)
	executor := operations.NewExecutor(store, nil, false)
	results, err := executor.Execute(ops, handler)

	require.NoError(t, err)
	assert.Len(t, results, 1)

	// Operation should succeed
	assert.True(t, results[0].Success)
	assert.Contains(t, results[0].Message, "Executed")

	// Verify all expectations were met
	store.AssertExpectations(t)
}

func TestHomebrewHandler_ClearIntegration(t *testing.T) {
	// Test clear functionality
	handler := homebrew.NewHandler()

	// Create mock store and executor
	store := new(MockSimpleDataStore)
	executor := operations.NewExecutor(store, nil, false)

	// Clear context
	ctx := operations.ClearContext{
		Pack: types.Pack{
			Name: "dev-tools",
		},
		DryRun: true,
	}

	// Execute clear
	clearedItems, err := executor.ExecuteClear(handler, ctx)
	require.NoError(t, err)
	assert.Len(t, clearedItems, 1)

	// Check cleared item
	item := clearedItems[0]
	assert.Equal(t, "homebrew_state", item.Type)
	assert.Contains(t, item.Path, "dev-tools/homebrew")
	assert.Contains(t, item.Description, "Would remove Homebrew state")
	assert.Contains(t, item.Description, "DODOT_HOMEBREW_UNINSTALL=true")
}

func TestHomebrewHandler_ClearWithUninstall(t *testing.T) {
	// Test clear with uninstall enabled
	_ = os.Setenv("DODOT_HOMEBREW_UNINSTALL", "true")
	defer func() { _ = os.Unsetenv("DODOT_HOMEBREW_UNINSTALL") }()

	handler := homebrew.NewHandler()

	// Create mock store and executor
	store := new(MockSimpleDataStore)
	executor := operations.NewExecutor(store, nil, false)

	// Clear context
	ctx := operations.ClearContext{
		Pack: types.Pack{
			Name: "dev-tools",
		},
		DryRun: true,
	}

	// Execute clear
	clearedItems, err := executor.ExecuteClear(handler, ctx)
	require.NoError(t, err)
	assert.Len(t, clearedItems, 1)

	// Check cleared item reflects uninstall
	item := clearedItems[0]
	assert.Contains(t, item.Description, "Would uninstall Homebrew packages")
}

func TestHomebrewHandler_MultipleBrewfiles(t *testing.T) {
	// Test handling multiple Brewfiles in different directories
	tempDir := t.TempDir()

	// Create multiple Brewfiles
	brewfile1 := filepath.Join(tempDir, "Brewfile")
	err := os.WriteFile(brewfile1, []byte("brew \"git\"\n"), 0644)
	require.NoError(t, err)

	appsDir := filepath.Join(tempDir, "apps")
	err = os.MkdirAll(appsDir, 0755)
	require.NoError(t, err)

	brewfile2 := filepath.Join(appsDir, "Brewfile")
	err = os.WriteFile(brewfile2, []byte("cask \"slack\"\n"), 0644)
	require.NoError(t, err)

	handler := homebrew.NewHandler()

	matches := []operations.FileInput{
		{
			PackName:     "tools",
			RelativePath: "Brewfile",
			SourcePath:   brewfile1,
		},
		{
			PackName:     "tools",
			RelativePath: "apps/Brewfile",
			SourcePath:   brewfile2,
		},
	}

	ops, err := handler.ToOperations(matches, nil)
	require.NoError(t, err)
	assert.Len(t, ops, 2)

	// Each should have unique sentinel
	assert.NotEqual(t, ops[0].Sentinel, ops[1].Sentinel)

	// Both should use brew bundle
	for _, op := range ops {
		assert.Contains(t, op.Command, "brew bundle")
	}
}
