package core

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/arthur-debert/dodot/pkg/testutil"
	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	// Import to register triggers and powerups
	_ "github.com/arthur-debert/dodot/pkg/powerups"
	_ "github.com/arthur-debert/dodot/pkg/triggers"
)

// TestConflictHandling_PreexistingFile_Integration tests how dodot handles
// symlinking when a file already exists at the target location.
func TestConflictHandling_PreexistingFile_Integration(t *testing.T) {
	// Setup test environment
	testEnv := testutil.NewTestEnvironment(t, "conflict-handling-integration")
	defer testEnv.Cleanup()

	// Create a pack with a file to be symlinked
	pack := testEnv.CreatePack("testpack")
	dotfileContent := "# This is my dotfile"
	testutil.CreateFile(t, pack, ".bashrc", dotfileContent)

	// Create a pre-existing file in the home directory
	preexistingContent := "# This is my original bashrc"
	homeDir := testEnv.Home()
	targetPath := filepath.Join(homeDir, ".bashrc")
	err := os.WriteFile(targetPath, []byte(preexistingContent), 0644)
	require.NoError(t, err, "Failed to create pre-existing file")

	// ===== TEST 1: Run without --force flag =====

	// Run InstallPacks, expecting it to report a conflict
	result, err := InstallPacks(InstallPacksOptions{
		DotfilesRoot: testEnv.DotfilesRoot(),
		PackNames:    []string{"testpack"},
		DryRun:       false,
		Force:        false,
	})
	require.NoError(t, err, "InstallPacks should not error out on conflict")

	// Find the symlink operation for .bashrc
	var symlinkOp *types.Operation
	for i, op := range result.Operations {
		if op.Type == types.OperationCreateSymlink && op.Target == targetPath {
			symlinkOp = &result.Operations[i]
			break
		}
	}
	require.NotNil(t, symlinkOp, "Expected a symlink operation for .bashrc")
	assert.Equal(t, types.StatusConflict, symlinkOp.Status, "Operation status should be conflict")

	// Execute operations - the conflicting one should be skipped
	executor := NewSynthfsExecutor(false)
	executor.EnableHomeSymlinks(true)
	err = executor.ExecuteOperations(result.Operations)
	require.NoError(t, err, "Executor should handle skippable operations gracefully")

	// Verify the original file is untouched
	currentContent, err := os.ReadFile(targetPath)
	require.NoError(t, err)
	assert.Equal(t, preexistingContent, string(currentContent), "Original file should not be modified without --force")

	// ===== TEST 2: Run with --force flag =====

	// Run InstallPacks again with Force: true
	resultForce, err := InstallPacks(InstallPacksOptions{
		DotfilesRoot: testEnv.DotfilesRoot(),
		PackNames:    []string{"testpack"},
		DryRun:       false,
		Force:        true,
	})
	require.NoError(t, err)

	// Find the symlink operation again
	var symlinkOpForce *types.Operation
	for i, op := range resultForce.Operations {
		if op.Type == types.OperationCreateSymlink && op.Target == targetPath {
			symlinkOpForce = &resultForce.Operations[i]
			break
		}
	}
	require.NotNil(t, symlinkOpForce, "Expected a symlink operation for .bashrc in force mode")
	assert.Equal(t, types.StatusReady, symlinkOpForce.Status, "Operation status should be ready in force mode")

	// Execute operations with force mode enabled
	executor.EnableForce(true)
	err = executor.ExecuteOperations(resultForce.Operations)
	require.NoError(t, err)

	// Verify the file is now a symlink
	info, err := os.Lstat(targetPath)
	require.NoError(t, err)
	assert.True(t, info.Mode()&os.ModeSymlink != 0, "File should be a symlink after --force")

	// Verify the content of the symlinked file
	finalContent, err := os.ReadFile(targetPath)
	require.NoError(t, err)
	assert.Equal(t, dotfileContent, string(finalContent), "Symlinked file content should match the source")
}
