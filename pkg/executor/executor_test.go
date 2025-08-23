package executor_test

import (
	"errors"
	"os"
	"testing"

	"github.com/arthur-debert/dodot/pkg/executor"
	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/rs/zerolog"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/mock"
)

// MockAction implements types.ActionV2 for testing
type MockAction struct {
	mock.Mock
	PackName string
	Desc     string
}

func (m *MockAction) Execute(store types.DataStore) error {
	args := m.Called(store)
	return args.Error(0)
}

func (m *MockAction) Description() string {
	return m.Desc
}

func (m *MockAction) Pack() string {
	return m.PackName
}

// MockFS implements types.FS for testing
type MockFS struct {
	mock.Mock
}

func (m *MockFS) Stat(name string) (os.FileInfo, error) {
	args := m.Called(name)
	if args.Get(0) == nil {
		return nil, args.Error(1)
	}
	return args.Get(0).(os.FileInfo), args.Error(1)
}

func (m *MockFS) Lstat(name string) (os.FileInfo, error) {
	args := m.Called(name)
	if args.Get(0) == nil {
		return nil, args.Error(1)
	}
	return args.Get(0).(os.FileInfo), args.Error(1)
}

func (m *MockFS) ReadFile(name string) ([]byte, error) {
	args := m.Called(name)
	if args.Get(0) == nil {
		return nil, args.Error(1)
	}
	return args.Get(0).([]byte), args.Error(1)
}

func (m *MockFS) WriteFile(name string, data []byte, perm os.FileMode) error {
	args := m.Called(name, data, perm)
	return args.Error(0)
}

func (m *MockFS) MkdirAll(path string, perm os.FileMode) error {
	args := m.Called(path, perm)
	return args.Error(0)
}

func (m *MockFS) Remove(name string) error {
	args := m.Called(name)
	return args.Error(0)
}

func (m *MockFS) RemoveAll(path string) error {
	args := m.Called(path)
	return args.Error(0)
}

func (m *MockFS) Symlink(oldname, newname string) error {
	args := m.Called(oldname, newname)
	return args.Error(0)
}

func (m *MockFS) Readlink(name string) (string, error) {
	args := m.Called(name)
	return args.String(0), args.Error(1)
}

func (m *MockFS) ReadDir(name string) ([]os.DirEntry, error) {
	args := m.Called(name)
	if args.Get(0) == nil {
		return nil, args.Error(1)
	}
	return args.Get(0).([]os.DirEntry), args.Error(1)
}

// MockDataStore is a mock implementation of types.DataStore
type MockDataStore struct {
	mock.Mock
}

func (m *MockDataStore) Link(pack, sourceFile string) (string, error) {
	args := m.Called(pack, sourceFile)
	return args.String(0), args.Error(1)
}

func (m *MockDataStore) Unlink(pack, sourceFile string) error {
	args := m.Called(pack, sourceFile)
	return args.Error(0)
}

func (m *MockDataStore) AddToPath(pack, dirPath string) error {
	args := m.Called(pack, dirPath)
	return args.Error(0)
}

func (m *MockDataStore) AddToShellProfile(pack, scriptPath string) error {
	args := m.Called(pack, scriptPath)
	return args.Error(0)
}

func (m *MockDataStore) RecordProvisioning(pack, sentinelName, checksum string) error {
	args := m.Called(pack, sentinelName, checksum)
	return args.Error(0)
}

func (m *MockDataStore) NeedsProvisioning(pack, sentinelName, checksum string) (bool, error) {
	args := m.Called(pack, sentinelName, checksum)
	return args.Bool(0), args.Error(1)
}

func (m *MockDataStore) GetStatus(pack, sourceFile string) (types.Status, error) {
	args := m.Called(pack, sourceFile)
	return args.Get(0).(types.Status), args.Error(1)
}

func (m *MockDataStore) GetSymlinkStatus(pack, sourceFile string) (types.Status, error) {
	args := m.Called(pack, sourceFile)
	return args.Get(0).(types.Status), args.Error(1)
}

func (m *MockDataStore) GetPathStatus(pack, dirPath string) (types.Status, error) {
	args := m.Called(pack, dirPath)
	return args.Get(0).(types.Status), args.Error(1)
}

func (m *MockDataStore) GetShellProfileStatus(pack, scriptPath string) (types.Status, error) {
	args := m.Called(pack, scriptPath)
	return args.Get(0).(types.Status), args.Error(1)
}

func (m *MockDataStore) GetProvisioningStatus(pack, sentinelName, currentChecksum string) (types.Status, error) {
	args := m.Called(pack, sentinelName, currentChecksum)
	return args.Get(0).(types.Status), args.Error(1)
}

func (m *MockDataStore) GetBrewStatus(pack, brewfilePath, currentChecksum string) (types.Status, error) {
	args := m.Called(pack, brewfilePath, currentChecksum)
	return args.Get(0).(types.Status), args.Error(1)
}

func TestExecutor_Execute(t *testing.T) {
	tests := []struct {
		name         string
		actions      []types.ActionV2
		dryRun       bool
		setupMocks   func(*MockDataStore, *MockFS, []types.ActionV2)
		expectedLen  int
		checkResults func(*testing.T, []types.ActionResultV2)
	}{
		{
			name: "successful execution",
			actions: []types.ActionV2{
				&MockAction{PackName: "test", Desc: "Test action"},
			},
			dryRun: false,
			setupMocks: func(ds *MockDataStore, fs *MockFS, actions []types.ActionV2) {
				mockAction := actions[0].(*MockAction)
				mockAction.On("Execute", ds).Return(nil)
			},
			expectedLen: 1,
			checkResults: func(t *testing.T, results []types.ActionResultV2) {
				assert.True(t, results[0].Success)
				assert.Nil(t, results[0].Error)
				assert.False(t, results[0].Skipped)
			},
		},
		{
			name: "dry run execution",
			actions: []types.ActionV2{
				&MockAction{PackName: "test", Desc: "Test action"},
			},
			dryRun: true,
			setupMocks: func(ds *MockDataStore, fs *MockFS, actions []types.ActionV2) {
				// No execution should happen in dry run
			},
			expectedLen: 1,
			checkResults: func(t *testing.T, results []types.ActionResultV2) {
				assert.True(t, results[0].Success)
				assert.True(t, results[0].Skipped)
				assert.Equal(t, "Dry run - no changes made", results[0].Message)
			},
		},
		{
			name: "failed execution",
			actions: []types.ActionV2{
				&MockAction{PackName: "test", Desc: "Test action"},
			},
			dryRun: false,
			setupMocks: func(ds *MockDataStore, fs *MockFS, actions []types.ActionV2) {
				mockAction := actions[0].(*MockAction)
				mockAction.On("Execute", ds).Return(errors.New("execution failed"))
			},
			expectedLen: 1,
			checkResults: func(t *testing.T, results []types.ActionResultV2) {
				assert.False(t, results[0].Success)
				assert.NotNil(t, results[0].Error)
				assert.Contains(t, results[0].Error.Error(), "execution failed")
			},
		},
		{
			name: "multiple actions",
			actions: []types.ActionV2{
				&MockAction{PackName: "test1", Desc: "Test action 1"},
				&MockAction{PackName: "test2", Desc: "Test action 2"},
			},
			dryRun: false,
			setupMocks: func(ds *MockDataStore, fs *MockFS, actions []types.ActionV2) {
				for _, action := range actions {
					mockAction := action.(*MockAction)
					mockAction.On("Execute", ds).Return(nil)
				}
			},
			expectedLen: 2,
			checkResults: func(t *testing.T, results []types.ActionResultV2) {
				assert.True(t, results[0].Success)
				assert.True(t, results[1].Success)
			},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			mockDS := new(MockDataStore)
			mockFS := new(MockFS)

			if tt.setupMocks != nil {
				tt.setupMocks(mockDS, mockFS, tt.actions)
			}

			exec := executor.New(executor.Options{
				DataStore: mockDS,
				DryRun:    tt.dryRun,
				Logger:    zerolog.Nop(),
				FS:        mockFS,
			})

			results := exec.Execute(tt.actions)

			assert.Len(t, results, tt.expectedLen)
			if tt.checkResults != nil {
				tt.checkResults(t, results)
			}

			mockDS.AssertExpectations(t)
			mockFS.AssertExpectations(t)
		})
	}
}

func TestExecutor_LinkAction(t *testing.T) {
	mockDS := new(MockDataStore)
	mockFS := new(MockFS)

	action := &types.LinkAction{
		PackName:   "vim",
		SourceFile: ".vimrc",
		TargetFile: "~/.vimrc",
	}

	// Setup mocks - Link will be called twice: once in Execute, once in post-execution
	mockDS.On("Link", "vim", ".vimrc").Return("/data/packs/vim/symlinks/.vimrc", nil).Twice()

	// Expect filesystem operations for creating the final symlink
	mockFS.On("MkdirAll", mock.Anything, os.FileMode(0755)).Return(nil)
	mockFS.On("Lstat", mock.Anything).Return(nil, os.ErrNotExist)
	mockFS.On("Symlink", "/data/packs/vim/symlinks/.vimrc", mock.Anything).Return(nil)

	exec := executor.New(executor.Options{
		DataStore: mockDS,
		DryRun:    false,
		Logger:    zerolog.Nop(),
		FS:        mockFS,
	})

	results := exec.Execute([]types.ActionV2{action})

	assert.Len(t, results, 1)
	assert.True(t, results[0].Success)
	assert.Nil(t, results[0].Error)

	mockDS.AssertExpectations(t)
	mockFS.AssertExpectations(t)
}

func TestExecutor_RunScriptAction(t *testing.T) {
	t.Run("script needs provisioning", func(t *testing.T) {
		mockDS := new(MockDataStore)
		mockFS := new(MockFS)

		action := &types.RunScriptAction{
			PackName:     "dev",
			ScriptPath:   "/path/to/install.sh",
			Checksum:     "sha256:12345",
			SentinelName: "install.sh.sentinel",
		}

		// Setup mocks - script needs provisioning
		mockDS.On("NeedsProvisioning", "dev", "install.sh.sentinel", "sha256:12345").
			Return(true, nil).Twice() // Called in action and post-execution

		// Note: Script execution will fail because the path doesn't exist
		// This is expected in unit tests

		exec := executor.New(executor.Options{
			DataStore: mockDS,
			DryRun:    false,
			Logger:    zerolog.Nop(),
			FS:        mockFS,
		})

		results := exec.Execute([]types.ActionV2{action})

		assert.Len(t, results, 1)
		assert.False(t, results[0].Success) // Expected to fail due to missing script
		assert.Error(t, results[0].Error)
		assert.Contains(t, results[0].Error.Error(), "no such file or directory")

		// RecordProvisioning should not be called due to failure
		mockDS.AssertExpectations(t)
	})

	t.Run("script already provisioned", func(t *testing.T) {
		mockDS := new(MockDataStore)
		mockFS := new(MockFS)

		action := &types.RunScriptAction{
			PackName:     "dev",
			ScriptPath:   "/path/to/install.sh",
			Checksum:     "sha256:12345",
			SentinelName: "install.sh.sentinel",
		}

		// Setup mocks - script already provisioned
		mockDS.On("NeedsProvisioning", "dev", "install.sh.sentinel", "sha256:12345").
			Return(false, nil)

		exec := executor.New(executor.Options{
			DataStore: mockDS,
			DryRun:    false,
			Logger:    zerolog.Nop(),
			FS:        mockFS,
		})

		results := exec.Execute([]types.ActionV2{action})

		assert.Len(t, results, 1)
		assert.True(t, results[0].Success)
		assert.Nil(t, results[0].Error)

		mockDS.AssertExpectations(t)
	})
}
