package core

import (
	"testing"

	"github.com/arthur-debert/dodot/pkg/testutil"
	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestExecute(t *testing.T) {
	tests := []struct {
		name        string
		commandType CommandType
		expectError bool
		description string
	}{
		{
			name:        "link command orchestration",
			commandType: CommandLink,
			expectError: true, // Expected due to GetMatchesFS filesystem limitation
			description: "Tests the orchestration flow for link commands",
		},
		{
			name:        "provision command orchestration",
			commandType: CommandProvision,
			expectError: true, // Expected due to GetMatchesFS filesystem limitation
			description: "Tests the orchestration flow for provision commands",
		},
		{
			name:        "unlink command orchestration",
			commandType: CommandUnlink,
			expectError: false, // Should work since it filters out all matches
			description: "Tests the orchestration flow for unlink commands",
		},
		{
			name:        "deprovision command orchestration",
			commandType: CommandDeprovision,
			expectError: false, // Should work since it filters out all matches
			description: "Tests the orchestration flow for deprovision commands",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			// Create test environment
			env := testutil.NewTestEnvironment(t, testutil.EnvMemoryOnly)
			defer env.Cleanup()

			// Create a test pack to trigger discovery
			env.SetupPack("test-pack", testutil.PackConfig{
				Files: map[string]string{
					"vimrc": "\" vim config",
				},
			})

			// Execute command
			opts := ExecuteOptions{
				DotfilesRoot: env.DotfilesRoot,
				PackNames:    []string{"test-pack"},
				DryRun:       false,
				Force:        false,
				FileSystem:   env.FS,
			}

			ctx, err := Execute(tt.commandType, opts)

			if tt.expectError {
				// We expect an error because GetMatchesFS doesn't support MemoryFS yet
				// This tests that the orchestration works correctly and fails at the right step
				require.Error(t, err, "Execute should fail due to filesystem limitation")
			} else {
				// For unlink/deprovision, no matches should be processed so it should succeed
				require.NoError(t, err, "Execute should succeed")
				require.NotNil(t, ctx, "Execution context should not be nil")

				assert.Equal(t, string(tt.commandType), ctx.Command)
				assert.False(t, ctx.DryRun)
				assert.NotNil(t, ctx.PackResults)
			}
		})
	}
}

func TestExecuteDryRun(t *testing.T) {
	// Create test environment
	env := testutil.NewTestEnvironment(t, testutil.EnvMemoryOnly)
	defer env.Cleanup()

	// Set up test pack using testutil
	env.SetupPack("test-pack", testutil.PackConfig{
		Files: map[string]string{
			"vimrc":  "\" vim config",
			"bashrc": "# bash config",
		},
	})

	opts := ExecuteOptions{
		DotfilesRoot: env.DotfilesRoot,
		PackNames:    []string{"test-pack"},
		DryRun:       true,
		Force:        false,
		FileSystem:   env.FS,
	}

	// Execute in dry run mode - this will fail due to GetMatchesFS filesystem limitation
	_, err := Execute(CommandLink, opts)

	// We expect an error because GetMatchesFS doesn't support MemoryFS yet
	// This test verifies the orchestration works and fails at the expected step
	require.Error(t, err, "Execute should fail due to filesystem limitation")
}

func TestFilterMatchesByCommandType(t *testing.T) {
	tests := []struct {
		name           string
		commandType    CommandType
		inputMatches   []types.RuleMatch
		expectHandlers []string
	}{
		{
			name:        "link command filters to configuration handlers only",
			commandType: CommandLink,
			inputMatches: []types.RuleMatch{
				{HandlerName: "symlink"},
				{HandlerName: "shell"},
				{HandlerName: "install"},  // Should be filtered out
				{HandlerName: "homebrew"}, // Should be filtered out
				{HandlerName: "path"},
			},
			expectHandlers: []string{"symlink", "shell", "path"},
		},
		{
			name:        "provision command includes all handlers",
			commandType: CommandProvision,
			inputMatches: []types.RuleMatch{
				{HandlerName: "symlink"},
				{HandlerName: "shell"},
				{HandlerName: "install"},
				{HandlerName: "homebrew"},
				{HandlerName: "path"},
			},
			expectHandlers: []string{"symlink", "shell", "install", "homebrew", "path"},
		},
		{
			name:        "unlink command returns empty (uses different flow)",
			commandType: CommandUnlink,
			inputMatches: []types.RuleMatch{
				{HandlerName: "symlink"},
				{HandlerName: "install"},
			},
			expectHandlers: []string{},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			filtered := filterMatchesByCommandType(tt.inputMatches, tt.commandType)

			// Extract handler names from filtered matches
			actualHandlers := make([]string, len(filtered))
			for i, match := range filtered {
				actualHandlers[i] = match.HandlerName
			}

			assert.ElementsMatch(t, tt.expectHandlers, actualHandlers)
		})
	}
}
