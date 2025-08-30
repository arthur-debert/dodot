package operations_test

import (
	"testing"

	"github.com/arthur-debert/dodot/pkg/operations"
	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestAdapter_ActionsToOperations(t *testing.T) {
	tests := []struct {
		name           string
		actions        []types.Action
		expectedOps    []operations.Operation
		expectedCount  int
		checkOperation func(*testing.T, operations.Operation)
	}{
		{
			name: "LinkAction converts to two operations",
			actions: []types.Action{
				&types.LinkAction{
					PackName:   "vim",
					SourceFile: ".vimrc",
					TargetFile: "/home/user/.vimrc",
				},
			},
			expectedCount: 2,
			checkOperation: func(t *testing.T, op operations.Operation) {
				// First operation should be CreateDataLink
				if op.Type == operations.CreateDataLink {
					assert.Equal(t, "vim", op.Pack)
					assert.Equal(t, "symlink", op.Handler)
					assert.Equal(t, ".vimrc", op.Source)
				}
				// Second operation should be CreateUserLink
				if op.Type == operations.CreateUserLink {
					assert.Equal(t, "vim", op.Pack)
					assert.Equal(t, "symlink", op.Handler)
					assert.Equal(t, "/home/user/.vimrc", op.Target)
				}
			},
		},
		{
			name: "AddToPathAction converts to single operation",
			actions: []types.Action{
				&types.AddToPathAction{
					PackName: "tools",
					DirPath:  "/home/user/tools/bin",
				},
			},
			expectedCount: 1,
			checkOperation: func(t *testing.T, op operations.Operation) {
				assert.Equal(t, operations.CreateDataLink, op.Type)
				assert.Equal(t, "tools", op.Pack)
				assert.Equal(t, "path", op.Handler)
				assert.Equal(t, "/home/user/tools/bin", op.Source)
			},
		},
		{
			name: "AddToShellProfileAction converts to single operation",
			actions: []types.Action{
				&types.AddToShellProfileAction{
					PackName:   "bash",
					ScriptPath: "/home/user/dotfiles/bash/aliases.sh",
				},
			},
			expectedCount: 1,
			checkOperation: func(t *testing.T, op operations.Operation) {
				assert.Equal(t, operations.CreateDataLink, op.Type)
				assert.Equal(t, "bash", op.Pack)
				assert.Equal(t, "shell_profile", op.Handler)
				assert.Equal(t, "/home/user/dotfiles/bash/aliases.sh", op.Source)
			},
		},
		{
			name: "RunScriptAction converts to RunCommand operation",
			actions: []types.Action{
				&types.RunScriptAction{
					PackName:     "tools",
					ScriptPath:   "./install.sh",
					SentinelName: "install-complete",
					Checksum:     "abc123",
				},
			},
			expectedCount: 1,
			checkOperation: func(t *testing.T, op operations.Operation) {
				assert.Equal(t, operations.RunCommand, op.Type)
				assert.Equal(t, "tools", op.Pack)
				assert.Equal(t, "install", op.Handler)
				assert.Equal(t, "./install.sh", op.Command)
				assert.Equal(t, "install-complete", op.Sentinel)
				assert.Equal(t, "abc123", op.Metadata["checksum"])
			},
		},
		{
			name: "BrewAction converts to RunCommand operation",
			actions: []types.Action{
				&types.BrewAction{
					PackName:     "dev",
					BrewfilePath: "/home/user/dotfiles/dev/Brewfile",
					Checksum:     "def456",
				},
			},
			expectedCount: 1,
			checkOperation: func(t *testing.T, op operations.Operation) {
				assert.Equal(t, operations.RunCommand, op.Type)
				assert.Equal(t, "dev", op.Pack)
				assert.Equal(t, "homebrew", op.Handler)
				assert.Contains(t, op.Command, "brew bundle")
				assert.Contains(t, op.Command, "/home/user/dotfiles/dev/Brewfile")
				assert.Contains(t, op.Sentinel, "brewfile-")
				assert.Equal(t, "/home/user/dotfiles/dev/Brewfile", op.Metadata["brewfile"])
				assert.Equal(t, "def456", op.Metadata["checksum"])
			},
		},
		{
			name: "multiple actions convert correctly",
			actions: []types.Action{
				&types.LinkAction{
					PackName:   "vim",
					SourceFile: ".vimrc",
					TargetFile: "/home/user/.vimrc",
				},
				&types.AddToPathAction{
					PackName: "tools",
					DirPath:  "/home/user/tools/bin",
				},
				&types.RunScriptAction{
					PackName:     "tools",
					ScriptPath:   "./install.sh",
					SentinelName: "install-complete",
					Checksum:     "abc123",
				},
			},
			expectedCount: 4, // 2 for link + 1 for path + 1 for script
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			executor := operations.NewExecutor(nil, nil, nil, false)
			adapter := operations.NewActionAdapter(executor)

			ops, err := adapter.ActionsToOperations(tt.actions)
			require.NoError(t, err)
			assert.Len(t, ops, tt.expectedCount)

			// Run specific checks if provided
			if tt.checkOperation != nil {
				for _, op := range ops {
					tt.checkOperation(t, op)
				}
			}
		})
	}
}

func TestAdapter_OperationsToActions(t *testing.T) {
	tests := []struct {
		name            string
		operations      []operations.Operation
		expectedActions int
		checkAction     func(*testing.T, types.Action)
	}{
		{
			name: "CreateDataLink for path becomes AddToPathAction",
			operations: []operations.Operation{
				{
					Type:    operations.CreateDataLink,
					Pack:    "tools",
					Handler: "path",
					Source:  "/home/user/tools/bin",
				},
			},
			expectedActions: 1,
			checkAction: func(t *testing.T, action types.Action) {
				pathAction, ok := action.(*types.AddToPathAction)
				require.True(t, ok)
				assert.Equal(t, "tools", pathAction.PackName)
				assert.Equal(t, "/home/user/tools/bin", pathAction.DirPath)
			},
		},
		{
			name: "CreateDataLink for shell becomes AddToShellProfileAction",
			operations: []operations.Operation{
				{
					Type:    operations.CreateDataLink,
					Pack:    "bash",
					Handler: "shell",
					Source:  "/home/user/dotfiles/bash/aliases.sh",
				},
			},
			expectedActions: 1,
			checkAction: func(t *testing.T, action types.Action) {
				shellAction, ok := action.(*types.AddToShellProfileAction)
				require.True(t, ok)
				assert.Equal(t, "bash", shellAction.PackName)
				assert.Equal(t, "/home/user/dotfiles/bash/aliases.sh", shellAction.ScriptPath)
			},
		},
		{
			name: "CreateUserLink becomes LinkAction",
			operations: []operations.Operation{
				{
					Type:    operations.CreateUserLink,
					Pack:    "vim",
					Handler: "symlink",
					Source:  "/datastore/vim/.vimrc",
					Target:  "/home/user/.vimrc",
				},
			},
			expectedActions: 1,
			checkAction: func(t *testing.T, action types.Action) {
				linkAction, ok := action.(*types.LinkAction)
				require.True(t, ok)
				assert.Equal(t, "vim", linkAction.PackName)
				assert.Equal(t, "/home/user/.vimrc", linkAction.TargetFile)
			},
		},
		{
			name: "RunCommand for install becomes RunScriptAction",
			operations: []operations.Operation{
				{
					Type:     operations.RunCommand,
					Pack:     "tools",
					Handler:  "install",
					Command:  "./install.sh",
					Sentinel: "install-complete",
					Metadata: map[string]interface{}{
						"checksum": "abc123",
					},
				},
			},
			expectedActions: 1,
			checkAction: func(t *testing.T, action types.Action) {
				scriptAction, ok := action.(*types.RunScriptAction)
				require.True(t, ok)
				assert.Equal(t, "tools", scriptAction.PackName)
				assert.Equal(t, "./install.sh", scriptAction.ScriptPath)
				assert.Equal(t, "install-complete", scriptAction.SentinelName)
				assert.Equal(t, "abc123", scriptAction.Checksum)
			},
		},
		{
			name: "CheckSentinel operations are skipped",
			operations: []operations.Operation{
				{
					Type:     operations.CheckSentinel,
					Pack:     "tools",
					Handler:  "install",
					Sentinel: "install-complete",
				},
			},
			expectedActions: 0,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			executor := operations.NewExecutor(nil, nil, nil, false)
			adapter := operations.NewActionAdapter(executor)

			actions, err := adapter.OperationsToActions(tt.operations)
			require.NoError(t, err)
			assert.Len(t, actions, tt.expectedActions)

			// Run specific checks if provided
			if tt.checkAction != nil && len(actions) > 0 {
				for _, action := range actions {
					tt.checkAction(t, action)
				}
			}
		})
	}
}

func TestAdapter_RoundTrip(t *testing.T) {
	// Test that converting actions to operations and back preserves information
	executor := operations.NewExecutor(nil, nil, nil, false)
	adapter := operations.NewActionAdapter(executor)

	originalActions := []types.Action{
		&types.AddToPathAction{
			PackName: "tools",
			DirPath:  "/home/user/tools/bin",
		},
		&types.RunScriptAction{
			PackName:     "tools",
			ScriptPath:   "./install.sh",
			SentinelName: "install-complete",
			Checksum:     "abc123",
		},
	}

	// Convert to operations
	ops, err := adapter.ActionsToOperations(originalActions)
	require.NoError(t, err)

	// Convert back to actions
	resultActions, err := adapter.OperationsToActions(ops)
	require.NoError(t, err)

	// Should have same number of actions
	assert.Len(t, resultActions, len(originalActions))

	// Verify each action type is preserved
	pathActionFound := false
	scriptActionFound := false

	for _, action := range resultActions {
		switch a := action.(type) {
		case *types.AddToPathAction:
			pathActionFound = true
			assert.Equal(t, "tools", a.PackName)
			assert.Equal(t, "/home/user/tools/bin", a.DirPath)
		case *types.RunScriptAction:
			scriptActionFound = true
			assert.Equal(t, "tools", a.PackName)
			assert.Equal(t, "./install.sh", a.ScriptPath)
			assert.Equal(t, "install-complete", a.SentinelName)
			assert.Equal(t, "abc123", a.Checksum)
		}
	}

	assert.True(t, pathActionFound, "Path action should be preserved")
	assert.True(t, scriptActionFound, "Script action should be preserved")
}

func TestExtractScriptPath(t *testing.T) {
	tests := []struct {
		name     string
		command  string
		expected string
	}{
		{
			name:     "simple script path",
			command:  "./install.sh",
			expected: "./install.sh",
		},
		{
			name:     "script with arguments",
			command:  "./install.sh --verbose --prefix=/usr/local",
			expected: "./install.sh",
		},
		{
			name:     "absolute path",
			command:  "/usr/bin/setup.sh arg1 arg2",
			expected: "/usr/bin/setup.sh",
		},
		{
			name:     "relative path with dots",
			command:  "../scripts/configure.sh --enable-feature",
			expected: "../scripts/configure.sh",
		},
		{
			name:     "simple command name",
			command:  "make",
			expected: "make",
		},
		{
			name:     "command with args but no path",
			command:  "npm install",
			expected: "npm install",
		},
		{
			name:     "empty command",
			command:  "",
			expected: "",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			// We need to test this through the adapter since extractScriptPath is private
			// Create a RunScriptAction and convert it to operations
			executor := operations.NewExecutor(nil, nil, nil, false)
			adapter := operations.NewActionAdapter(executor)

			action := &types.RunScriptAction{
				PackName:     "test",
				ScriptPath:   tt.command,
				SentinelName: "test-sentinel",
				Checksum:     "test123",
			}

			ops, err := adapter.ActionsToOperations([]types.Action{action})
			require.NoError(t, err)
			require.Len(t, ops, 1)

			// The command in the operation should be the original command
			assert.Equal(t, tt.command, ops[0].Command)

			// Now convert back and check the script path extraction
			actions, err := adapter.OperationsToActions(ops)
			require.NoError(t, err)
			require.Len(t, actions, 1)

			scriptAction, ok := actions[0].(*types.RunScriptAction)
			require.True(t, ok)
			assert.Equal(t, tt.expected, scriptAction.ScriptPath)
		})
	}
}
