package core

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/arthur-debert/dodot/pkg/testutil"
)

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