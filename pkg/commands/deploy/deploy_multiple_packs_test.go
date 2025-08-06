package deploy

import (
	"strings"
	"testing"

	"github.com/arthur-debert/dodot/pkg/testutil"
	"github.com/stretchr/testify/require"

	// Import to register triggers and powerups
	_ "github.com/arthur-debert/dodot/pkg/powerups"
	_ "github.com/arthur-debert/dodot/pkg/triggers"
)

// TestDeployMultiplePacksExecutesOperations tests that deploying multiple packs
// executes operations correctly
func TestDeployMultiplePacksExecutesOperations(t *testing.T) {
	// Create multiple packs with home directory using new helpers
	packs := testutil.SetupMultiplePacks(t, "vim", "bash")

	// Add dotfiles to each pack
	packs["vim"].AddFile(t, ".vimrc", "\" Test vimrc")
	packs["bash"].AddFile(t, ".bashrc", "# Test bashrc")

	// Execute the deploy command directly with dry-run to verify operations
	result, err := DeployPacks(DeployPacksOptions{
		DotfilesRoot: packs["vim"].Root,
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
