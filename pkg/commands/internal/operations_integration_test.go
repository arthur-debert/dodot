package internal_test

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/arthur-debert/dodot/pkg/commands/internal"
	"github.com/arthur-debert/dodot/pkg/testutil"
	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestOperationSystem_Integration(t *testing.T) {
	// Skip unless feature flag is set
	if os.Getenv("DODOT_USE_OPERATIONS") != "true" {
		t.Skip("Skipping operation system integration test (set DODOT_USE_OPERATIONS=true to run)")
	}

	tests := []struct {
		name         string
		setupPack    func(*testutil.TestEnvironment) error
		expectedOps  int
		checkResults func(*testing.T, *types.ExecutionContext)
	}{
		{
			name: "path handler creates operations",
			setupPack: func(env *testutil.TestEnvironment) error {
				// Create a pack with path handler files
				packPath := filepath.Join(env.DotfilesRoot, "tools")
				if err := env.FS.MkdirAll(packPath, 0755); err != nil {
					return err
				}

				// Create bin directory that should be added to PATH
				binPath := filepath.Join(packPath, "bin")
				if err := env.FS.MkdirAll(binPath, 0755); err != nil {
					return err
				}

				// Create .dodot.toml with path handler config
				config := `
[path]
patterns = ["bin"]
`
				return env.FS.WriteFile(
					filepath.Join(packPath, ".dodot.toml"),
					[]byte(config),
					0644,
				)
			},
			expectedOps: 1,
			checkResults: func(t *testing.T, ctx *types.ExecutionContext) {
				// Verify pack was processed
				assert.Len(t, ctx.PackResults, 1)

				// Verify operation was executed
				packResult, exists := ctx.PackResults["tools"]
				require.True(t, exists)
				require.NotNil(t, packResult)
				assert.Greater(t, len(packResult.HandlerResults), 0)

				// Check the result
				handlerResult := packResult.HandlerResults[0]
				assert.Equal(t, "path", handlerResult.HandlerName)
				assert.Equal(t, types.StatusReady, handlerResult.Status)
			},
		},
		{
			name: "dry run creates operations without execution",
			setupPack: func(env *testutil.TestEnvironment) error {
				// Same setup as above
				packPath := filepath.Join(env.DotfilesRoot, "dryrun")
				if err := env.FS.MkdirAll(packPath, 0755); err != nil {
					return err
				}

				binPath := filepath.Join(packPath, "bin")
				if err := env.FS.MkdirAll(binPath, 0755); err != nil {
					return err
				}

				config := `
[path]
patterns = ["bin"]
`
				return env.FS.WriteFile(
					filepath.Join(packPath, ".dodot.toml"),
					[]byte(config),
					0644,
				)
			},
			expectedOps: 1,
			checkResults: func(t *testing.T, ctx *types.ExecutionContext) {
				// Verify it's marked as dry run
				assert.True(t, ctx.DryRun)

				// Should still have results
				packResult, exists := ctx.PackResults["dryrun"]
				require.True(t, exists)
				require.NotNil(t, packResult)
				assert.Greater(t, len(packResult.HandlerResults), 0)
			},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			// Ensure feature flag is set
			oldFlag := os.Getenv("DODOT_USE_OPERATIONS")
			_ = os.Setenv("DODOT_USE_OPERATIONS", "true")
			defer func() {
				if oldFlag == "" {
					_ = os.Unsetenv("DODOT_USE_OPERATIONS")
				} else {
					_ = os.Setenv("DODOT_USE_OPERATIONS", oldFlag)
				}
			}()

			// Create test environment
			env := testutil.NewTestEnvironment(t, testutil.EnvMemoryOnly)
			defer env.Cleanup()

			// Setup pack
			err := tt.setupPack(env)
			require.NoError(t, err)

			// Run pipeline with operations
			isDryRun := tt.name == "dry run creates operations without execution"
			ctx, err := internal.RunPipeline(internal.PipelineOptions{
				DotfilesRoot:       env.DotfilesRoot,
				PackNames:          []string{}, // All packs
				DryRun:             isDryRun,
				CommandMode:        internal.CommandModeConfiguration,
				EnableHomeSymlinks: true,
				UseSimplifiedRules: false,
			})

			require.NoError(t, err)
			require.NotNil(t, ctx)

			// Check results
			if tt.checkResults != nil {
				tt.checkResults(t, ctx)
			}
		})
	}
}

func TestPathHandler_EndToEnd(t *testing.T) {
	// This test verifies the complete flow for path handler
	// from matches to operations to execution

	// Skip unless feature flag is set
	if os.Getenv("DODOT_USE_OPERATIONS") != "true" {
		t.Skip("Skipping path handler end-to-end test (set DODOT_USE_OPERATIONS=true to run)")
	}

	env := testutil.NewTestEnvironment(t, testutil.EnvMemoryOnly)
	defer env.Cleanup()

	// Create a pack with multiple path entries
	packPath := filepath.Join(env.DotfilesRoot, "multi-path")
	require.NoError(t, env.FS.MkdirAll(packPath, 0755))

	// Create multiple directories
	for _, dir := range []string{"bin", "scripts", "tools/bin"} {
		dirPath := filepath.Join(packPath, dir)
		require.NoError(t, env.FS.MkdirAll(dirPath, 0755))
	}

	// Create config
	config := `
[path]
patterns = ["bin", "scripts", "tools/bin"]
`
	require.NoError(t, env.FS.WriteFile(
		filepath.Join(packPath, ".dodot.toml"),
		[]byte(config),
		0644,
	))

	// Enable operations
	_ = os.Setenv("DODOT_USE_OPERATIONS", "true")
	defer func() { _ = os.Unsetenv("DODOT_USE_OPERATIONS") }()

	// Run link command
	ctx, err := internal.RunPipeline(internal.PipelineOptions{
		DotfilesRoot:       env.DotfilesRoot,
		PackNames:          []string{"multi-path"},
		DryRun:             false,
		CommandMode:        internal.CommandModeConfiguration,
		EnableHomeSymlinks: true,
	})

	require.NoError(t, err)
	require.NotNil(t, ctx)

	// Verify all paths were processed
	packResult, exists := ctx.PackResults["multi-path"]
	require.True(t, exists)
	require.NotNil(t, packResult)

	// Should have path handler with 3 files
	require.Greater(t, len(packResult.HandlerResults), 0)
	handlerResult := packResult.HandlerResults[0]
	assert.Equal(t, "path", handlerResult.HandlerName)
	assert.Len(t, handlerResult.Files, 3) // 3 directories
	assert.Equal(t, types.StatusReady, handlerResult.Status)
}
