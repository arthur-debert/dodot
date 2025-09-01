package operations_test

import (
	"fmt"
	"testing"

	"github.com/arthur-debert/dodot/pkg/operations"
	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/mock"
	"github.com/stretchr/testify/require"
)

// MockSimpleDataStore implements operations.SimpleDataStore for testing
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

// MockHandler implements operations.Handler for testing
type MockHandler struct {
	operations.BaseHandler
	mock.Mock
}

func (m *MockHandler) ToOperations(matches []types.RuleMatch) ([]operations.Operation, error) {
	args := m.Called(matches)
	return args.Get(0).([]operations.Operation), args.Error(1)
}

func (m *MockHandler) GetMetadata() operations.HandlerMetadata {
	args := m.Called()
	return args.Get(0).(operations.HandlerMetadata)
}

func (m *MockHandler) ValidateOperations(ops []operations.Operation) error {
	args := m.Called(ops)
	return args.Error(0)
}

// MockConfirmer implements operations.Confirmer for testing
type MockConfirmer struct {
	mock.Mock
}

func (m *MockConfirmer) RequestConfirmation(id, title, description string, items ...string) bool {
	args := m.Called(id, title, description, items)
	return args.Bool(0)
}

func TestExecutor_Execute(t *testing.T) {
	tests := []struct {
		name       string
		operations []operations.Operation
		setupMocks func(*MockSimpleDataStore, *MockHandler)
		wantErr    bool
		checkFunc  func(*testing.T, []operations.OperationResult)
	}{
		{
			name: "execute CreateDataLink operation",
			operations: []operations.Operation{
				{
					Type:    operations.CreateDataLink,
					Pack:    "vim",
					Handler: "symlink",
					Source:  ".vimrc",
				},
			},
			setupMocks: func(store *MockSimpleDataStore, handler *MockHandler) {
				handler.On("ValidateOperations", mock.Anything).Return(nil)
				handler.On("GetMetadata").Return(operations.HandlerMetadata{})
				store.On("CreateDataLink", "vim", "symlink", ".vimrc").
					Return("/home/user/.local/share/dodot/data/vim/symlink/.vimrc", nil)
			},
			wantErr: false,
			checkFunc: func(t *testing.T, results []operations.OperationResult) {
				assert.Len(t, results, 1)
				assert.True(t, results[0].Success)
				assert.Contains(t, results[0].Message, "Created data link")
			},
		},
		{
			name: "execute CreateUserLink operation",
			operations: []operations.Operation{
				{
					Type:    operations.CreateUserLink,
					Pack:    "vim",
					Handler: "symlink",
					Source:  "/datastore/vim/.vimrc",
					Target:  "/home/user/.vimrc",
				},
			},
			setupMocks: func(store *MockSimpleDataStore, handler *MockHandler) {
				handler.On("ValidateOperations", mock.Anything).Return(nil)
				handler.On("GetMetadata").Return(operations.HandlerMetadata{})
				store.On("CreateUserLink", "/datastore/vim/.vimrc", "/home/user/.vimrc").Return(nil)
			},
			wantErr: false,
			checkFunc: func(t *testing.T, results []operations.OperationResult) {
				assert.Len(t, results, 1)
				assert.True(t, results[0].Success)
				assert.Contains(t, results[0].Message, "Created link")
			},
		},
		{
			name: "execute RunCommand operation",
			operations: []operations.Operation{
				{
					Type:     operations.RunCommand,
					Pack:     "tools",
					Handler:  "install",
					Command:  "./install.sh",
					Sentinel: "install-complete",
				},
			},
			setupMocks: func(store *MockSimpleDataStore, handler *MockHandler) {
				handler.On("ValidateOperations", mock.Anything).Return(nil)
				handler.On("GetMetadata").Return(operations.HandlerMetadata{})
				store.On("RunAndRecord", "tools", "install", "./install.sh", "install-complete").Return(nil)
			},
			wantErr: false,
			checkFunc: func(t *testing.T, results []operations.OperationResult) {
				assert.Len(t, results, 1)
				assert.True(t, results[0].Success)
				assert.Contains(t, results[0].Message, "Executed")
			},
		},
		{
			name: "handler validation fails",
			operations: []operations.Operation{
				{
					Type:    operations.CreateUserLink,
					Pack:    "vim",
					Handler: "symlink",
					Target:  "/home/user/.vimrc",
				},
			},
			setupMocks: func(store *MockSimpleDataStore, handler *MockHandler) {
				handler.On("ValidateOperations", mock.Anything).
					Return(fmt.Errorf("multiple files cannot link to same target"))
			},
			wantErr: true,
		},
		{
			name: "dry run simulation",
			operations: []operations.Operation{
				{
					Type:     operations.RunCommand,
					Pack:     "tools",
					Handler:  "homebrew",
					Command:  "brew bundle --file=Brewfile",
					Sentinel: "brewfile-abc123",
				},
			},
			setupMocks: func(store *MockSimpleDataStore, handler *MockHandler) {
				handler.On("ValidateOperations", mock.Anything).Return(nil)
				handler.On("GetMetadata").Return(operations.HandlerMetadata{})
				// No store methods should be called in dry run
			},
			wantErr: false,
			checkFunc: func(t *testing.T, results []operations.OperationResult) {
				assert.Len(t, results, 1)
				assert.True(t, results[0].Success)
				assert.Contains(t, results[0].Message, "Would execute")
			},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			// Setup mocks
			store := new(MockSimpleDataStore)
			handler := new(MockHandler)
			confirmer := new(MockConfirmer)

			// Configure handler base
			handler.BaseHandler = operations.BaseHandler{}

			// Setup test-specific mocks
			tt.setupMocks(store, handler)

			// Determine if dry run based on test name
			dryRun := tt.name == "dry run simulation"

			// Create executor
			// For phase 1, we pass nil for FS as it's not used yet
			executor := operations.NewExecutor(store, nil, confirmer, dryRun)

			// Execute operations
			results, err := executor.Execute(tt.operations, handler)

			// Check error
			if tt.wantErr {
				assert.Error(t, err)
			} else {
				assert.NoError(t, err)
			}

			// Run custom checks
			if tt.checkFunc != nil {
				tt.checkFunc(t, results)
			}

			// Verify all mocks
			store.AssertExpectations(t)
			handler.AssertExpectations(t)
		})
	}
}

func TestExecutor_CheckSentinel(t *testing.T) {
	store := new(MockSimpleDataStore)
	handler := new(MockHandler)
	confirmer := new(MockConfirmer)

	executor := operations.NewExecutor(store, nil, confirmer, false)

	// Test sentinel exists
	store.On("HasSentinel", "tools", "install", "install-complete").Return(true, nil)
	handler.On("ValidateOperations", mock.Anything).Return(nil)
	handler.On("GetMetadata").Return(operations.HandlerMetadata{})

	op := operations.Operation{
		Type:     operations.CheckSentinel,
		Pack:     "tools",
		Handler:  "install",
		Sentinel: "install-complete",
	}

	results, err := executor.Execute([]operations.Operation{op}, handler)
	require.NoError(t, err)
	assert.Len(t, results, 1)
	assert.True(t, results[0].Success)
	assert.Equal(t, "Already completed", results[0].Message)

	store.AssertExpectations(t)
}
