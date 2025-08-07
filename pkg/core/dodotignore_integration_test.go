package core

import (
	"strings"
	"testing"

	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/testutil"
	"github.com/arthur-debert/dodot/pkg/types"
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

	// Run the core pipeline directly
	// 1. Get pack candidates
	candidates, err := GetPackCandidates(root)
	testutil.AssertNoError(t, err)

	// 2. Get packs
	packs, err := GetPacks(candidates)
	testutil.AssertNoError(t, err)

	// 3. Process triggers for each pack
	var allMatches []types.TriggerMatch
	for _, pack := range packs {
		matches, err := ProcessPackTriggers(pack)
		testutil.AssertNoError(t, err)
		allMatches = append(allMatches, matches...)
	}

	// 4. Convert to actions
	actions, err := GetActions(allMatches)
	testutil.AssertNoError(t, err)

	// 5. Convert to operations
	// Verify packs were processed correctly
	packNames := make(map[string]bool)
	for _, pack := range packs {
		packNames[pack.Name] = true
	}

	// Should have vim-pack, mixed-pack, and config-pack
	testutil.AssertTrue(t, packNames["vim-pack"], "vim-pack should be processed")
	testutil.AssertTrue(t, packNames["mixed-pack"], "mixed-pack should be processed")
	testutil.AssertTrue(t, packNames["config-pack"], "config-pack should be processed")

	// Should NOT have private-pack
	testutil.AssertFalse(t, packNames["private-pack"],
		"private-pack with .dodotignore should not be processed")

	// Check that actions don't reference ignored files
	for _, action := range actions {
		// No actions should reference files in private directories
		if action.Source != "" {
			testutil.AssertTrue(t, !strings.Contains(action.Source, "private/credentials.txt"),
				"Files in .dodotignore directories should not generate actions")

			// No actions should reference ignored files from .dodot.toml
			testutil.AssertTrue(t, !strings.Contains(action.Source, "backup.bak"),
				"Files matching .dodot.toml ignore patterns should not generate actions")
			testutil.AssertTrue(t, !strings.Contains(action.Source, "temp/cache.tmp"),
				"Files in ignored directories should not generate actions")
		}
	}

	t.Logf("Processed %d packs with %d total actions", len(packs), len(actions))
}
