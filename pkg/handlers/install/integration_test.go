package install_test

import (
	"fmt"
	"os"
	"path/filepath"
	"testing"

	"github.com/arthur-debert/dodot/pkg/handlers/install"
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

func TestInstallHandler_OperationIntegration(t *testing.T) {
	// This test verifies the install handler works with the operation system

	// Create test script
	tempDir := t.TempDir()
	scriptContent := "#!/bin/bash\necho 'Installing test pack'\n"
	scriptPath := filepath.Join(tempDir, "install.sh")
	err := os.WriteFile(scriptPath, []byte(scriptContent), 0755)
	require.NoError(t, err)

	// Create simplified handler
	handler := install.NewHandler()

	// Create test matches
	matches := []operations.FileInput{
		{
			PackName:     "testpack",
			RelativePath: "install.sh",
			SourcePath:   scriptPath,
		},
	}

	// Convert to operations
	ops, err := handler.ToOperations(matches)
	require.NoError(t, err)
	assert.Len(t, ops, 1)

	// Verify operation
	op := ops[0]
	assert.Equal(t, operations.RunCommand, op.Type)
	assert.Equal(t, "testpack", op.Pack)
	assert.Equal(t, "install", op.Handler)
	assert.Contains(t, op.Command, scriptPath)
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

func TestInstallHandler_ExecuteWithDataStore(t *testing.T) {
	// This test verifies actual execution with the datastore

	// Create test script
	tempDir := t.TempDir()
	scriptContent := "#!/bin/bash\necho 'Installing test pack'\n"
	scriptPath := filepath.Join(tempDir, "install.sh")
	err := os.WriteFile(scriptPath, []byte(scriptContent), 0755)
	require.NoError(t, err)

	handler := install.NewHandler()

	matches := []operations.FileInput{
		{
			PackName:     "testpack",
			RelativePath: "install.sh",
			SourcePath:   scriptPath,
		},
	}

	ops, err := handler.ToOperations(matches)
	require.NoError(t, err)

	// Create mock store and set expectations
	store := new(MockSimpleDataStore)

	// Expect RunAndRecord to be called with the correct parameters
	expectedCommand := fmt.Sprintf("bash '%s'", scriptPath)
	store.On("RunAndRecord", "testpack", "install", expectedCommand, mock.MatchedBy(func(s string) bool {
		// Sentinel should contain filename and checksum
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

func TestInstallHandler_CheckSentinel(t *testing.T) {
	// Test that CheckSentinel operations work correctly

	handler := install.NewHandler()

	// Create a CheckSentinel operation
	op := operations.Operation{
		Type:     operations.CheckSentinel,
		Pack:     "testpack",
		Handler:  "install",
		Sentinel: "install.sh-abc123",
	}

	// Test when sentinel exists
	store := new(MockSimpleDataStore)

	store.On("HasSentinel", "testpack", "install", "install.sh-abc123").Return(true, nil)

	executor := operations.NewExecutor(store, nil, false)
	results, err := executor.Execute([]operations.Operation{op}, handler)

	require.NoError(t, err)
	assert.Len(t, results, 1)
	assert.True(t, results[0].Success)
	assert.Equal(t, "Already completed", results[0].Message)

	// Test when sentinel doesn't exist
	store2 := new(MockSimpleDataStore)
	store2.On("HasSentinel", "testpack", "install", "install.sh-abc123").Return(false, nil)

	executor2 := operations.NewExecutor(store2, nil, false)
	results2, err := executor2.Execute([]operations.Operation{op}, handler)

	require.NoError(t, err)
	assert.Len(t, results2, 1)
	assert.True(t, results2[0].Success)
	assert.Equal(t, "Not completed", results2[0].Message)
}

func TestInstallHandler_ClearIntegration(t *testing.T) {
	// Test clear functionality
	handler := install.NewHandler()

	// Create mock store and executor
	store := new(MockSimpleDataStore)
	executor := operations.NewExecutor(store, nil, false)

	// Clear context
	ctx := operations.ClearContext{
		Pack: types.Pack{
			Name: "testpack",
		},
		DryRun: true,
	}

	// Execute clear
	clearedItems, err := executor.ExecuteClear(handler, ctx)
	require.NoError(t, err)
	assert.Len(t, clearedItems, 1)

	// Check cleared item
	item := clearedItems[0]
	assert.Equal(t, "provision_state", item.Type)
	assert.Contains(t, item.Path, "testpack/install")
	assert.Equal(t, "Would remove install run records", item.Description)
}

func TestInstallHandler_IdempotentExecution(t *testing.T) {
	// Test that scripts with same content get same sentinel

	// Create test script
	tempDir := t.TempDir()
	scriptContent := "#!/bin/bash\necho 'test'\n"
	scriptPath := filepath.Join(tempDir, "install.sh")
	err := os.WriteFile(scriptPath, []byte(scriptContent), 0755)
	require.NoError(t, err)

	handler := install.NewHandler()

	match := operations.FileInput{
		PackName:     "test",
		RelativePath: "install.sh",
		SourcePath:   scriptPath,
	}

	// Generate operations twice
	ops1, err := handler.ToOperations([]operations.FileInput{match})
	require.NoError(t, err)

	ops2, err := handler.ToOperations([]operations.FileInput{match})
	require.NoError(t, err)

	// Same content should produce same sentinel
	assert.Equal(t, ops1[0].Sentinel, ops2[0].Sentinel)
}
