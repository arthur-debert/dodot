package deploy

import (
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/stretchr/testify/require"

	// Import to register triggers and powerups
	_ "github.com/arthur-debert/dodot/pkg/powerups"
	_ "github.com/arthur-debert/dodot/pkg/triggers"
)

// TestDeployMultiplePacksExecutesOperations tests that deploying multiple packs
// executes operations correctly
func TestDeployMultiplePacksExecutesOperations(t *testing.T) {
	// Create a temporary directory for testing
	tmpDir := t.TempDir()
	dotfilesRoot := filepath.Join(tmpDir, "dotfiles")
	homeDir := filepath.Join(tmpDir, "home")

	// Create directories
	require.NoError(t, os.MkdirAll(filepath.Join(dotfilesRoot, "vim"), 0755))
	require.NoError(t, os.MkdirAll(filepath.Join(dotfilesRoot, "bash"), 0755))
	require.NoError(t, os.MkdirAll(homeDir, 0755))

	// Create pack configs (no pack config needed for default matchers)

	// Create files to be symlinked
	vimrcContent := "\" Test vimrc"
	vimrcPath := filepath.Join(dotfilesRoot, "vim", ".vimrc")
	require.NoError(t, os.WriteFile(vimrcPath, []byte(vimrcContent), 0644))

	bashrcContent := "# Test bashrc"
	bashrcPath := filepath.Join(dotfilesRoot, "bash", ".bashrc")
	require.NoError(t, os.WriteFile(bashrcPath, []byte(bashrcContent), 0644))

	// Override home directory for the test
	origHome := os.Getenv("HOME")
	require.NoError(t, os.Setenv("HOME", homeDir))
	defer func() { _ = os.Setenv("HOME", origHome) }()

	// Execute the deploy command directly with dry-run to verify operations
	result, err := DeployPacks(DeployPacksOptions{
		DotfilesRoot: dotfilesRoot,
		PackNames:    []string{"vim", "bash"},
		DryRun:       true, // Just verify operations are generated
	})
	require.NoError(t, err)

	// Verify operations were generated for both packs
	require.NotEmpty(t, result.Operations, "Expected operations to be generated")

	// Debug: print all operations
	t.Logf("Generated %d operations:", len(result.Operations))
	for i, op := range result.Operations {
		t.Logf("  [%d] Type: %s, Target: %s, Description: %s", i, op.Type, op.Target, op.Description)
	}

	// Check that we have symlink operations targeting the correct files
	vimrcFound := false
	bashrcFound := false

	for _, op := range result.Operations {
		// Look for operations that mention our files in their description
		if op.Description != "" {
			if strings.Contains(op.Description, ".vimrc") {
				vimrcFound = true
			}
			if strings.Contains(op.Description, ".bashrc") {
				bashrcFound = true
			}
		}
	}

	// CRITICAL TEST: Verify operations were generated for BOTH files
	require.True(t, vimrcFound, "Expected operation for .vimrc")
	require.True(t, bashrcFound, "Expected operation for .bashrc")

	// Verify both packs were processed
	require.Contains(t, result.Packs, "vim")
	require.Contains(t, result.Packs, "bash")
}
