package provision

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestProvisionScriptHandler_ProcessProvisioning(t *testing.T) {
	// Create a temporary directory for test scripts
	tempDir := t.TempDir()

	// Create test scripts with known content
	script1Content := "#!/bin/sh\necho 'Installing pack1'\n"
	script1Path := filepath.Join(tempDir, "pack1", "install.sh")
	require.NoError(t, os.MkdirAll(filepath.Dir(script1Path), 0755))
	require.NoError(t, os.WriteFile(script1Path, []byte(script1Content), 0755))

	script2Content := "#!/bin/sh\necho 'Installing pack2'\n"
	script2Path := filepath.Join(tempDir, "pack2", "provision.sh")
	require.NoError(t, os.MkdirAll(filepath.Dir(script2Path), 0755))
	require.NoError(t, os.WriteFile(script2Path, []byte(script2Content), 0755))

	handler := NewProvisionScriptHandler()

	tests := []struct {
		name          string
		matches       []types.TriggerMatch
		expectedCount int
		expectedError bool
		checkActions  func(t *testing.T, actions []types.ProvisioningAction)
	}{
		{
			name: "single install script",
			matches: []types.TriggerMatch{
				{
					Path:         "install.sh",
					AbsolutePath: script1Path,
					Pack:         "pack1",
					TriggerName:  "filename",
				},
			},
			expectedCount: 1,
			expectedError: false,
			checkActions: func(t *testing.T, actions []types.ProvisioningAction) {
				action, ok := actions[0].(*types.RunScriptAction)
				require.True(t, ok, "action should be RunScriptAction")
				assert.Equal(t, "pack1", action.PackName)
				assert.Equal(t, script1Path, action.ScriptPath)
				assert.NotEmpty(t, action.Checksum)
				assert.Contains(t, action.Checksum, "sha256:")
				assert.Equal(t, "install.sh.sentinel", action.SentinelName)
			},
		},
		{
			name: "multiple provision scripts",
			matches: []types.TriggerMatch{
				{
					Path:         "install.sh",
					AbsolutePath: script1Path,
					Pack:         "pack1",
					TriggerName:  "filename",
				},
				{
					Path:         "provision.sh",
					AbsolutePath: script2Path,
					Pack:         "pack2",
					TriggerName:  "filename",
				},
			},
			expectedCount: 2,
			expectedError: false,
			checkActions: func(t *testing.T, actions []types.ProvisioningAction) {
				// Check first action
				action1, ok := actions[0].(*types.RunScriptAction)
				require.True(t, ok)
				assert.Equal(t, "pack1", action1.PackName)
				assert.Equal(t, script1Path, action1.ScriptPath)
				assert.Equal(t, "install.sh.sentinel", action1.SentinelName)

				// Check second action
				action2, ok := actions[1].(*types.RunScriptAction)
				require.True(t, ok)
				assert.Equal(t, "pack2", action2.PackName)
				assert.Equal(t, script2Path, action2.ScriptPath)
				assert.Equal(t, "provision.sh.sentinel", action2.SentinelName)

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
			name: "non-existent script",
			matches: []types.TriggerMatch{
				{
					Path:         "missing.sh",
					AbsolutePath: "/non/existent/path/missing.sh",
					Pack:         "missing",
					TriggerName:  "filename",
				},
			},
			expectedCount: 0,
			expectedError: true,
		},
		{
			name: "nested provision script",
			matches: []types.TriggerMatch{
				{
					Path:         "scripts/setup.sh",
					AbsolutePath: script1Path,
					Pack:         "complex",
					TriggerName:  "glob",
				},
			},
			expectedCount: 1,
			expectedError: false,
			checkActions: func(t *testing.T, actions []types.ProvisioningAction) {
				action, ok := actions[0].(*types.RunScriptAction)
				require.True(t, ok)
				assert.Equal(t, "complex", action.PackName)
				assert.Equal(t, "scripts/setup.sh.sentinel", action.SentinelName)
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

func TestProvisionScriptHandler_ValidateOptions(t *testing.T) {
	handler := NewProvisionScriptHandler()

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

func TestProvisionScriptHandler_Properties(t *testing.T) {
	handler := NewProvisionScriptHandler()

	assert.Equal(t, ProvisionScriptHandlerName, handler.Name())
	assert.Equal(t, "Runs install.sh scripts for initial setup", handler.Description())
	assert.Equal(t, types.RunModeProvisioning, handler.RunMode())

	// Verify template content
	template := handler.GetTemplateContent()
	assert.NotEmpty(t, template)
	assert.Contains(t, template, "dodot install script")
}
