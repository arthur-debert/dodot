// pkg/types/action_test.go
// TEST TYPE: Unit Tests
// DEPENDENCIES: Mock DataStore
// PURPOSE: Test action types and their Execute methods

package types_test

import (
	"errors"
	"testing"

	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/mock"
)

// MockDataStore is a mock implementation of types.DataStore for testing
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

func (m *MockDataStore) DeleteProvisioningState(packName, handlerName string) error {
	args := m.Called(packName, handlerName)
	return args.Error(0)
}

func (m *MockDataStore) GetProvisioningHandlers(packName string) ([]string, error) {
	args := m.Called(packName)
	return args.Get(0).([]string), args.Error(1)
}

func (m *MockDataStore) ListProvisioningState(packName string) (map[string][]string, error) {
	args := m.Called(packName)
	return args.Get(0).(map[string][]string), args.Error(1)
}

// TestLinkAction_Execute tests LinkAction execution
func TestLinkAction_Execute(t *testing.T) {
	tests := []struct {
		name           string
		action         *types.LinkAction
		mockSetup      func(*MockDataStore)
		expectedErr    bool
		expectedErrMsg string
	}{
		{
			name: "successful_link",
			action: &types.LinkAction{
				PackName:   "vim",
				SourceFile: "/home/user/dotfiles/vim/.vimrc",
				TargetFile: "/home/user/.vimrc",
			},
			mockSetup: func(m *MockDataStore) {
				m.On("Link", "vim", "/home/user/dotfiles/vim/.vimrc").
					Return("/home/user/.local/share/dodot/links/vim/.vimrc", nil)
			},
			expectedErr: false,
		},
		{
			name: "link_failure",
			action: &types.LinkAction{
				PackName:   "vim",
				SourceFile: "/home/user/dotfiles/vim/.vimrc",
				TargetFile: "/home/user/.vimrc",
			},
			mockSetup: func(m *MockDataStore) {
				m.On("Link", "vim", "/home/user/dotfiles/vim/.vimrc").
					Return("", errors.New("permission denied"))
			},
			expectedErr:    true,
			expectedErrMsg: "failed to create intermediate link",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			mockStore := new(MockDataStore)
			tt.mockSetup(mockStore)

			err := tt.action.Execute(mockStore)

			if tt.expectedErr {
				assert.Error(t, err)
				assert.Contains(t, err.Error(), tt.expectedErrMsg)
			} else {
				assert.NoError(t, err)
			}

			mockStore.AssertExpectations(t)
		})
	}
}

// TestLinkAction_Properties tests LinkAction properties
func TestLinkAction_Properties(t *testing.T) {
	action := &types.LinkAction{
		PackName:   "vim",
		SourceFile: ".vimrc",
		TargetFile: "~/.vimrc",
	}

	assert.Equal(t, "vim", action.Pack())
	assert.Equal(t, "Link .vimrc to ~/.vimrc", action.Description())

	// Verify it implements LinkingAction interface
	var _ types.LinkingAction = action
}

// TestUnlinkAction_Execute tests UnlinkAction execution
func TestUnlinkAction_Execute(t *testing.T) {
	tests := []struct {
		name        string
		action      *types.UnlinkAction
		mockSetup   func(*MockDataStore)
		expectedErr bool
	}{
		{
			name: "successful_unlink",
			action: &types.UnlinkAction{
				PackName:   "vim",
				SourceFile: ".vimrc",
			},
			mockSetup: func(m *MockDataStore) {
				m.On("Unlink", "vim", ".vimrc").Return(nil)
			},
			expectedErr: false,
		},
		{
			name: "unlink_failure",
			action: &types.UnlinkAction{
				PackName:   "vim",
				SourceFile: ".vimrc",
			},
			mockSetup: func(m *MockDataStore) {
				m.On("Unlink", "vim", ".vimrc").Return(errors.New("not found"))
			},
			expectedErr: true,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			mockStore := new(MockDataStore)
			tt.mockSetup(mockStore)

			err := tt.action.Execute(mockStore)

			if tt.expectedErr {
				assert.Error(t, err)
			} else {
				assert.NoError(t, err)
			}

			mockStore.AssertExpectations(t)
		})
	}
}

// TestUnlinkAction_Properties tests UnlinkAction properties
func TestUnlinkAction_Properties(t *testing.T) {
	action := &types.UnlinkAction{
		PackName:   "vim",
		SourceFile: ".vimrc",
	}

	assert.Equal(t, "vim", action.Pack())
	assert.Equal(t, "Unlink .vimrc", action.Description())

	// Verify it implements LinkingAction interface
	var _ types.LinkingAction = action
}

// TestAddToPathAction_Execute tests AddToPathAction execution
func TestAddToPathAction_Execute(t *testing.T) {
	tests := []struct {
		name        string
		action      *types.AddToPathAction
		mockSetup   func(*MockDataStore)
		expectedErr bool
	}{
		{
			name: "successful_add_to_path",
			action: &types.AddToPathAction{
				PackName: "tools",
				DirPath:  "/home/user/tools/bin",
			},
			mockSetup: func(m *MockDataStore) {
				m.On("AddToPath", "tools", "/home/user/tools/bin").Return(nil)
			},
			expectedErr: false,
		},
		{
			name: "add_to_path_failure",
			action: &types.AddToPathAction{
				PackName: "tools",
				DirPath:  "/home/user/tools/bin",
			},
			mockSetup: func(m *MockDataStore) {
				m.On("AddToPath", "tools", "/home/user/tools/bin").
					Return(errors.New("failed to add"))
			},
			expectedErr: true,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			mockStore := new(MockDataStore)
			tt.mockSetup(mockStore)

			err := tt.action.Execute(mockStore)

			if tt.expectedErr {
				assert.Error(t, err)
			} else {
				assert.NoError(t, err)
			}

			mockStore.AssertExpectations(t)
		})
	}
}

// TestAddToPathAction_Properties tests AddToPathAction properties
func TestAddToPathAction_Properties(t *testing.T) {
	action := &types.AddToPathAction{
		PackName: "tools",
		DirPath:  "bin",
	}

	assert.Equal(t, "tools", action.Pack())
	assert.Equal(t, "Add bin to PATH", action.Description())

	// Verify it implements LinkingAction interface
	var _ types.LinkingAction = action
}

// TestAddToShellProfileAction_Execute tests AddToShellProfileAction execution
func TestAddToShellProfileAction_Execute(t *testing.T) {
	tests := []struct {
		name        string
		action      *types.AddToShellProfileAction
		mockSetup   func(*MockDataStore)
		expectedErr bool
	}{
		{
			name: "successful_add_to_shell_profile",
			action: &types.AddToShellProfileAction{
				PackName:   "git",
				ScriptPath: "/home/user/dotfiles/git/aliases.sh",
			},
			mockSetup: func(m *MockDataStore) {
				m.On("AddToShellProfile", "git", "/home/user/dotfiles/git/aliases.sh").Return(nil)
			},
			expectedErr: false,
		},
		{
			name: "add_to_shell_profile_failure",
			action: &types.AddToShellProfileAction{
				PackName:   "git",
				ScriptPath: "/home/user/dotfiles/git/aliases.sh",
			},
			mockSetup: func(m *MockDataStore) {
				m.On("AddToShellProfile", "git", "/home/user/dotfiles/git/aliases.sh").
					Return(errors.New("failed"))
			},
			expectedErr: true,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			mockStore := new(MockDataStore)
			tt.mockSetup(mockStore)

			err := tt.action.Execute(mockStore)

			if tt.expectedErr {
				assert.Error(t, err)
			} else {
				assert.NoError(t, err)
			}

			mockStore.AssertExpectations(t)
		})
	}
}

// TestAddToShellProfileAction_Properties tests AddToShellProfileAction properties
func TestAddToShellProfileAction_Properties(t *testing.T) {
	action := &types.AddToShellProfileAction{
		PackName:   "git",
		ScriptPath: "aliases.sh",
	}

	assert.Equal(t, "git", action.Pack())
	assert.Equal(t, "Add aliases.sh to shell profile", action.Description())

	// Verify it implements LinkingAction interface
	var _ types.LinkingAction = action
}

// TestRunScriptAction_Execute tests RunScriptAction execution
func TestRunScriptAction_Execute(t *testing.T) {
	tests := []struct {
		name        string
		action      *types.RunScriptAction
		mockSetup   func(*MockDataStore)
		expectedErr bool
	}{
		{
			name: "already_provisioned",
			action: &types.RunScriptAction{
				PackName:     "tools",
				ScriptPath:   "./install.sh",
				Checksum:     "sha256:12345",
				SentinelName: "install.sh.sentinel",
			},
			mockSetup: func(m *MockDataStore) {
				m.On("NeedsProvisioning", "tools", "install.sh.sentinel", "sha256:12345").
					Return(false, nil)
			},
			expectedErr: false,
		},
		{
			name: "needs_provisioning",
			action: &types.RunScriptAction{
				PackName:     "tools",
				ScriptPath:   "./install.sh",
				Checksum:     "sha256:12345",
				SentinelName: "install.sh.sentinel",
			},
			mockSetup: func(m *MockDataStore) {
				m.On("NeedsProvisioning", "tools", "install.sh.sentinel", "sha256:12345").
					Return(true, nil)
			},
			expectedErr: false, // Action itself doesn't run the script
		},
		{
			name: "check_provisioning_error",
			action: &types.RunScriptAction{
				PackName:     "tools",
				ScriptPath:   "./install.sh",
				Checksum:     "sha256:12345",
				SentinelName: "install.sh.sentinel",
			},
			mockSetup: func(m *MockDataStore) {
				m.On("NeedsProvisioning", "tools", "install.sh.sentinel", "sha256:12345").
					Return(false, errors.New("check failed"))
			},
			expectedErr: true,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			mockStore := new(MockDataStore)
			tt.mockSetup(mockStore)

			err := tt.action.Execute(mockStore)

			if tt.expectedErr {
				assert.Error(t, err)
			} else {
				assert.NoError(t, err)
			}

			mockStore.AssertExpectations(t)
		})
	}
}

// TestRunScriptAction_Properties tests RunScriptAction properties
func TestRunScriptAction_Properties(t *testing.T) {
	action := &types.RunScriptAction{
		PackName:     "tools",
		ScriptPath:   "./install.sh",
		Checksum:     "sha256:12345",
		SentinelName: "install.sh.sentinel",
	}

	assert.Equal(t, "tools", action.Pack())
	assert.Equal(t, "Run provisioning script ./install.sh", action.Description())

	// Verify it implements ProvisioningAction interface
	var _ types.ProvisioningAction = action
}

// TestRecordProvisioningAction_Execute tests RecordProvisioningAction
func TestRecordProvisioningAction_Execute(t *testing.T) {
	action := &types.RecordProvisioningAction{
		PackName:     "tools",
		SentinelName: "install.sh.sentinel",
		Checksum:     "sha256:12345",
	}

	mockStore := new(MockDataStore)
	mockStore.On("RecordProvisioning", "tools", "install.sh.sentinel", "sha256:12345").Return(nil)

	err := action.Execute(mockStore)
	assert.NoError(t, err)

	assert.Equal(t, "tools", action.Pack())
	assert.Equal(t, "Record provisioning complete for install.sh.sentinel", action.Description())

	// Verify it implements ProvisioningAction interface
	var _ types.ProvisioningAction = action

	mockStore.AssertExpectations(t)
}

// TestBrewAction_Execute tests BrewAction execution
func TestBrewAction_Execute(t *testing.T) {
	tests := []struct {
		name        string
		action      *types.BrewAction
		mockSetup   func(*MockDataStore)
		expectedErr bool
	}{
		{
			name: "already_installed",
			action: &types.BrewAction{
				PackName:     "dev",
				BrewfilePath: "/home/user/dotfiles/dev/Brewfile",
				Checksum:     "sha256:67890",
			},
			mockSetup: func(m *MockDataStore) {
				m.On("NeedsProvisioning", "dev", "homebrew-dev.sentinel", "sha256:67890").
					Return(false, nil)
			},
			expectedErr: false,
		},
		{
			name: "needs_installation",
			action: &types.BrewAction{
				PackName:     "dev",
				BrewfilePath: "/home/user/dotfiles/dev/Brewfile",
				Checksum:     "sha256:67890",
			},
			mockSetup: func(m *MockDataStore) {
				m.On("NeedsProvisioning", "dev", "homebrew-dev.sentinel", "sha256:67890").
					Return(true, nil)
			},
			expectedErr: false, // Action itself doesn't run brew
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			mockStore := new(MockDataStore)
			tt.mockSetup(mockStore)

			err := tt.action.Execute(mockStore)

			if tt.expectedErr {
				assert.Error(t, err)
			} else {
				assert.NoError(t, err)
			}

			mockStore.AssertExpectations(t)
		})
	}
}

// TestBrewAction_Properties tests BrewAction properties
func TestBrewAction_Properties(t *testing.T) {
	action := &types.BrewAction{
		PackName:     "dev",
		BrewfilePath: "/home/user/dotfiles/dev/Brewfile",
		Checksum:     "sha256:67890",
	}

	assert.Equal(t, "dev", action.Pack())
	assert.Equal(t, "Install Homebrew packages from /home/user/dotfiles/dev/Brewfile", action.Description())

	// Verify it implements ProvisioningAction interface
	var _ types.ProvisioningAction = action
}

