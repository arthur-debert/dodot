package commands

import (
	"testing"

	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/testutil"
)

func init() {
	// Set up logging for tests
	logging.SetupLogger(0)
}

func TestDodotIgnoreIntegration(t *testing.T) {
	// Create a complete dotfiles structure
	root := testutil.TempDir(t, "dotfiles")

	// Pack 1: Normal pack
	pack1 := testutil.CreateDir(t, root, "vim-pack")
	testutil.CreateFile(t, pack1, ".vimrc", "set number")
	testutil.CreateFile(t, pack1, "README.txxt", "Vim configuration")

	// Pack 2: Pack with .dodotignore (should be skipped)
	pack2 := testutil.CreateDir(t, root, "private-pack")
	testutil.CreateFile(t, pack2, ".dodotignore", "")
	testutil.CreateFile(t, pack2, "secret.conf", "private data")

	// Pack 3: Pack with directory containing .dodotignore
	pack3 := testutil.CreateDir(t, root, "mixed-pack")
	testutil.CreateFile(t, pack3, "public.conf", "public config")

	privateDir := testutil.CreateDir(t, pack3, "private")
	testutil.CreateFile(t, privateDir, ".dodotignore", "")
	testutil.CreateFile(t, privateDir, "credentials.txt", "secret")

	publicDir := testutil.CreateDir(t, pack3, "public")
	testutil.CreateFile(t, publicDir, "shared.conf", "shared config")

	// Pack 4: Pack with .dodot.toml ignore rules
	pack4 := testutil.CreateDir(t, root, "config-pack")
	configContent := `
[[ignore]]
path = "*.bak"

[[ignore]]
path = "temp/*"
`
	testutil.CreateFile(t, pack4, ".dodot.toml", configContent)
	testutil.CreateFile(t, pack4, "app.conf", "app config")
	testutil.CreateFile(t, pack4, "backup.bak", "backup file")

	tempDir := testutil.CreateDir(t, pack4, "temp")
	testutil.CreateFile(t, tempDir, "cache.tmp", "temp file")

	// Run the full pipeline
	result, err := DeployPacks(DeployPacksOptions{
		DotfilesRoot: root,
		DryRun:       true,
	})

	testutil.AssertNoError(t, err)

	// Verify packs were processed correctly
	packNames := make(map[string]bool)
	for _, packName := range result.Packs {
		packNames[packName] = true
	}

	// Should have vim-pack, mixed-pack, and config-pack
	testutil.AssertTrue(t, packNames["vim-pack"], "vim-pack should be processed")
	testutil.AssertTrue(t, packNames["mixed-pack"], "mixed-pack should be processed")
	testutil.AssertTrue(t, packNames["config-pack"], "config-pack should be processed")

	// Should NOT have private-pack
	testutil.AssertFalse(t, packNames["private-pack"],
		"private-pack with .dodotignore should not be processed")

	// Check that operations don't reference ignored files
	for _, op := range result.Operations {
		// No operations should reference files in private directories
		if op.Source != "" {
			testutil.AssertTrue(t, op.Source != "private/credentials.txt",
				"Files in .dodotignore directories should not generate operations")

			// No operations should reference ignored files from .dodot.toml
			testutil.AssertTrue(t, op.Source != "backup.bak",
				"Files matching .dodot.toml ignore patterns should not generate operations")
			testutil.AssertTrue(t, op.Source != "temp/cache.tmp",
				"Files in ignored directories should not generate operations")
		}
	}

	t.Logf("Processed %d packs with %d total operations", len(result.Packs), len(result.Operations))
}
