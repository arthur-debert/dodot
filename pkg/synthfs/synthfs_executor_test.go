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

func TestSynthfsExecutor_ConvertOperations(t *testing.T) {
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
		name      string
		operation types.Operation
		expectErr bool
	}{
		{
			name: "create directory",
			operation: types.Operation{
				Type:        types.OperationCreateDir,
				Target:      filepath.Join(dataDir, "test-dir"),
				Description: "Create test directory",
			},
			expectErr: false,
		},
		{
			name: "write file",
			operation: types.Operation{
				Type:        types.OperationWriteFile,
				Target:      filepath.Join(dataDir, "test.txt"),
				Content:     "Hello, World!",
				Mode:        modePtr(0644),
				Description: "Write test file",
			},
			expectErr: false,
		},
		{
			name: "operation outside safe directory",
			operation: types.Operation{
				Type:        types.OperationWriteFile,
				Target:      "/tmp/unsafe.txt",
				Content:     "Should fail",
				Description: "Unsafe write",
			},
			expectErr: true,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			synthOp, err := executor.convertToSynthfsOperation(tt.operation)
			if tt.expectErr {
				testutil.AssertError(t, err)
				testutil.AssertNil(t, synthOp)
			} else {
				testutil.AssertNoError(t, err)
				testutil.AssertNotNil(t, synthOp)
			}
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
		},
		{
			Type:        types.OperationWriteFile,
			Target:      filepath.Join(dataDir, "test.txt"),
			Content:     "Hello, World!",
			Description: "Write test file",
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
		},
		{
			Type:        types.OperationChecksum,
			Source:      filepath.Join(dataDir, "test.txt"),
			Description: "Calculate checksum",
		},
		{
			Type:        types.OperationCreateDir,
			Target:      filepath.Join(dataDir, "real-dir"),
			Description: "Create real directory",
		},
	}

	// Only the create directory operation should be converted
	synthOps := make([]interface{}, 0)
	for _, op := range operations {
		synthOp, err := executor.convertToSynthfsOperation(op)
		testutil.AssertNoError(t, err)
		if synthOp != nil {
			synthOps = append(synthOps, synthOp)
		}
	}

	// Should only have one operation (the create directory)
	testutil.AssertEqual(t, 1, len(synthOps))
}
