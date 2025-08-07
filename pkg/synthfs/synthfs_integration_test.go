package synthfs

import (
	"path/filepath"
	"strings"
	"testing"

	"github.com/arthur-debert/dodot/pkg/paths"
	"github.com/arthur-debert/dodot/pkg/testutil"
	"github.com/arthur-debert/dodot/pkg/types"
)

func TestSynthfsExecutor_Integration(t *testing.T) {
	// Create a test environment
	tempHome := testutil.TempDir(t, "synthfs-integration")
	t.Setenv("HOME", tempHome)
	t.Setenv("DODOT_DATA_DIR", filepath.Join(tempHome, ".local", "share", "dodot"))
	t.Setenv("DOTFILES_ROOT", filepath.Join(tempHome, "dotfiles"))

	// Create necessary directories
	dataDir := filepath.Join(tempHome, ".local", "share", "dodot")
	testutil.CreateDir(t, tempHome, ".local")
	testutil.CreateDir(t, filepath.Join(tempHome, ".local"), "share")
	testutil.CreateDir(t, filepath.Join(tempHome, ".local", "share"), "dodot")
	testutil.CreateDir(t, dataDir, "deployed")
	testutil.CreateDir(t, dataDir, "shell")

	// Create paths and executor
	p, err := paths.New("")
	testutil.AssertNoError(t, err)
	executor := NewSynthfsExecutorWithPaths(false, p)

	// Define operations to execute
	operations := []types.Operation{
		{
			Type:        types.OperationCreateDir,
			Target:      filepath.Join(dataDir, "test-dir"),
			Description: "Create test directory",
			Status:      types.StatusReady,
		},
		{
			Type:        types.OperationWriteFile,
			Target:      filepath.Join(dataDir, "test.txt"),
			Content:     "Hello from synthfs!",
			Mode:        modePtr(0644),
			Description: "Write test file",
			Status:      types.StatusReady,
		},
		{
			Type:        types.OperationCreateDir,
			Target:      filepath.Join(dataDir, "deployed", "symlink"),
			Description: "Create symlink parent directory",
			Status:      types.StatusReady,
		},
		{
			Type:        types.OperationWriteFile,
			Target:      filepath.Join(dataDir, "shell", "init.sh"),
			Content:     "#!/bin/bash\necho 'Shell initialized'",
			Mode:        modePtr(0755),
			Description: "Write shell script",
			Status:      types.StatusReady,
		},
	}

	// Execute operations
	_, err = executor.ExecuteOperations(operations)
	testutil.AssertNoError(t, err)

	// Verify results
	testutil.AssertTrue(t, testutil.DirExists(t, filepath.Join(dataDir, "test-dir")),
		"Directory should have been created")

	testutil.AssertTrue(t, testutil.FileExists(t, filepath.Join(dataDir, "test.txt")),
		"File should have been created")

	content := testutil.ReadFile(t, filepath.Join(dataDir, "test.txt"))
	testutil.AssertEqual(t, "Hello from synthfs!", content)

	// Verify shell script
	testutil.AssertTrue(t, testutil.FileExists(t, filepath.Join(dataDir, "shell", "init.sh")),
		"Shell script should have been created")
	shellContent := testutil.ReadFile(t, filepath.Join(dataDir, "shell", "init.sh"))
	testutil.AssertEqual(t, "#!/bin/bash\necho 'Shell initialized'", shellContent)
}

// Note: Validation tests have been moved to pkg/validation/paths_test.go
// This test now focuses on actual execution errors, not validation errors

func TestSynthfsExecutor_Integration_Errors(t *testing.T) {
	// Create a test environment
	tempHome := testutil.TempDir(t, "synthfs-errors")
	t.Setenv("HOME", tempHome)
	t.Setenv("DODOT_DATA_DIR", filepath.Join(tempHome, ".local", "share", "dodot"))
	t.Setenv("DOTFILES_ROOT", filepath.Join(tempHome, "dotfiles"))

	// Create necessary directories
	testutil.CreateDir(t, tempHome, ".local")
	testutil.CreateDir(t, filepath.Join(tempHome, ".local"), "share")
	testutil.CreateDir(t, filepath.Join(tempHome, ".local", "share"), "dodot")
	dataDir := filepath.Join(tempHome, ".local", "share", "dodot")

	// Create paths and executor
	p, err := paths.New("")
	testutil.AssertNoError(t, err)
	executor := NewSynthfsExecutorWithPaths(false, p)

	// Test execution error - trying to create symlink when target already exists
	existingFile := filepath.Join(dataDir, "existing.txt")
	testutil.CreateFile(t, dataDir, "existing.txt", "existing content")
	sourceFile := filepath.Join(dataDir, "source.txt")
	testutil.CreateFile(t, dataDir, "source.txt", "source content")

	operations := []types.Operation{
		{
			Type:        types.OperationCreateSymlink,
			Source:      sourceFile,
			Target:      existingFile,
			Description: "Attempt to create symlink over existing file",
			Status:      types.StatusReady,
		},
	}

	_, err = executor.ExecuteOperations(operations)
	testutil.AssertError(t, err)
	// synthfs will fail because file already exists
	if !strings.Contains(err.Error(), "already exists") && !strings.Contains(err.Error(), "file exists") {
		t.Errorf("Expected error about file already existing, got: %v", err)
	}
}
