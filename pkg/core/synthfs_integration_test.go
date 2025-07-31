package core

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
		},
		{
			Type:        types.OperationWriteFile,
			Target:      filepath.Join(dataDir, "test.txt"),
			Content:     "Hello from synthfs!",
			Mode:        modePtr(0644),
			Description: "Write test file",
		},
		{
			Type:        types.OperationCreateDir,
			Target:      filepath.Join(dataDir, "deployed", "symlink"),
			Description: "Create symlink parent directory",
		},
		{
			Type:        types.OperationWriteFile,
			Target:      filepath.Join(dataDir, "source.txt"),
			Content:     "Symlink source content",
			Mode:        modePtr(0644),
			Description: "Create symlink source",
		},
		{
			Type:        types.OperationCreateSymlink,
			Source:      filepath.Join(dataDir, "source.txt"),
			Target:      filepath.Join(dataDir, "deployed", "symlink", "link.txt"),
			Description: "Create test symlink",
		},
		{
			Type:        types.OperationWriteFile,
			Target:      filepath.Join(dataDir, "shell", "init.sh"),
			Content:     "#!/bin/bash\necho 'Shell initialized'",
			Mode:        modePtr(0755),
			Description: "Write shell script",
		},
	}

	// Execute operations
	err = executor.ExecuteOperations(operations)
	testutil.AssertNoError(t, err)

	// Verify results
	testutil.AssertTrue(t, testutil.DirExists(t, filepath.Join(dataDir, "test-dir")),
		"Directory should have been created")

	testutil.AssertTrue(t, testutil.FileExists(t, filepath.Join(dataDir, "test.txt")),
		"File should have been created")

	content := testutil.ReadFile(t, filepath.Join(dataDir, "test.txt"))
	testutil.AssertEqual(t, "Hello from synthfs!", content)

	testutil.AssertTrue(t, testutil.FileExists(t, filepath.Join(dataDir, "source.txt")),
		"Symlink source should have been created")

	testutil.AssertTrue(t, testutil.FileExists(t, filepath.Join(dataDir, "deployed", "symlink", "link.txt")),
		"Symlink should have been created")

	// Verify symlink points to correct target
	linkTarget, err := filepath.EvalSymlinks(filepath.Join(dataDir, "deployed", "symlink", "link.txt"))
	testutil.AssertNoError(t, err)
	// Also evaluate the expected path to handle macOS /var -> /private/var conversion
	expectedTarget, err := filepath.EvalSymlinks(filepath.Join(dataDir, "source.txt"))
	testutil.AssertNoError(t, err)
	testutil.AssertEqual(t, expectedTarget, linkTarget)

	// Verify shell script
	testutil.AssertTrue(t, testutil.FileExists(t, filepath.Join(dataDir, "shell", "init.sh")),
		"Shell script should have been created")
	shellContent := testutil.ReadFile(t, filepath.Join(dataDir, "shell", "init.sh"))
	testutil.AssertEqual(t, "#!/bin/bash\necho 'Shell initialized'", shellContent)
}

func TestSynthfsExecutor_Integration_Errors(t *testing.T) {
	// Create a test environment
	tempHome := testutil.TempDir(t, "synthfs-errors")
	t.Setenv("HOME", tempHome)
	t.Setenv("DODOT_DATA_DIR", filepath.Join(tempHome, ".local", "share", "dodot"))

	// Create necessary directories
	testutil.CreateDir(t, tempHome, ".local")
	testutil.CreateDir(t, filepath.Join(tempHome, ".local"), "share")
	testutil.CreateDir(t, filepath.Join(tempHome, ".local", "share"), "dodot")

	// Create paths and executor
	p, err := paths.New("")
	testutil.AssertNoError(t, err)
	executor := NewSynthfsExecutorWithPaths(false, p)

	// Test operation outside safe directory
	operations := []types.Operation{
		{
			Type:        types.OperationWriteFile,
			Target:      "/etc/passwd",
			Content:     "This should fail",
			Description: "Attempt to write to system file",
		},
	}

	err = executor.ExecuteOperations(operations)
	testutil.AssertError(t, err)
	if !strings.Contains(err.Error(), "outside dodot-controlled directories") {
		t.Errorf("Expected error to contain 'outside dodot-controlled directories', got: %v", err)
	}
}
