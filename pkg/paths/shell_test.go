// Test Type: Unit Test
// Description: Tests for the paths package - shell path resolution functions

package paths_test

import (
	"testing"

	"github.com/arthur-debert/dodot/pkg/errors"
	"github.com/arthur-debert/dodot/pkg/paths"
	"github.com/stretchr/testify/assert"
)

func TestResolveShellScriptPath(t *testing.T) {
	tests := []struct {
		name        string
		scriptName  string
		expectError bool
		errorCode   errors.ErrorCode
	}{
		{
			name:        "bash_script",
			scriptName:  "bash.sh",
			expectError: false,
		},
		{
			name:        "zsh_script",
			scriptName:  "zsh.sh",
			expectError: false,
		},
		{
			name:        "fish_script",
			scriptName:  "fish.sh",
			expectError: false,
		},
		{
			name:        "empty_script_name",
			scriptName:  "",
			expectError: true,
			errorCode:   errors.ErrInvalidInput,
		},
		{
			name:        "unknown_script",
			scriptName:  "unknown.sh",
			expectError: true,
			errorCode:   errors.ErrNotFound,
		},
		{
			name:        "no_extension",
			scriptName:  "bash",
			expectError: true,
			errorCode:   errors.ErrNotFound,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result, err := paths.ResolveShellScriptPath(tt.scriptName)

			if tt.expectError {
				assert.Error(t, err)
				assert.Empty(t, result)
				if terr, ok := err.(*errors.DodotError); ok {
					assert.Equal(t, tt.errorCode, terr.Code)
				}
			} else {
				assert.NoError(t, err)
				assert.NotEmpty(t, result)
				assert.Contains(t, result, tt.scriptName)
				assert.Contains(t, result, "templates")
			}
		})
	}
}

func TestGetShellScriptPath(t *testing.T) {
	tests := []struct {
		name     string
		shell    string
		expected string
	}{
		{
			name:     "bash_shell",
			shell:    "bash",
			expected: "bash.sh",
		},
		{
			name:     "zsh_shell",
			shell:    "zsh",
			expected: "zsh.sh",
		},
		{
			name:     "fish_shell",
			shell:    "fish",
			expected: "fish.sh",
		},
		{
			name:     "unknown_shell",
			shell:    "unknown",
			expected: "",
		},
		{
			name:     "empty_shell",
			shell:    "",
			expected: "",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result := paths.GetShellScriptPath(tt.shell)
			assert.Equal(t, tt.expected, result)
		})
	}
}
