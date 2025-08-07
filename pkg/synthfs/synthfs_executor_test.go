package synthfs

import (
	"path/filepath"
	"testing"

	"github.com/arthur-debert/dodot/pkg/paths"
	"github.com/arthur-debert/dodot/pkg/testutil"
	"github.com/arthur-debert/dodot/pkg/types"
)

func modePtr(mode uint32) *uint32 {
	return &mode
}

// Note: Path validation tests have been moved to pkg/validation/paths_test.go
// since validation is now done earlier in the pipeline during operation conversion

func TestSynthfsExecutor_ExecuteOperations(t *testing.T) {
	// Use a temp directory that mimics dodot structure
	tempDir := testutil.TempDir(t, "synthfs-test")
	t.Setenv("HOME", tempDir)
	t.Setenv("DODOT_DATA_DIR", filepath.Join(tempDir, ".local", "share", "dodot"))
	t.Setenv("DOTFILES_ROOT", filepath.Join(tempDir, "dotfiles"))

	dataDir := filepath.Join(tempDir, ".local", "share", "dodot")
	testutil.CreateDir(t, tempDir, ".local")
	testutil.CreateDir(t, filepath.Join(tempDir, ".local"), "share")
	testutil.CreateDir(t, filepath.Join(tempDir, ".local", "share"), "dodot")
	testutil.CreateDir(t, dataDir, "deployed")
	testutil.CreateDir(t, tempDir, "dotfiles")

	// Create paths and executor
	p, err := paths.New("")
	testutil.AssertNoError(t, err)
	executor := NewSynthfsExecutorWithPaths(false, p)

	tests := []struct {
		name       string
		operations []types.Operation
		expectErr  bool
		checkFunc  func(t *testing.T)
	}{
		{
			name: "create directory",
			operations: []types.Operation{
				{
					Type:        types.OperationCreateDir,
					Target:      filepath.Join(dataDir, "test-dir"),
					Description: "Create test directory",
					Status:      types.StatusReady,
				},
			},
			expectErr: false,
			checkFunc: func(t *testing.T) {
				testutil.AssertTrue(t, testutil.DirExists(t, filepath.Join(dataDir, "test-dir")),
					"Directory should have been created")
			},
		},
		{
			name: "write file",
			operations: []types.Operation{
				{
					Type:        types.OperationWriteFile,
					Target:      filepath.Join(dataDir, "test.txt"),
					Content:     "Hello, World!",
					Mode:        modePtr(0644),
					Description: "Write test file",
					Status:      types.StatusReady,
				},
			},
			expectErr: false,
			checkFunc: func(t *testing.T) {
				content := testutil.ReadFile(t, filepath.Join(dataDir, "test.txt"))
				testutil.AssertEqual(t, "Hello, World!", content)
			},
		},
		{
			name: "operation outside safe directory",
			operations: []types.Operation{
				{
					Type:        types.OperationWriteFile,
					Target:      "/tmp/unsafe.txt",
					Content:     "Should fail",
					Description: "Unsafe write",
					Status:      types.StatusReady,
				},
			},
			expectErr: true,
			checkFunc: func(t *testing.T) {
				// Nothing to check - operation should fail
			},
		},
		{
			name: "skip non-ready operations",
			operations: []types.Operation{
				{
					Type:        types.OperationCreateDir,
					Target:      filepath.Join(dataDir, "skipped-dir"),
					Description: "Should be skipped",
					Status:      types.StatusSkipped,
				},
			},
			expectErr: false,
			checkFunc: func(t *testing.T) {
				testutil.AssertFalse(t, testutil.DirExists(t, filepath.Join(dataDir, "skipped-dir")),
					"Directory should not have been created")
			},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			_, err := executor.ExecuteOperations(tt.operations)
			if tt.expectErr {
				testutil.AssertError(t, err)
			} else {
				testutil.AssertNoError(t, err)
			}
			tt.checkFunc(t)
		})
	}
}

func TestSynthfsExecutor_DryRun(t *testing.T) {
	// Use a temp directory that mimics dodot structure
	tempDir := testutil.TempDir(t, "synthfs-dryrun")
	t.Setenv("HOME", tempDir)
	t.Setenv("DODOT_DATA_DIR", filepath.Join(tempDir, ".local", "share", "dodot"))
	t.Setenv("DOTFILES_ROOT", filepath.Join(tempDir, "dotfiles"))

	dataDir := filepath.Join(tempDir, ".local", "share", "dodot")
	testutil.CreateDir(t, tempDir, ".local")
	testutil.CreateDir(t, filepath.Join(tempDir, ".local"), "share")
	testutil.CreateDir(t, filepath.Join(tempDir, ".local", "share"), "dodot")

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
			Content:     "Hello, World!",
			Description: "Write test file",
			Status:      types.StatusReady,
		},
	}

	// Create paths and executor
	p, err := paths.New("")
	testutil.AssertNoError(t, err)
	executor := NewSynthfsExecutorWithPaths(true, p) // dry run mode

	// Execute in dry run mode
	_, err = executor.ExecuteOperations(operations)
	testutil.AssertNoError(t, err)

	// Verify nothing was actually created
	testutil.AssertFalse(t, testutil.DirExists(t, filepath.Join(dataDir, "test-dir")),
		"Directory should not exist in dry run mode")
	testutil.AssertFalse(t, testutil.FileExists(t, filepath.Join(dataDir, "test.txt")),
		"File should not exist in dry run mode")
}

func TestSynthfsExecutor_ExecuteOperations_EmptyList(t *testing.T) {
	executor := NewSynthfsExecutor(false)

	// Execute with empty operations list
	_, err := executor.ExecuteOperations([]types.Operation{})
	testutil.AssertNoError(t, err)
}

func TestSynthfsExecutor_SkipNonMutatingOperations(t *testing.T) {
	// Use a temp directory that mimics dodot structure
	tempDir := testutil.TempDir(t, "synthfs-skip")
	t.Setenv("HOME", tempDir)
	t.Setenv("DODOT_DATA_DIR", filepath.Join(tempDir, ".local", "share", "dodot"))
	t.Setenv("DOTFILES_ROOT", filepath.Join(tempDir, "dotfiles"))

	dataDir := filepath.Join(tempDir, ".local", "share", "dodot")
	testutil.CreateDir(t, tempDir, ".local")
	testutil.CreateDir(t, filepath.Join(tempDir, ".local"), "share")
	testutil.CreateDir(t, filepath.Join(tempDir, ".local", "share"), "dodot")

	// Create paths and executor
	p, err := paths.New("")
	testutil.AssertNoError(t, err)
	executor := NewSynthfsExecutorWithPaths(false, p)

	operations := []types.Operation{
		{
			Type:        types.OperationReadFile,
			Source:      filepath.Join(dataDir, "test.txt"),
			Description: "Read test file",
			Status:      types.StatusReady,
		},
		{
			Type:        types.OperationChecksum,
			Source:      filepath.Join(dataDir, "test.txt"),
			Description: "Calculate checksum",
			Status:      types.StatusReady,
		},
		{
			Type:        types.OperationCreateDir,
			Target:      filepath.Join(dataDir, "real-dir"),
			Description: "Create real directory",
			Status:      types.StatusReady,
		},
	}

	// Execute operations
	_, err = executor.ExecuteOperations(operations)
	testutil.AssertNoError(t, err)

	// Only the create directory operation should have been executed
	testutil.AssertTrue(t, testutil.DirExists(t, filepath.Join(dataDir, "real-dir")),
		"Directory should have been created")
}

func TestSynthfsExecutor_Symlink(t *testing.T) {
	// Use a temp directory that mimics dodot structure
	tempDir := testutil.TempDir(t, "synthfs-symlink")
	t.Setenv("HOME", tempDir)
	t.Setenv("DODOT_DATA_DIR", filepath.Join(tempDir, ".local", "share", "dodot"))
	t.Setenv("DOTFILES_ROOT", filepath.Join(tempDir, "dotfiles"))

	dataDir := filepath.Join(tempDir, ".local", "share", "dodot")
	dotfilesDir := filepath.Join(tempDir, "dotfiles")

	testutil.CreateDir(t, tempDir, ".local")
	testutil.CreateDir(t, filepath.Join(tempDir, ".local"), "share")
	testutil.CreateDir(t, filepath.Join(tempDir, ".local", "share"), "dodot")
	testutil.CreateDir(t, dataDir, "deployed")
	testutil.CreateDir(t, tempDir, "dotfiles")

	// Create a source file in dotfiles
	sourceFile := testutil.CreateFile(t, dotfilesDir, "test.conf", "config content")

	// Create paths and executor
	p, err := paths.New(dotfilesDir)
	testutil.AssertNoError(t, err)
	// Enable home symlinks because in the test environment, everything is under temp home
	executor := NewSynthfsExecutorWithPaths(false, p).EnableHomeSymlinks(false)

	// Test symlink within safe directories
	operations := []types.Operation{
		{
			Type:        types.OperationCreateSymlink,
			Source:      sourceFile,
			Target:      filepath.Join(dataDir, "deployed", "test.conf"),
			Description: "Create symlink",
			Status:      types.StatusReady,
		},
	}

	_, err = executor.ExecuteOperations(operations)
	testutil.AssertNoError(t, err)

	// Verify symlink was created
	testutil.AssertTrue(t, testutil.FileExists(t, filepath.Join(dataDir, "deployed", "test.conf")),
		"Symlink should have been created")
}

func TestSynthfsExecutor_Rollback(t *testing.T) {
	// Use a temp directory that mimics dodot structure
	tempDir := testutil.TempDir(t, "synthfs-rollback")
	t.Setenv("HOME", tempDir)
	t.Setenv("DODOT_DATA_DIR", filepath.Join(tempDir, ".local", "share", "dodot"))
	t.Setenv("DOTFILES_ROOT", filepath.Join(tempDir, "dotfiles"))

	testutil.CreateDir(t, tempDir, ".local")
	testutil.CreateDir(t, filepath.Join(tempDir, ".local"), "share")
	testutil.CreateDir(t, filepath.Join(tempDir, ".local", "share"), "dodot")

	// Create paths and executor with rollback enabled (default)
	p, err := paths.New("")
	testutil.AssertNoError(t, err)
	executor := NewSynthfsExecutorWithPaths(false, p)

	t.Run("rollback enabled by default", func(t *testing.T) {
		// The executor should have rollback enabled by default
		testutil.AssertTrue(t, executor.enableRollback, "Rollback should be enabled by default")
	})

	t.Run("can disable rollback", func(t *testing.T) {
		executor.EnableRollback(false)
		testutil.AssertFalse(t, executor.enableRollback, "Rollback should be disabled")

		// Re-enable for next tests
		executor.EnableRollback(true)
	})
}

func TestSynthfsExecutor_Force(t *testing.T) {
	// Use a temp directory that mimics dodot structure
	tempDir := testutil.TempDir(t, "synthfs-force")
	t.Setenv("HOME", tempDir)
	t.Setenv("DODOT_DATA_DIR", filepath.Join(tempDir, ".local", "share", "dodot"))
	t.Setenv("DOTFILES_ROOT", filepath.Join(tempDir, "dotfiles"))

	dataDir := filepath.Join(tempDir, ".local", "share", "dodot")
	dotfilesDir := filepath.Join(tempDir, "dotfiles")

	testutil.CreateDir(t, tempDir, ".local")
	testutil.CreateDir(t, filepath.Join(tempDir, ".local"), "share")
	testutil.CreateDir(t, filepath.Join(tempDir, ".local", "share"), "dodot")
	testutil.CreateDir(t, dataDir, "deployed")
	testutil.CreateDir(t, tempDir, "dotfiles")

	// Create a source file in dotfiles
	sourceFile := testutil.CreateFile(t, dotfilesDir, "test.conf", "new config")

	// Create an existing file that will be overwritten
	targetFile := filepath.Join(dataDir, "deployed", "test.conf")
	testutil.CreateFile(t, filepath.Join(dataDir, "deployed"), "test.conf", "old config")

	// Create paths and executor with force mode
	p, err := paths.New(dotfilesDir)
	testutil.AssertNoError(t, err)
	// Enable home symlinks because in the test environment, everything is under temp home
	executor := NewSynthfsExecutorWithPaths(false, p).EnableHomeSymlinks(false).EnableForce(true)

	operations := []types.Operation{
		{
			Type:        types.OperationCreateSymlink,
			Source:      sourceFile,
			Target:      targetFile,
			Description: "Create symlink with force",
			Status:      types.StatusReady,
		},
	}

	_, err = executor.ExecuteOperations(operations)
	testutil.AssertNoError(t, err)

	// Verify symlink was created (replacing the existing file)
	testutil.AssertTrue(t, testutil.FileExists(t, targetFile),
		"Symlink should have been created")
}
