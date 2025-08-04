package synthfs

import (
	"path/filepath"
	"testing"

	"github.com/arthur-debert/dodot/pkg/paths"
	"github.com/arthur-debert/dodot/pkg/testutil"
	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestCombinedExecutor_OperationOrdering(t *testing.T) {
	// Create a temp directory for testing
	tempDir := testutil.TempDir(t, "combined-executor-test")
	t.Setenv("HOME", tempDir)
	t.Setenv("DODOT_DATA_DIR", filepath.Join(tempDir, ".local", "share", "dodot"))
	t.Setenv("DOTFILES_ROOT", filepath.Join(tempDir, "dotfiles"))

	dataDir := filepath.Join(tempDir, ".local", "share", "dodot")

	// Set up the directory structure
	testutil.CreateDir(t, tempDir, ".local")
	testutil.CreateDir(t, filepath.Join(tempDir, ".local"), "share")
	testutil.CreateDir(t, filepath.Join(tempDir, ".local", "share"), "dodot")
	testutil.CreateDir(t, dataDir, "install")
	testutil.CreateDir(t, dataDir, "brewfile")
	testutil.CreateDir(t, tempDir, "dotfiles")

	t.Run("executes operations in correct order", func(t *testing.T) {
		// Create a test script that creates a marker file
		scriptPath := filepath.Join(dataDir, "test.sh")
		markerPath := filepath.Join(dataDir, "marker.txt")
		sentinelPath := filepath.Join(dataDir, "install", "test-sentinel")

		// Create operations in mixed order
		operations := []types.Operation{
			// Sentinel file (should execute last)
			{
				Type:        types.OperationWriteFile,
				Target:      sentinelPath,
				Content:     "installed",
				Description: "Create sentinel file for test",
				Status:      types.StatusReady,
			},
			// Execute command (should execute after filesystem ops but before sentinel)
			{
				Type:        types.OperationExecute,
				Command:     "/bin/sh",
				Args:        []string{scriptPath},
				Description: "Run test script",
				Status:      types.StatusReady,
			},
			// Regular file write (should execute first with other filesystem ops)
			{
				Type:        types.OperationWriteFile,
				Target:      scriptPath,
				Content:     "#!/bin/sh\necho 'executed' > " + markerPath,
				Mode:        modePtr(0755),
				Description: "Create test script",
				Status:      types.StatusReady,
			},
			// Directory creation (should execute first with other filesystem ops)
			{
				Type:        types.OperationCreateDir,
				Target:      filepath.Join(dataDir, "testdir"),
				Description: "Create test directory",
				Status:      types.StatusReady,
			},
		}

		// Execute operations
		p, err := paths.New(filepath.Join(tempDir, "dotfiles"))
		require.NoError(t, err)
		executor := NewCombinedExecutorWithPaths(false, p)
		err = executor.ExecuteOperations(operations)
		require.NoError(t, err)

		// Verify results
		// 1. Directory should exist
		assert.True(t, testutil.DirExists(t, filepath.Join(dataDir, "testdir")))

		// 2. Script should have been created and executed (marker file exists)
		assert.True(t, testutil.FileExists(t, markerPath))
		content := testutil.ReadFile(t, markerPath)
		assert.Contains(t, content, "executed")

		// 3. Sentinel file should exist (created after successful execution)
		assert.True(t, testutil.FileExists(t, sentinelPath))
	})

	t.Run("does not create sentinel on command failure", func(t *testing.T) {
		failScriptPath := filepath.Join(dataDir, "fail.sh")
		failSentinelPath := filepath.Join(dataDir, "install", "fail-sentinel")

		operations := []types.Operation{
			{
				Type:        types.OperationWriteFile,
				Target:      failScriptPath,
				Content:     "#!/bin/sh\nexit 1",
				Mode:        modePtr(0755),
				Description: "Create failing script",
				Status:      types.StatusReady,
			},
			{
				Type:        types.OperationExecute,
				Command:     "/bin/sh",
				Args:        []string{failScriptPath},
				Description: "Run failing script",
				Status:      types.StatusReady,
			},
			{
				Type:        types.OperationWriteFile,
				Target:      failSentinelPath,
				Content:     "should not be created",
				Description: "Create sentinel file",
				Status:      types.StatusReady,
			},
		}

		p, err := paths.New(filepath.Join(tempDir, "dotfiles"))
		require.NoError(t, err)
		executor := NewCombinedExecutorWithPaths(false, p)
		err = executor.ExecuteOperations(operations)

		// Should fail due to command execution error
		require.Error(t, err)
		assert.Contains(t, err.Error(), "failed to execute commands")

		// Sentinel file should NOT exist
		assert.False(t, testutil.FileExists(t, failSentinelPath))
	})

	t.Run("handles empty operation list", func(t *testing.T) {
		p, err := paths.New(filepath.Join(tempDir, "dotfiles"))
		require.NoError(t, err)
		executor := NewCombinedExecutorWithPaths(false, p)
		err = executor.ExecuteOperations([]types.Operation{})
		require.NoError(t, err)
	})

	t.Run("skips non-ready operations", func(t *testing.T) {
		operations := []types.Operation{
			{
				Type:        types.OperationWriteFile,
				Target:      filepath.Join(dataDir, "skipped.txt"),
				Content:     "should not be created",
				Description: "Skipped operation",
				Status:      types.StatusSkipped,
			},
		}

		p, err := paths.New(filepath.Join(tempDir, "dotfiles"))
		require.NoError(t, err)
		executor := NewCombinedExecutorWithPaths(false, p)
		err = executor.ExecuteOperations(operations)
		require.NoError(t, err)

		// File should not exist
		assert.False(t, testutil.FileExists(t, filepath.Join(dataDir, "skipped.txt")))
	})
}

func TestCombinedExecutor_SentinelDetection(t *testing.T) {
	tests := []struct {
		name     string
		op       types.Operation
		expected bool
	}{
		{
			name: "sentinel in description",
			op: types.Operation{
				Type:        types.OperationWriteFile,
				Description: "Create sentinel file",
			},
			expected: true,
		},
		{
			name: "sentinel in path",
			op: types.Operation{
				Type:   types.OperationWriteFile,
				Target: "/data/sentinels/test",
			},
			expected: true,
		},
		{
			name: "install directory path",
			op: types.Operation{
				Type:   types.OperationWriteFile,
				Target: "/data/install/package",
			},
			expected: true,
		},
		{
			name: "brewfile directory path",
			op: types.Operation{
				Type:   types.OperationWriteFile,
				Target: "/data/brewfile/Brewfile.lock",
			},
			expected: true,
		},
		{
			name: "regular file write",
			op: types.Operation{
				Type:        types.OperationWriteFile,
				Target:      "/data/config.json",
				Description: "Write config file",
			},
			expected: false,
		},
		{
			name: "non-write operation",
			op: types.Operation{
				Type:        types.OperationCreateDir,
				Target:      "/data/sentinels",
				Description: "Create sentinels directory",
			},
			expected: false,
		},
		{
			name: "case insensitive sentinel detection",
			op: types.Operation{
				Type:        types.OperationWriteFile,
				Description: "Create SENTINEL file",
			},
			expected: true,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result := isSentinelWrite(tt.op)
			assert.Equal(t, tt.expected, result)
		})
	}
}
