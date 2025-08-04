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

func TestSynthfsExecutor_ValidateSafePath(t *testing.T) {
	// Create a temp directory to use as home
	tempHome := testutil.TempDir(t, "synthfs-validate")
	t.Setenv("HOME", tempHome)
	t.Setenv("DODOT_DATA_DIR", filepath.Join(tempHome, ".local", "share", "dodot"))

	// Create the necessary directories
	testutil.CreateDir(t, tempHome, ".local")
	testutil.CreateDir(t, filepath.Join(tempHome, ".local"), "share")
	testutil.CreateDir(t, filepath.Join(tempHome, ".local", "share"), "dodot")
	testutil.CreateDir(t, filepath.Join(tempHome, ".local", "share", "dodot"), "deployed")

	// Create paths and executor
	p, err := paths.New("")
	testutil.AssertNoError(t, err)
	executor := NewSynthfsExecutorWithPaths(false, p)

	tests := []struct {
		name      string
		path      string
		expectErr bool
	}{
		{
			name:      "data directory is safe",
			path:      filepath.Join(tempHome, ".local", "share", "dodot", "test.txt"),
			expectErr: false,
		},
		{
			name:      "deployed directory is safe",
			path:      filepath.Join(tempHome, ".local", "share", "dodot", "deployed", "symlink", "test"),
			expectErr: false,
		},
		{
			name:      "user home directory is not safe",
			path:      filepath.Join(tempHome, ".vimrc"),
			expectErr: true,
		},
		{
			name:      "system directory is not safe",
			path:      "/etc/passwd",
			expectErr: true,
		},
		{
			name:      "temp directory is not safe",
			path:      "/tmp/test.txt",
			expectErr: true,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			err := executor.validateSafePath(tt.path)
			if tt.expectErr {
				testutil.AssertError(t, err)
			} else {
				testutil.AssertNoError(t, err)
			}
		})
	}
}

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
			err := executor.ExecuteOperations(tt.operations)
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
	err = executor.ExecuteOperations(operations)
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
	err := executor.ExecuteOperations([]types.Operation{})
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
	err = executor.ExecuteOperations(operations)
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

	err = executor.ExecuteOperations(operations)
	testutil.AssertNoError(t, err)

	// Verify symlink was created
	testutil.AssertTrue(t, testutil.FileExists(t, filepath.Join(dataDir, "deployed", "test.conf")),
		"Symlink should have been created")
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

	err = executor.ExecuteOperations(operations)
	testutil.AssertNoError(t, err)

	// Verify symlink was created (replacing the existing file)
	testutil.AssertTrue(t, testutil.FileExists(t, targetFile),
		"Symlink should have been created")
}
