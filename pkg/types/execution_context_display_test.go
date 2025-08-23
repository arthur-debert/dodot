package types_test

import (
	"os"
	"testing"
	"time"

	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestExecutionContext_ToDisplayResult_WithActionDetails(t *testing.T) {
	// Create execution context
	ctx := types.NewExecutionContext("link", false)

	// Create a pack
	pack := &types.Pack{
		Name: "test",
		Path: "/test/path",
	}
	packResult := types.NewPackExecutionResult(pack)

	// Test symlink handler with LinkAction
	t.Run("symlink handler shows target path", func(t *testing.T) {
		homeDir := os.Getenv("HOME")
		if homeDir == "" {
			homeDir = "/home/user"
		}

		handlerResult := &types.HandlerResult{
			HandlerName: "symlink",
			Files:       []string{".vimrc"},
			Status:      types.StatusReady,
			StartTime:   time.Now(),
			EndTime:     time.Now(),
			Pack:        "test",
			Actions: []types.Action{
				&types.LinkAction{
					PackName:   "test",
					SourceFile: ".vimrc",
					TargetFile: homeDir + "/.vimrc",
				},
			},
		}

		packResult.AddHandlerResult(handlerResult)
		packResult.Complete()
		ctx.AddPackResult("test", packResult)
		ctx.Complete()

		displayResult := ctx.ToDisplayResult()
		require.Len(t, displayResult.Packs, 1)
		require.Len(t, displayResult.Packs[0].Files, 1)

		file := displayResult.Packs[0].Files[0]
		assert.Equal(t, "symlink", file.Handler)
		assert.Equal(t, ".vimrc", file.Path)
		assert.Contains(t, file.AdditionalInfo, "/.vimrc") // Should show target path
	})

	// Test path handler with AddToPathAction
	t.Run("path handler shows directory info", func(t *testing.T) {
		ctx := types.NewExecutionContext("link", false)
		packResult := types.NewPackExecutionResult(pack)

		handlerResult := &types.HandlerResult{
			HandlerName: "path",
			Files:       []string{"/test/bin"},
			Status:      types.StatusReady,
			StartTime:   time.Now(),
			EndTime:     time.Now(),
			Pack:        "test",
			Actions: []types.Action{
				&types.AddToPathAction{
					PackName: "test",
					DirPath:  "/test/bin",
				},
			},
		}

		packResult.AddHandlerResult(handlerResult)
		packResult.Complete()
		ctx.AddPackResult("test", packResult)
		ctx.Complete()

		displayResult := ctx.ToDisplayResult()
		require.Len(t, displayResult.Packs, 1)
		require.Len(t, displayResult.Packs[0].Files, 1)

		file := displayResult.Packs[0].Files[0]
		assert.Equal(t, "path", file.Handler)
		assert.Equal(t, "/test/bin", file.Path)
		assert.Equal(t, "→ $PATH/bin", file.AdditionalInfo)
	})

	// Test shell_profile handler with AddToShellProfileAction
	t.Run("shell_profile handler shows shell type", func(t *testing.T) {
		testCases := []struct {
			scriptName   string
			expectedInfo string
		}{
			{"bash_profile.sh", "→ bash profile"},
			{"zsh_config.sh", "→ zsh profile"},
			{"config.fish", "→ fish config"},
			{"generic.sh", "→ shell profile"},
		}

		for _, tc := range testCases {
			t.Run(tc.scriptName, func(t *testing.T) {
				ctx := types.NewExecutionContext("link", false)
				packResult := types.NewPackExecutionResult(pack)

				handlerResult := &types.HandlerResult{
					HandlerName: "shell_profile",
					Files:       []string{tc.scriptName},
					Status:      types.StatusReady,
					StartTime:   time.Now(),
					EndTime:     time.Now(),
					Pack:        "test",
					Actions: []types.Action{
						&types.AddToShellProfileAction{
							PackName:   "test",
							ScriptPath: tc.scriptName,
						},
					},
				}

				packResult.AddHandlerResult(handlerResult)
				packResult.Complete()
				ctx.AddPackResult("test", packResult)
				ctx.Complete()

				displayResult := ctx.ToDisplayResult()
				require.Len(t, displayResult.Packs, 1)
				require.Len(t, displayResult.Packs[0].Files, 1)

				file := displayResult.Packs[0].Files[0]
				assert.Equal(t, "shell_profile", file.Handler)
				assert.Equal(t, tc.scriptName, file.Path)
				assert.Equal(t, tc.expectedInfo, file.AdditionalInfo)
			})
		}
	})
}
