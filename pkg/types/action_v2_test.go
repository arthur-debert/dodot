package types_test

import (
	"testing"

	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/mock"
)

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

func TestLinkAction_Execute(t *testing.T) {
	tests := []struct {
		name           string
		action         *types.LinkAction
		mockSetup      func(*MockDataStore)
		expectedErr    bool
		expectedErrMsg string
	}{
		{
			name: "successful link",
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
			name: "link failure",
			action: &types.LinkAction{
				PackName:   "vim",
				SourceFile: "/home/user/dotfiles/vim/.vimrc",
				TargetFile: "/home/user/.vimrc",
			},
			mockSetup: func(m *MockDataStore) {
				m.On("Link", "vim", "/home/user/dotfiles/vim/.vimrc").
					Return("", assert.AnError)
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

func TestLinkAction_Properties(t *testing.T) {
	action := &types.LinkAction{
		PackName:   "vim",
		SourceFile: ".vimrc",
		TargetFile: "~/.vimrc",
	}

	assert.Equal(t, "vim", action.Pack())
	assert.Equal(t, "Link .vimrc to ~/.vimrc", action.Description())

	// Verify it implements LinkingAction
	var _ types.LinkingAction = action
}

func TestUnlinkAction_Execute(t *testing.T) {
	tests := []struct {
		name        string
		action      *types.UnlinkAction
		mockSetup   func(*MockDataStore)
		expectedErr bool
	}{
		{
			name: "successful unlink",
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
			name: "unlink failure",
			action: &types.UnlinkAction{
				PackName:   "vim",
				SourceFile: ".vimrc",
			},
			mockSetup: func(m *MockDataStore) {
				m.On("Unlink", "vim", ".vimrc").Return(assert.AnError)
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

func TestAddToPathAction_Execute(t *testing.T) {
	action := &types.AddToPathAction{
		PackName: "tools",
		DirPath:  "/home/user/dotfiles/tools/bin",
	}

	mockStore := new(MockDataStore)
	mockStore.On("AddToPath", "tools", "/home/user/dotfiles/tools/bin").Return(nil)

	err := action.Execute(mockStore)
	assert.NoError(t, err)
	assert.Equal(t, "tools", action.Pack())
	assert.Equal(t, "Add /home/user/dotfiles/tools/bin to PATH", action.Description())

	mockStore.AssertExpectations(t)
}

func TestAddToShellProfileAction_Execute(t *testing.T) {
	action := &types.AddToShellProfileAction{
		PackName:   "shell",
		ScriptPath: "/home/user/dotfiles/shell/aliases.sh",
	}

	mockStore := new(MockDataStore)
	mockStore.On("AddToShellProfile", "shell", "/home/user/dotfiles/shell/aliases.sh").Return(nil)

	err := action.Execute(mockStore)
	assert.NoError(t, err)
	assert.Equal(t, "shell", action.Pack())
	assert.Equal(t, "Add /home/user/dotfiles/shell/aliases.sh to shell profile", action.Description())

	mockStore.AssertExpectations(t)
}

func TestRunScriptAction_Execute(t *testing.T) {
	tests := []struct {
		name           string
		action         *types.RunScriptAction
		mockSetup      func(*MockDataStore)
		expectedErr    bool
		expectedErrMsg string
	}{
		{
			name: "needs provisioning",
			action: &types.RunScriptAction{
				PackName:     "dev",
				ScriptPath:   "/home/user/dotfiles/dev/install.sh",
				Checksum:     "sha256:12345",
				SentinelName: "install.sh.sentinel",
			},
			mockSetup: func(m *MockDataStore) {
				m.On("NeedsProvisioning", "dev", "install.sh.sentinel", "sha256:12345").
					Return(true, nil)
			},
			expectedErr: false,
		},
		{
			name: "already provisioned",
			action: &types.RunScriptAction{
				PackName:     "dev",
				ScriptPath:   "/home/user/dotfiles/dev/install.sh",
				Checksum:     "sha256:12345",
				SentinelName: "install.sh.sentinel",
			},
			mockSetup: func(m *MockDataStore) {
				m.On("NeedsProvisioning", "dev", "install.sh.sentinel", "sha256:12345").
					Return(false, nil)
			},
			expectedErr: false,
		},
		{
			name: "check provisioning fails",
			action: &types.RunScriptAction{
				PackName:     "dev",
				ScriptPath:   "/home/user/dotfiles/dev/install.sh",
				Checksum:     "sha256:12345",
				SentinelName: "install.sh.sentinel",
			},
			mockSetup: func(m *MockDataStore) {
				m.On("NeedsProvisioning", "dev", "install.sh.sentinel", "sha256:12345").
					Return(false, assert.AnError)
			},
			expectedErr:    true,
			expectedErrMsg: "failed to check provisioning status",
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

func TestRunScriptAction_Properties(t *testing.T) {
	action := &types.RunScriptAction{
		PackName:     "dev",
		ScriptPath:   "install.sh",
		Checksum:     "sha256:12345",
		SentinelName: "install.sh.sentinel",
	}

	assert.Equal(t, "dev", action.Pack())
	assert.Equal(t, "Run provisioning script install.sh", action.Description())

	// Verify it implements ProvisioningAction
	var _ types.ProvisioningAction = action
}

func TestRecordProvisioningAction_Execute(t *testing.T) {
	action := &types.RecordProvisioningAction{
		PackName:     "dev",
		SentinelName: "install.sh.sentinel",
		Checksum:     "sha256:12345",
	}

	mockStore := new(MockDataStore)
	mockStore.On("RecordProvisioning", "dev", "install.sh.sentinel", "sha256:12345").Return(nil)

	err := action.Execute(mockStore)
	assert.NoError(t, err)
	assert.Equal(t, "dev", action.Pack())
	assert.Equal(t, "Record provisioning complete for install.sh.sentinel", action.Description())

	mockStore.AssertExpectations(t)
}

// Test that all actions implement the correct interfaces
func TestActionInterfaces(t *testing.T) {
	// Linking actions
	var _ types.LinkingAction = &types.LinkAction{}
	var _ types.LinkingAction = &types.UnlinkAction{}
	var _ types.LinkingAction = &types.AddToPathAction{}
	var _ types.LinkingAction = &types.AddToShellProfileAction{}

	// Provisioning actions
	var _ types.ProvisioningAction = &types.RunScriptAction{}
	var _ types.ProvisioningAction = &types.RecordProvisioningAction{}

	// All implement base ActionV2 interface
	var _ types.ActionV2 = &types.LinkAction{}
	var _ types.ActionV2 = &types.UnlinkAction{}
	var _ types.ActionV2 = &types.AddToPathAction{}
	var _ types.ActionV2 = &types.AddToShellProfileAction{}
	var _ types.ActionV2 = &types.RunScriptAction{}
	var _ types.ActionV2 = &types.RecordProvisioningAction{}
}
