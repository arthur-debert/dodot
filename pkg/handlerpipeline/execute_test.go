package handlerpipeline

import (
	"path/filepath"
	"testing"

	"github.com/arthur-debert/dodot/pkg/testutil"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestExecuteMatches(t *testing.T) {
	tests := []struct {
		name           string
		matches        []RuleMatch
		expectSuccess  bool
		expectHandlers []string
	}{
		{
			name:           "empty matches",
			matches:        []RuleMatch{},
			expectSuccess:  true,
			expectHandlers: []string{},
		},
		{
			name: "single symlink match",
			matches: []RuleMatch{
				{
					Pack:         "test-pack",
					Path:         "vimrc",
					AbsolutePath: "/test/dotfiles/test-pack/vimrc",
					HandlerName:  "symlink",
				},
			},
			expectSuccess:  true,
			expectHandlers: []string{"symlink"},
		},
		{
			name: "multiple symlink handlers",
			matches: []RuleMatch{
				{
					Pack:         "test-pack",
					Path:         "vimrc",
					AbsolutePath: "/test/dotfiles/test-pack/vimrc",
					HandlerName:  "symlink",
				},
				{
					Pack:         "test-pack",
					Path:         "bashrc",
					AbsolutePath: "/test/dotfiles/test-pack/bashrc",
					HandlerName:  "symlink",
				},
			},
			expectSuccess:  true,
			expectHandlers: []string{"symlink"},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			// Create test environment
			env := testutil.NewTestEnvironment(t, testutil.EnvMemoryOnly)
			defer env.Cleanup()

			// Create test files that handlers expect
			for _, match := range tt.matches {
				// Create the source file
				err := env.FS.MkdirAll(filepath.Dir(match.AbsolutePath), 0755)
				require.NoError(t, err)

				// Create a simple test file
				testContent := "# Test file for " + match.HandlerName
				err = env.FS.WriteFile(match.AbsolutePath, []byte(testContent), 0644)
				require.NoError(t, err)
			}

			// Set up execution options
			opts := ExecutionOptions{
				DryRun:     false,
				Force:      false,
				FileSystem: env.FS,
			}

			// Execute matches
			ctx, err := ExecuteMatches(tt.matches, env.DataStore, opts)

			if tt.expectSuccess {
				require.NoError(t, err, "ExecuteMatches should succeed")
				require.NotNil(t, ctx, "Execution context should not be nil")
				assert.Equal(t, "execute", ctx.Command)
				assert.False(t, ctx.DryRun)

				// Check that execution completed without errors
				// Note: ExecutionContext totals depend on proper PackExecutionResult setup
				// For now, just verify no errors occurred
				assert.NotNil(t, ctx.PackResults, "Should have pack results")
				if len(tt.expectHandlers) > 0 {
					assert.NotEmpty(t, ctx.PackResults, "Should have processed some packs")
				}
			} else {
				require.Error(t, err, "ExecuteMatches should fail")
			}
		})
	}
}

func TestExecuteMatchesDryRun(t *testing.T) {
	// Create test environment
	env := testutil.NewTestEnvironment(t, testutil.EnvMemoryOnly)
	defer env.Cleanup()

	// Create test matches
	matches := []RuleMatch{
		{
			Pack:         "test-pack",
			Path:         "vimrc",
			AbsolutePath: "/test/dotfiles/test-pack/vimrc",
			HandlerName:  "symlink",
		},
	}

	opts := ExecutionOptions{
		DryRun:     true,
		Force:      false,
		FileSystem: env.FS,
	}

	// Execute in dry run mode
	ctx, err := ExecuteMatches(matches, env.DataStore, opts)

	require.NoError(t, err)
	require.NotNil(t, ctx)
	assert.True(t, ctx.DryRun, "Context should indicate dry run")
}

func TestGetHandlerExecutionOrder(t *testing.T) {
	tests := []struct {
		name          string
		handlerNames  []string
		expectedOrder []string
	}{
		{
			name:          "empty list",
			handlerNames:  []string{},
			expectedOrder: []string{},
		},
		{
			name:          "configuration handlers only",
			handlerNames:  []string{"symlink", "shell", "path"},
			expectedOrder: []string{"path", "shell", "symlink"}, // alphabetical within same category
		},
		{
			name:          "code execution handlers only",
			handlerNames:  []string{"install", "homebrew"},
			expectedOrder: []string{"homebrew", "install"}, // alphabetical within same category
		},
		{
			name:          "mixed handlers - code execution first",
			handlerNames:  []string{"symlink", "install", "shell", "homebrew"},
			expectedOrder: []string{"homebrew", "install", "shell", "symlink"}, // code first, then config
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			order := GetHandlerExecutionOrder(tt.handlerNames)
			assert.Equal(t, tt.expectedOrder, order)
		})
	}
}

func TestCreateOperationsHandler(t *testing.T) {
	tests := []struct {
		name        string
		handlerName string
		expectError bool
	}{
		{
			name:        "symlink handler",
			handlerName: "symlink",
			expectError: false,
		},
		{
			name:        "shell handler",
			handlerName: "shell",
			expectError: false,
		},
		{
			name:        "install handler",
			handlerName: "install",
			expectError: false,
		},
		{
			name:        "homebrew handler",
			handlerName: "homebrew",
			expectError: false,
		},
		{
			name:        "path handler",
			handlerName: "path",
			expectError: false,
		},
		{
			name:        "unknown handler",
			handlerName: "nonexistent",
			expectError: true,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			handler, err := createOperationsHandler(tt.handlerName)

			if tt.expectError {
				assert.Error(t, err)
				assert.Nil(t, handler)
			} else {
				assert.NoError(t, err)
				assert.NotNil(t, handler)
				assert.Equal(t, tt.handlerName, handler.Name())
			}
		})
	}
}
