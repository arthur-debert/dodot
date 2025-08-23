package homebrew

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestHomebrewHandler_ProcessProvisioning(t *testing.T) {
	// Create a temporary directory for test Brewfiles
	tempDir := t.TempDir()

	// Create test Brewfiles with known content
	brewfile1Content := `# Test Brewfile 1
brew 'git'
brew 'tmux'
cask 'firefox'
`
	brewfile1Path := filepath.Join(tempDir, "pack1", "Brewfile")
	require.NoError(t, os.MkdirAll(filepath.Dir(brewfile1Path), 0755))
	require.NoError(t, os.WriteFile(brewfile1Path, []byte(brewfile1Content), 0644))

	brewfile2Content := `# Test Brewfile 2
brew 'neovim'
brew 'ripgrep'
`
	brewfile2Path := filepath.Join(tempDir, "pack2", "Brewfile")
	require.NoError(t, os.MkdirAll(filepath.Dir(brewfile2Path), 0755))
	require.NoError(t, os.WriteFile(brewfile2Path, []byte(brewfile2Content), 0644))

	handler := NewHomebrewHandler()

	tests := []struct {
		name          string
		matches       []types.TriggerMatch
		expectedCount int
		expectedError bool
		checkActions  func(t *testing.T, actions []types.ProvisioningAction)
	}{
		{
			name: "single Brewfile",
			matches: []types.TriggerMatch{
				{
					Path:         "Brewfile",
					AbsolutePath: brewfile1Path,
					Pack:         "pack1",
					TriggerName:  "filename",
				},
			},
			expectedCount: 1,
			expectedError: false,
			checkActions: func(t *testing.T, actions []types.ProvisioningAction) {
				action, ok := actions[0].(*types.BrewAction)
				require.True(t, ok, "action should be BrewAction")
				assert.Equal(t, "pack1", action.PackName)
				assert.Equal(t, brewfile1Path, action.BrewfilePath)
				assert.NotEmpty(t, action.Checksum)
				assert.Contains(t, action.Checksum, "sha256:")
			},
		},
		{
			name: "multiple Brewfiles",
			matches: []types.TriggerMatch{
				{
					Path:         "Brewfile",
					AbsolutePath: brewfile1Path,
					Pack:         "pack1",
					TriggerName:  "filename",
				},
				{
					Path:         "Brewfile",
					AbsolutePath: brewfile2Path,
					Pack:         "pack2",
					TriggerName:  "filename",
				},
			},
			expectedCount: 2,
			expectedError: false,
			checkActions: func(t *testing.T, actions []types.ProvisioningAction) {
				// Check first action
				action1, ok := actions[0].(*types.BrewAction)
				require.True(t, ok)
				assert.Equal(t, "pack1", action1.PackName)
				assert.Equal(t, brewfile1Path, action1.BrewfilePath)

				// Check second action
				action2, ok := actions[1].(*types.BrewAction)
				require.True(t, ok)
				assert.Equal(t, "pack2", action2.PackName)
				assert.Equal(t, brewfile2Path, action2.BrewfilePath)

				// Verify different checksums (different content)
				assert.NotEqual(t, action1.Checksum, action2.Checksum)
			},
		},
		{
			name:          "empty matches",
			matches:       []types.TriggerMatch{},
			expectedCount: 0,
			expectedError: false,
		},
		{
			name: "non-existent Brewfile",
			matches: []types.TriggerMatch{
				{
					Path:         "Brewfile",
					AbsolutePath: "/non/existent/path/Brewfile",
					Pack:         "missing",
					TriggerName:  "filename",
				},
			},
			expectedCount: 0,
			expectedError: true,
		},
		{
			name: "Brewfile with custom name",
			matches: []types.TriggerMatch{
				{
					Path:         "Brewfile.custom",
					AbsolutePath: brewfile1Path,
					Pack:         "custom",
					TriggerName:  "glob",
				},
			},
			expectedCount: 1,
			expectedError: false,
			checkActions: func(t *testing.T, actions []types.ProvisioningAction) {
				action, ok := actions[0].(*types.BrewAction)
				require.True(t, ok)
				assert.Equal(t, "custom", action.PackName)
			},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			actions, err := handler.ProcessProvisioning(tt.matches)

			if tt.expectedError {
				assert.Error(t, err)
				return
			}

			require.NoError(t, err)
			assert.Len(t, actions, tt.expectedCount)

			if tt.checkActions != nil {
				tt.checkActions(t, actions)
			}
		})
	}
}

func TestHomebrewHandler_ValidateOptions(t *testing.T) {
	handler := NewHomebrewHandler()

	tests := []struct {
		name          string
		options       map[string]interface{}
		expectedError bool
	}{
		{
			name:          "nil options",
			options:       nil,
			expectedError: false,
		},
		{
			name:          "empty options",
			options:       map[string]interface{}{},
			expectedError: false,
		},
		{
			name: "any options are accepted",
			options: map[string]interface{}{
				"anything": "goes",
			},
			expectedError: false, // Currently no options are validated
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			err := handler.ValidateOptions(tt.options)
			if tt.expectedError {
				assert.Error(t, err)
			} else {
				assert.NoError(t, err)
			}
		})
	}
}

func TestHomebrewHandler_Properties(t *testing.T) {
	handler := NewHomebrewHandler()

	assert.Equal(t, HomebrewHandlerName, handler.Name())
	assert.Equal(t, "Processes Brewfiles to install Homebrew packages", handler.Description())
	assert.Equal(t, types.RunModeProvisioning, handler.RunMode())

	// Verify template content
	template := handler.GetTemplateContent()
	assert.NotEmpty(t, template)
	assert.Contains(t, template, "Homebrew dependencies")
}

func TestBrewActionDescription(t *testing.T) {
	action := &types.BrewAction{
		PackName:     "test",
		BrewfilePath: "/path/to/Brewfile",
		Checksum:     "sha256:abcd1234",
	}

	desc := action.Description()
	assert.Contains(t, desc, "Install Homebrew packages")
	assert.Contains(t, desc, "/path/to/Brewfile")
}
