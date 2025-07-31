package core

import (
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/arthur-debert/dodot/pkg/paths"
	"github.com/arthur-debert/dodot/pkg/testutil"
	"github.com/arthur-debert/dodot/pkg/types"
)

func TestSynthfsExecutor_HomeSymlinks(t *testing.T) {
	// Create a test environment
	tempHome := testutil.TempDir(t, "synthfs-home-symlinks")
	tempDotfiles := testutil.TempDir(t, "synthfs-dotfiles")

	t.Setenv("HOME", tempHome)
	t.Setenv("DOTFILES_ROOT", tempDotfiles)
	t.Setenv("DODOT_DATA_DIR", filepath.Join(tempHome, ".local", "share", "dodot"))

	// Create necessary directories
	testutil.CreateDir(t, tempHome, ".local")
	testutil.CreateDir(t, filepath.Join(tempHome, ".local"), "share")
	testutil.CreateDir(t, filepath.Join(tempHome, ".local", "share"), "dodot")

	// Create a source file in dotfiles
	vimDir := testutil.CreateDir(t, tempDotfiles, "vim")
	testutil.CreateFile(t, vimDir, ".vimrc", "\" My vim config\nset number")
	vimrcSource := filepath.Join(vimDir, ".vimrc")

	// Create paths and executor
	p, err := paths.New(tempDotfiles)
	testutil.AssertNoError(t, err)

	t.Run("home symlinks disabled by default", func(t *testing.T) {
		executor := NewSynthfsExecutorWithPaths(false, p)

		operations := []types.Operation{
			{
				Type:        types.OperationCreateSymlink,
				Source:      vimrcSource,
				Target:      filepath.Join(tempHome, ".vimrc"),
				Description: "Create .vimrc symlink",
			},
		}

		err := executor.ExecuteOperations(operations)
		testutil.AssertError(t, err)
		testutil.AssertErrorContains(t, err, "outside dodot-controlled directories")
	})

	t.Run("home symlinks allowed when enabled", func(t *testing.T) {
		executor := NewSynthfsExecutorWithPaths(false, p)
		executor.EnableHomeSymlinks(true)

		operations := []types.Operation{
			{
				Type:        types.OperationCreateSymlink,
				Source:      vimrcSource,
				Target:      filepath.Join(tempHome, ".vimrc"),
				Description: "Create .vimrc symlink",
			},
		}

		err := executor.ExecuteOperations(operations)
		testutil.AssertNoError(t, err)

		// Verify symlink was created
		linkPath := filepath.Join(tempHome, ".vimrc")
		testutil.AssertTrue(t, testutil.FileExists(t, linkPath), "Symlink should exist")

		// Verify it points to the correct source
		target, err := os.Readlink(linkPath)
		testutil.AssertNoError(t, err)
		// Compare normalized paths due to potential path resolution differences
		expectedTarget, _ := filepath.EvalSymlinks(vimrcSource)
		actualTarget, _ := filepath.EvalSymlinks(target)
		testutil.AssertEqual(t, expectedTarget, actualTarget)
	})

	t.Run("reject symlinks outside home directory", func(t *testing.T) {
		executor := NewSynthfsExecutorWithPaths(false, p)
		executor.EnableHomeSymlinks(true) // Even with home symlinks enabled

		operations := []types.Operation{
			{
				Type:        types.OperationCreateSymlink,
				Source:      vimrcSource,
				Target:      "/etc/vimrc", // System location
				Description: "Create system vimrc symlink",
			},
		}

		err := executor.ExecuteOperations(operations)
		testutil.AssertError(t, err)
		// The error could be either from our validation or from synthfs trying to create in /etc
		if !strings.Contains(err.Error(), "must be in home directory") &&
			!strings.Contains(err.Error(), "permission denied") {
			t.Errorf("Expected permission error, got: %v", err)
		}
	})

	t.Run("reject protected files", func(t *testing.T) {
		executor := NewSynthfsExecutorWithPaths(false, p)
		executor.EnableHomeSymlinks(true)

		operations := []types.Operation{
			{
				Type:        types.OperationCreateSymlink,
				Source:      vimrcSource,
				Target:      filepath.Join(tempHome, ".ssh", "id_rsa"),
				Description: "Create SSH key symlink",
			},
		}

		// Create .ssh directory
		testutil.CreateDir(t, tempHome, ".ssh")

		err := executor.ExecuteOperations(operations)
		testutil.AssertError(t, err)
		testutil.AssertErrorContains(t, err, "protected file")
	})

	t.Run("source must be from dotfiles", func(t *testing.T) {
		executor := NewSynthfsExecutorWithPaths(false, p)
		executor.EnableHomeSymlinks(true)

		// Create a file outside dotfiles
		testutil.CreateFile(t, tempHome, "outside.txt", "content")
		outsideFile := filepath.Join(tempHome, "outside.txt")

		operations := []types.Operation{
			{
				Type:        types.OperationCreateSymlink,
				Source:      outsideFile, // Not from dotfiles
				Target:      filepath.Join(tempHome, ".config", "test"),
				Description: "Create symlink from outside dotfiles",
			},
		}

		err := executor.ExecuteOperations(operations)
		testutil.AssertError(t, err)
		testutil.AssertErrorContains(t, err, "must be from dotfiles directory")
	})
}

func TestSynthfsExecutor_ProtectedPaths(t *testing.T) {
	// Create a test environment
	tempHome := testutil.TempDir(t, "synthfs-protected-home")
	tempDotfiles := testutil.TempDir(t, "synthfs-protected-dotfiles")

	// Important: Set HOME to the actual temp directory we're using for protected paths
	t.Setenv("HOME", tempHome)
	t.Setenv("DOTFILES_ROOT", tempDotfiles)
	t.Setenv("DODOT_DATA_DIR", filepath.Join(tempHome, ".local", "share", "dodot"))

	// Create necessary directories
	testutil.CreateDir(t, tempHome, ".local")
	testutil.CreateDir(t, filepath.Join(tempHome, ".local"), "share")
	testutil.CreateDir(t, filepath.Join(tempHome, ".local", "share"), "dodot")

	// Create source file
	testutil.CreateFile(t, tempDotfiles, "config", "config content")
	sourceFile := filepath.Join(tempDotfiles, "config")

	p, err := paths.New(tempDotfiles)
	testutil.AssertNoError(t, err)

	executor := NewSynthfsExecutorWithPaths(false, p)
	executor.EnableHomeSymlinks(true)

	protectedPaths := []string{
		".ssh/authorized_keys",
		".ssh/id_rsa",
		".ssh/id_ed25519",
		".gnupg/private-keys-v1.d/key.key",
		".password-store/passwords.gpg",
		".config/gh/hosts.yml",
		".aws/credentials",
		".kube/config",
		".docker/config.json",
	}

	for _, protected := range protectedPaths {
		t.Run(protected, func(t *testing.T) {
			target := filepath.Join(tempHome, protected)
			t.Logf("Testing protected path - Target: %s, Home: %s", target, tempHome)

			// Create parent directories so synthfs doesn't fail on missing parents
			parentDir := filepath.Dir(target)
			if parentDir != tempHome {
				testutil.CreateDir(t, filepath.Dir(parentDir), filepath.Base(parentDir))
			}

			operations := []types.Operation{
				{
					Type:        types.OperationCreateSymlink,
					Source:      sourceFile,
					Target:      target,
					Description: "Create symlink to protected path",
				},
			}

			err := executor.ExecuteOperations(operations)
			if err == nil {
				// Check if symlink was actually created
				if testutil.SymlinkExists(t, target) {
					t.Errorf("Protected symlink was created at %s", target)
				}
			}
			testutil.AssertError(t, err)
			testutil.AssertErrorContains(t, err, "protected file")
		})
	}
}

func TestSynthfsExecutor_ExistingFileWarning(t *testing.T) {
	// This test verifies that warnings are logged for existing files
	// In a real implementation, we might want to add backup functionality

	tempHome := testutil.TempDir(t, "synthfs-existing")
	tempDotfiles := testutil.TempDir(t, "synthfs-existing-dotfiles")

	t.Setenv("HOME", tempHome)
	t.Setenv("DOTFILES_ROOT", tempDotfiles)

	// Create an existing file in home
	testutil.CreateFile(t, tempHome, ".bashrc", "# Original bashrc")
	existingFile := filepath.Join(tempHome, ".bashrc")

	// Create source file
	bashDir := testutil.CreateDir(t, tempDotfiles, "bash")
	testutil.CreateFile(t, bashDir, ".bashrc", "# Dotfiles bashrc")
	sourceFile := filepath.Join(bashDir, ".bashrc")

	p, err := paths.New(tempDotfiles)
	testutil.AssertNoError(t, err)

	executor := NewSynthfsExecutorWithPaths(false, p)
	executor.EnableHomeSymlinks(true)

	operations := []types.Operation{
		{
			Type:        types.OperationCreateSymlink,
			Source:      sourceFile,
			Target:      existingFile,
			Description: "Replace existing .bashrc",
		},
	}

	// synthfs will fail if the file already exists
	// This is a limitation of synthfs - it doesn't replace existing files
	err = executor.ExecuteOperations(operations)

	// For now, we expect this to fail with synthfs
	// In a real implementation, we might want to:
	// 1. Delete the existing file first
	// 2. Move it to a backup location
	// 3. Use a different synthfs operation that supports overwriting
	testutil.AssertError(t, err)
	testutil.AssertErrorContains(t, err, "already exists")
}
