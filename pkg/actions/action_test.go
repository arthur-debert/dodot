// Test Type: Unit Test
// Description: Tests for the actions package - pure logic tests with no filesystem or external dependencies

package actions_test

import (
	"testing"

	"github.com/arthur-debert/dodot/pkg/actions"
	"github.com/arthur-debert/dodot/pkg/testutil"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestLinkAction_Execute(t *testing.T) {
	tests := []struct {
		name       string
		action     *actions.LinkAction
		setupMock  func(*testutil.MockDataStore)
		wantErr    bool
		wantErrMsg string
	}{
		{
			name: "successful_link",
			action: &actions.LinkAction{
				PackName:   "vim",
				SourceFile: "/home/user/dotfiles/vim/.vimrc",
				TargetFile: "/home/user/.vimrc",
			},
			setupMock: func(m *testutil.MockDataStore) {
				// Mock returns success
			},
			wantErr: false,
		},
		{
			name: "link_failure_returns_wrapped_error",
			action: &actions.LinkAction{
				PackName:   "vim",
				SourceFile: "/home/user/dotfiles/vim/.vimrc",
				TargetFile: "/home/user/.vimrc",
			},
			setupMock: func(m *testutil.MockDataStore) {
				m.WithError("Link", assert.AnError)
			},
			wantErr:    true,
			wantErrMsg: "failed to create intermediate link",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			mockStore := testutil.NewMockDataStore()
			if tt.setupMock != nil {
				tt.setupMock(mockStore)
			}

			err := tt.action.Execute(mockStore)

			if tt.wantErr {
				require.Error(t, err)
				assert.Contains(t, err.Error(), tt.wantErrMsg)
			} else {
				require.NoError(t, err)
			}

			// Verify the correct method was called
			calls := mockStore.GetCalls()
			assert.Contains(t, calls[0], "Link(vim,/home/user/dotfiles/vim/.vimrc)")
		})
	}
}

func TestLinkAction_Properties(t *testing.T) {
	action := &actions.LinkAction{
		PackName:   "vim",
		SourceFile: ".vimrc",
		TargetFile: "~/.vimrc",
	}

	assert.Equal(t, "vim", action.Pack())
	assert.Equal(t, "Link .vimrc to ~/.vimrc", action.Description())

	// Verify it implements LinkingAction
	var _ actions.LinkingAction = action
}

func TestUnlinkAction_Execute(t *testing.T) {
	tests := []struct {
		name      string
		action    *actions.UnlinkAction
		setupMock func(*testutil.MockDataStore)
		wantErr   bool
	}{
		{
			name: "successful_unlink",
			action: &actions.UnlinkAction{
				PackName:   "vim",
				SourceFile: ".vimrc",
			},
			setupMock: func(m *testutil.MockDataStore) {
				// Mock returns success
			},
			wantErr: false,
		},
		{
			name: "unlink_failure",
			action: &actions.UnlinkAction{
				PackName:   "vim",
				SourceFile: ".vimrc",
			},
			setupMock: func(m *testutil.MockDataStore) {
				m.WithError("Unlink", assert.AnError)
			},
			wantErr: true,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			mockStore := testutil.NewMockDataStore()
			if tt.setupMock != nil {
				tt.setupMock(mockStore)
			}

			err := tt.action.Execute(mockStore)

			if tt.wantErr {
				require.Error(t, err)
			} else {
				require.NoError(t, err)
			}

			// Verify the correct method was called
			calls := mockStore.GetCalls()
			assert.Contains(t, calls[0], "Unlink(vim,.vimrc)")
		})
	}
}

func TestUnlinkAction_Properties(t *testing.T) {
	action := &actions.UnlinkAction{
		PackName:   "vim",
		SourceFile: ".vimrc",
	}

	assert.Equal(t, "vim", action.Pack())
	assert.Equal(t, "Unlink .vimrc", action.Description())

	// Verify it implements LinkingAction
	var _ actions.LinkingAction = action
}

func TestAddToPathAction_Execute(t *testing.T) {
	tests := []struct {
		name      string
		action    *actions.AddToPathAction
		setupMock func(*testutil.MockDataStore)
		wantErr   bool
	}{
		{
			name: "successful_add_to_path",
			action: &actions.AddToPathAction{
				PackName: "tools",
				DirPath:  "/home/user/dotfiles/tools/bin",
			},
			setupMock: func(m *testutil.MockDataStore) {
				// Mock returns success
			},
			wantErr: false,
		},
		{
			name: "add_to_path_failure",
			action: &actions.AddToPathAction{
				PackName: "tools",
				DirPath:  "/home/user/dotfiles/tools/bin",
			},
			setupMock: func(m *testutil.MockDataStore) {
				m.WithError("AddToPath", assert.AnError)
			},
			wantErr: true,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			mockStore := testutil.NewMockDataStore()
			if tt.setupMock != nil {
				tt.setupMock(mockStore)
			}

			err := tt.action.Execute(mockStore)

			if tt.wantErr {
				require.Error(t, err)
			} else {
				require.NoError(t, err)
			}

			// Verify the correct method was called
			calls := mockStore.GetCalls()
			assert.Contains(t, calls[0], "AddToPath(tools,/home/user/dotfiles/tools/bin)")
		})
	}
}

func TestAddToPathAction_Properties(t *testing.T) {
	action := &actions.AddToPathAction{
		PackName: "tools",
		DirPath:  "/home/user/dotfiles/tools/bin",
	}

	assert.Equal(t, "tools", action.Pack())
	assert.Equal(t, "Add /home/user/dotfiles/tools/bin to PATH", action.Description())

	// Verify it implements LinkingAction
	var _ actions.LinkingAction = action
}

func TestAddToShellProfileAction_Execute(t *testing.T) {
	tests := []struct {
		name      string
		action    *actions.AddToShellProfileAction
		setupMock func(*testutil.MockDataStore)
		wantErr   bool
	}{
		{
			name: "successful_add_to_shell_profile",
			action: &actions.AddToShellProfileAction{
				PackName:   "shell",
				ScriptPath: "/home/user/dotfiles/shell/aliases.sh",
			},
			setupMock: func(m *testutil.MockDataStore) {
				// Mock returns success
			},
			wantErr: false,
		},
		{
			name: "add_to_shell_profile_failure",
			action: &actions.AddToShellProfileAction{
				PackName:   "shell",
				ScriptPath: "/home/user/dotfiles/shell/aliases.sh",
			},
			setupMock: func(m *testutil.MockDataStore) {
				m.WithError("AddToShellProfile", assert.AnError)
			},
			wantErr: true,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			mockStore := testutil.NewMockDataStore()
			if tt.setupMock != nil {
				tt.setupMock(mockStore)
			}

			err := tt.action.Execute(mockStore)

			if tt.wantErr {
				require.Error(t, err)
			} else {
				require.NoError(t, err)
			}

			// Verify the correct method was called
			calls := mockStore.GetCalls()
			assert.Contains(t, calls[0], "AddToShellProfile(shell,/home/user/dotfiles/shell/aliases.sh)")
		})
	}
}

func TestAddToShellProfileAction_Properties(t *testing.T) {
	action := &actions.AddToShellProfileAction{
		PackName:   "shell",
		ScriptPath: "/home/user/dotfiles/shell/aliases.sh",
	}

	assert.Equal(t, "shell", action.Pack())
	assert.Equal(t, "Add /home/user/dotfiles/shell/aliases.sh to shell profile", action.Description())

	// Verify it implements LinkingAction
	var _ actions.LinkingAction = action
}

func TestRunScriptAction_Execute(t *testing.T) {
	tests := []struct {
		name       string
		action     *actions.RunScriptAction
		setupMock  func(*testutil.MockDataStore)
		wantErr    bool
		wantErrMsg string
	}{
		{
			name: "needs_provisioning_returns_no_error",
			action: &actions.RunScriptAction{
				PackName:     "dev",
				ScriptPath:   "/home/user/dotfiles/dev/install.sh",
				Checksum:     "sha256:12345",
				SentinelName: "install.sh.sentinel",
			},
			setupMock: func(m *testutil.MockDataStore) {
				// Mock returns true (needs provisioning)
			},
			wantErr: false,
		},
		{
			name: "already_provisioned_returns_no_error",
			action: &actions.RunScriptAction{
				PackName:     "dev",
				ScriptPath:   "/home/user/dotfiles/dev/install.sh",
				Checksum:     "sha256:12345",
				SentinelName: "install.sh.sentinel",
			},
			setupMock: func(m *testutil.MockDataStore) {
				// Set up existing provisioning state
				m.WithProvisioningState("dev", "install.sh.sentinel", true)
			},
			wantErr: false,
		},
		{
			name: "check_provisioning_fails",
			action: &actions.RunScriptAction{
				PackName:     "dev",
				ScriptPath:   "/home/user/dotfiles/dev/install.sh",
				Checksum:     "sha256:12345",
				SentinelName: "install.sh.sentinel",
			},
			setupMock: func(m *testutil.MockDataStore) {
				m.WithError("NeedsProvisioning", assert.AnError)
			},
			wantErr:    true,
			wantErrMsg: "failed to check provisioning status",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			mockStore := testutil.NewMockDataStore()
			if tt.setupMock != nil {
				tt.setupMock(mockStore)
			}

			err := tt.action.Execute(mockStore)

			if tt.wantErr {
				require.Error(t, err)
				assert.Contains(t, err.Error(), tt.wantErrMsg)
			} else {
				require.NoError(t, err)
			}

			// Verify the correct method was called
			calls := mockStore.GetCalls()
			assert.Contains(t, calls[0], "NeedsProvisioning(dev,install.sh.sentinel,sha256:12345)")
		})
	}
}

func TestRunScriptAction_Properties(t *testing.T) {
	action := &actions.RunScriptAction{
		PackName:     "dev",
		ScriptPath:   "install.sh",
		Checksum:     "sha256:12345",
		SentinelName: "install.sh.sentinel",
	}

	assert.Equal(t, "dev", action.Pack())
	assert.Equal(t, "Run provisioning script install.sh", action.Description())

	// Verify it implements ProvisioningAction
	var _ actions.ProvisioningAction = action
}

func TestRecordProvisioningAction_Execute(t *testing.T) {
	tests := []struct {
		name      string
		action    *actions.RecordProvisioningAction
		setupMock func(*testutil.MockDataStore)
		wantErr   bool
	}{
		{
			name: "successful_record_provisioning",
			action: &actions.RecordProvisioningAction{
				PackName:     "dev",
				SentinelName: "install.sh.sentinel",
				Checksum:     "sha256:12345",
			},
			setupMock: func(m *testutil.MockDataStore) {
				// Mock returns success
			},
			wantErr: false,
		},
		{
			name: "record_provisioning_failure",
			action: &actions.RecordProvisioningAction{
				PackName:     "dev",
				SentinelName: "install.sh.sentinel",
				Checksum:     "sha256:12345",
			},
			setupMock: func(m *testutil.MockDataStore) {
				m.WithError("RecordProvisioning", assert.AnError)
			},
			wantErr: true,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			mockStore := testutil.NewMockDataStore()
			if tt.setupMock != nil {
				tt.setupMock(mockStore)
			}

			err := tt.action.Execute(mockStore)

			if tt.wantErr {
				require.Error(t, err)
			} else {
				require.NoError(t, err)
			}

			// Verify the correct method was called
			calls := mockStore.GetCalls()
			assert.Contains(t, calls[0], "RecordProvisioning(dev,install.sh.sentinel,sha256:12345)")
		})
	}
}

func TestRecordProvisioningAction_Properties(t *testing.T) {
	action := &actions.RecordProvisioningAction{
		PackName:     "dev",
		SentinelName: "install.sh.sentinel",
		Checksum:     "sha256:12345",
	}

	assert.Equal(t, "dev", action.Pack())
	assert.Equal(t, "Record provisioning complete for install.sh.sentinel", action.Description())

	// Verify it implements ProvisioningAction
	var _ actions.ProvisioningAction = action
}

func TestBrewAction_Execute(t *testing.T) {
	tests := []struct {
		name       string
		action     *actions.BrewAction
		setupMock  func(*testutil.MockDataStore)
		wantErr    bool
		wantErrMsg string
	}{
		{
			name: "needs_provisioning_returns_no_error",
			action: &actions.BrewAction{
				PackName:     "dev",
				BrewfilePath: "/home/user/dotfiles/dev/Brewfile",
				Checksum:     "sha256:12345",
			},
			setupMock: func(m *testutil.MockDataStore) {
				// Mock returns true (needs provisioning)
			},
			wantErr: false,
		},
		{
			name: "already_provisioned_returns_no_error",
			action: &actions.BrewAction{
				PackName:     "dev",
				BrewfilePath: "/home/user/dotfiles/dev/Brewfile",
				Checksum:     "sha256:12345",
			},
			setupMock: func(m *testutil.MockDataStore) {
				// Set up existing provisioning state
				m.WithProvisioningState("dev", "homebrew-dev.sentinel", true)
			},
			wantErr: false,
		},
		{
			name: "check_provisioning_fails",
			action: &actions.BrewAction{
				PackName:     "dev",
				BrewfilePath: "/home/user/dotfiles/dev/Brewfile",
				Checksum:     "sha256:12345",
			},
			setupMock: func(m *testutil.MockDataStore) {
				m.WithError("NeedsProvisioning", assert.AnError)
			},
			wantErr:    true,
			wantErrMsg: "failed to check provisioning status",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			mockStore := testutil.NewMockDataStore()
			if tt.setupMock != nil {
				tt.setupMock(mockStore)
			}

			err := tt.action.Execute(mockStore)

			if tt.wantErr {
				require.Error(t, err)
				assert.Contains(t, err.Error(), tt.wantErrMsg)
			} else {
				require.NoError(t, err)
			}

			// Verify the correct method was called
			calls := mockStore.GetCalls()
			assert.Contains(t, calls[0], "NeedsProvisioning(dev,homebrew-dev.sentinel,sha256:12345)")
		})
	}
}

func TestBrewAction_Properties(t *testing.T) {
	action := &actions.BrewAction{
		PackName:     "dev",
		BrewfilePath: "Brewfile",
		Checksum:     "sha256:12345",
	}

	assert.Equal(t, "dev", action.Pack())
	assert.Equal(t, "Install Homebrew packages from Brewfile", action.Description())

	// Verify it implements ProvisioningAction
	var _ actions.ProvisioningAction = action
}

// TestActionInterfaces verifies all actions implement the correct interfaces
func TestActionInterfaces(t *testing.T) {
	t.Run("linking_actions_implement_LinkingAction", func(t *testing.T) {
		var _ actions.LinkingAction = &actions.LinkAction{}
		var _ actions.LinkingAction = &actions.UnlinkAction{}
		var _ actions.LinkingAction = &actions.AddToPathAction{}
		var _ actions.LinkingAction = &actions.AddToShellProfileAction{}
	})

	t.Run("provisioning_actions_implement_ProvisioningAction", func(t *testing.T) {
		var _ actions.ProvisioningAction = &actions.RunScriptAction{}
		var _ actions.ProvisioningAction = &actions.RecordProvisioningAction{}
		var _ actions.ProvisioningAction = &actions.BrewAction{}
	})

	t.Run("all_actions_implement_base_Action_interface", func(t *testing.T) {
		var _ actions.Action = &actions.LinkAction{}
		var _ actions.Action = &actions.UnlinkAction{}
		var _ actions.Action = &actions.AddToPathAction{}
		var _ actions.Action = &actions.AddToShellProfileAction{}
		var _ actions.Action = &actions.RunScriptAction{}
		var _ actions.Action = &actions.RecordProvisioningAction{}
		var _ actions.Action = &actions.BrewAction{}
	})
}

// TestActionDescriptions ensures descriptions are properly formatted
func TestActionDescriptions(t *testing.T) {
	tests := []struct {
		name     string
		action   actions.Action
		wantDesc string
	}{
		{
			name: "link_action_description",
			action: &actions.LinkAction{
				PackName:   "vim",
				SourceFile: ".vimrc",
				TargetFile: "~/.vimrc",
			},
			wantDesc: "Link .vimrc to ~/.vimrc",
		},
		{
			name: "unlink_action_description",
			action: &actions.UnlinkAction{
				PackName:   "vim",
				SourceFile: ".vimrc",
			},
			wantDesc: "Unlink .vimrc",
		},
		{
			name: "add_to_path_action_description",
			action: &actions.AddToPathAction{
				PackName: "tools",
				DirPath:  "/home/user/bin",
			},
			wantDesc: "Add /home/user/bin to PATH",
		},
		{
			name: "add_to_shell_profile_action_description",
			action: &actions.AddToShellProfileAction{
				PackName:   "shell",
				ScriptPath: "aliases.sh",
			},
			wantDesc: "Add aliases.sh to shell profile",
		},
		{
			name: "run_script_action_description",
			action: &actions.RunScriptAction{
				PackName:     "dev",
				ScriptPath:   "install.sh",
				Checksum:     "sha256:12345",
				SentinelName: "install.sh.sentinel",
			},
			wantDesc: "Run provisioning script install.sh",
		},
		{
			name: "record_provisioning_action_description",
			action: &actions.RecordProvisioningAction{
				PackName:     "dev",
				SentinelName: "install.sh.sentinel",
				Checksum:     "sha256:12345",
			},
			wantDesc: "Record provisioning complete for install.sh.sentinel",
		},
		{
			name: "brew_action_description",
			action: &actions.BrewAction{
				PackName:     "dev",
				BrewfilePath: "Brewfile",
				Checksum:     "sha256:12345",
			},
			wantDesc: "Install Homebrew packages from Brewfile",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			assert.Equal(t, tt.wantDesc, tt.action.Description())
		})
	}
}
