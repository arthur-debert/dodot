package core

import (
	"os"
	"path/filepath"
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
	actions, err := GetActionsV2(allMatches)
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
		// Get source file path based on action type
		var sourceFile string
		switch a := action.(type) {
		case *types.LinkAction:
			sourceFile = a.SourceFile
		case *types.RunScriptAction:
			sourceFile = a.ScriptPath
		case *types.BrewAction:
			sourceFile = a.BrewfilePath
		case *types.AddToPathAction:
			sourceFile = a.DirPath
		case *types.AddToShellProfileAction:
			sourceFile = a.ScriptPath
		}

		if sourceFile != "" {
			// No actions should reference files in private directories
			testutil.AssertTrue(t, !strings.Contains(sourceFile, "private/credentials.txt"),
				"Files in .dodotignore directories should not generate actions")

			// No actions should reference ignored files from .dodot.toml
			testutil.AssertTrue(t, !strings.Contains(sourceFile, "backup.bak"),
				"Files matching .dodot.toml ignore patterns should not generate actions")
			testutil.AssertTrue(t, !strings.Contains(sourceFile, "temp/cache.tmp"),
				"Files in ignored directories should not generate actions")
		}
	}

	t.Logf("Processed %d packs with %d total actions", len(packs), len(actions))
}

func TestDodotIgnoreFunctionality(t *testing.T) {
	t.Run("shouldIgnorePack function", func(t *testing.T) {
		// Create test directory
		packDir := testutil.TempDir(t, "test-pack")

		// Test without .dodotignore
		testutil.AssertFalse(t, shouldIgnorePack(packDir),
			"Pack without .dodotignore should not be ignored")

		// Create .dodotignore file
		testutil.CreateFile(t, packDir, ".dodotignore", "")

		// Test with .dodotignore
		testutil.AssertTrue(t, shouldIgnorePack(packDir),
			"Pack with .dodotignore should be ignored")
	})

	t.Run("directory traversal skips .dodotignore dirs", func(t *testing.T) {
		// Create test directory structure
		packDir := testutil.TempDir(t, "test-pack")

		// Create files and directories
		testutil.CreateFile(t, packDir, "root.txt", "content")

		normalDir := testutil.CreateDir(t, packDir, "normal")
		testutil.CreateFile(t, normalDir, "file1.txt", "content")

		ignoredDir := testutil.CreateDir(t, packDir, "ignored")
		testutil.CreateFile(t, ignoredDir, ".dodotignore", "")
		testutil.CreateFile(t, ignoredDir, "file2.txt", "should be ignored")

		// Track visited files
		visitedFiles := make(map[string]bool)

		// Walk the directory
		err := filepath.Walk(packDir, func(path string, info os.FileInfo, err error) error {
			if err != nil {
				return err
			}

			// Skip pack root
			if path == packDir {
				return nil
			}

			relPath, _ := filepath.Rel(packDir, path)

			// Check for .dodotignore in directories
			if info.IsDir() {
				if _, err := os.Stat(filepath.Join(path, ".dodotignore")); err == nil {
					return filepath.SkipDir
				}
			}

			if !info.IsDir() {
				visitedFiles[relPath] = true
			}

			return nil
		})

		testutil.AssertNoError(t, err)

		// Check that expected files were visited
		testutil.AssertTrue(t, visitedFiles["root.txt"],
			"root.txt should have been visited")
		testutil.AssertTrue(t, visitedFiles["normal/file1.txt"],
			"normal/file1.txt should have been visited")

		// Check that ignored files were not visited
		testutil.AssertFalse(t, visitedFiles["ignored/file2.txt"],
			"ignored/file2.txt should not have been visited")
		testutil.AssertFalse(t, visitedFiles["ignored/.dodotignore"],
			"ignored/.dodotignore should not have been visited")
	})
}

func TestGetPacksWithDodotIgnore(t *testing.T) {
	// Create dotfiles directory
	root := testutil.TempDir(t, "dotfiles")

	// Create normal pack
	pack1 := testutil.CreateDir(t, root, "normal-pack")
	testutil.CreateFile(t, pack1, "file.txt", "content")

	// Create pack with .dodotignore
	pack2 := testutil.CreateDir(t, root, "ignored-pack")
	testutil.CreateFile(t, pack2, ".dodotignore", "")
	testutil.CreateFile(t, pack2, "file.txt", "content")

	// Create another normal pack
	pack3 := testutil.CreateDir(t, root, "another-pack")
	testutil.CreateFile(t, pack3, "file.txt", "content")

	// Get pack candidates
	candidates, err := GetPackCandidates(root)
	testutil.AssertNoError(t, err)
	testutil.AssertEqual(t, 3, len(candidates), "Should find all 3 directories as candidates")

	// Get packs (should skip the one with .dodotignore)
	packs, err := GetPacks(candidates)
	testutil.AssertNoError(t, err)
	testutil.AssertEqual(t, 2, len(packs), "Should only load 2 packs (skipping the one with .dodotignore)")

	// Check pack names
	packNames := make(map[string]bool)
	for _, pack := range packs {
		packNames[pack.Name] = true
	}

	testutil.AssertTrue(t, packNames["normal-pack"], "normal-pack should be loaded")
	testutil.AssertTrue(t, packNames["another-pack"], "another-pack should be loaded")
	testutil.AssertFalse(t, packNames["ignored-pack"], "ignored-pack should not be loaded")
}
