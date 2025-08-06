package synthfs

import (
	"path/filepath"
	"testing"
	"time"

	"github.com/arthur-debert/dodot/pkg/paths"
	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestSynthfsExecutor_ExecuteOperations_ReturnsResults(t *testing.T) {
	// Setup test paths
	tempDir := t.TempDir()
	t.Setenv("HOME", tempDir)
	t.Setenv("DOTFILES_ROOT", filepath.Join(tempDir, "dotfiles"))

	p, err := paths.New("")
	require.NoError(t, err)
	tests := []struct {
		name       string
		operations []types.Operation
		dryRun     bool
		validate   func(t *testing.T, results []types.OperationResult, err error)
	}{
		{
			name: "dry run returns results without execution",
			operations: []types.Operation{
				{
					Type:        types.OperationCreateDir,
					Target:      filepath.Join(p.DataDir(), "test"),
					Description: "Create test directory",
					Status:      types.StatusReady,
					Pack:        "test",
					PowerUp:     "mkdir",
				},
				{
					Type:        types.OperationWriteFile,
					Target:      filepath.Join(p.DataDir(), "test", "file.txt"),
					Content:     "test content",
					Description: "Write test file",
					Status:      types.StatusReady,
					Pack:        "test",
					PowerUp:     "write",
				},
			},
			dryRun: true,
			validate: func(t *testing.T, results []types.OperationResult, err error) {
				require.NoError(t, err)
				assert.Len(t, results, 2)

				// All operations should have ready status in dry run
				for i, result := range results {
					assert.Equal(t, types.StatusReady, result.Status)
					assert.Nil(t, result.Error)
					assert.NotNil(t, result.Operation)
					assert.Equal(t, "test", result.Operation.Pack)
					assert.False(t, result.StartTime.IsZero())
					assert.False(t, result.EndTime.IsZero())

					if i == 0 {
						assert.Equal(t, "mkdir", result.Operation.PowerUp)
					} else {
						assert.Equal(t, "write", result.Operation.PowerUp)
					}
				}
			},
		},
		{
			name: "skipped operations are tracked",
			operations: []types.Operation{
				{
					Type:        types.OperationCreateDir,
					Target:      filepath.Join(p.DataDir(), "test2"),
					Description: "Create test directory",
					Status:      types.StatusReady,
				},
				{
					Type:        types.OperationWriteFile,
					Target:      filepath.Join(p.DataDir(), "test2", "file.txt"),
					Content:     "test content",
					Description: "Write test file",
					Status:      types.StatusSkipped,
				},
			},
			dryRun: false,
			validate: func(t *testing.T, results []types.OperationResult, err error) {
				require.NoError(t, err)
				require.Len(t, results, 2)

				// Find the operations by their status
				var readyOp, skippedOp *types.OperationResult
				for i := range results {
					switch results[i].Status {
					case types.StatusReady:
						readyOp = &results[i]
					case types.StatusSkipped:
						skippedOp = &results[i]
					}
				}

				// We should have one of each
				assert.NotNil(t, readyOp, "Should have one ready operation")
				assert.NotNil(t, skippedOp, "Should have one skipped operation")
				assert.Nil(t, readyOp.Error)
				assert.Nil(t, skippedOp.Error)
			},
		},
		{
			name: "operations preserve metadata",
			operations: []types.Operation{
				{
					Type:        types.OperationCreateSymlink,
					Source:      filepath.Join(p.DotfilesRoot(), "vim", ".vimrc"),
					Target:      filepath.Join(p.DataDir(), "symlinks", ".vimrc"),
					Description: "Create symlink",
					Status:      types.StatusReady,
					Pack:        "vim",
					PowerUp:     "symlink",
					TriggerInfo: &types.TriggerMatchInfo{
						TriggerName:  "FileName",
						OriginalPath: ".vimrc",
						Priority:     10,
					},
					Metadata: map[string]interface{}{
						"app": "vim",
					},
					GroupID: "vim-config",
				},
			},
			dryRun: true,
			validate: func(t *testing.T, results []types.OperationResult, err error) {
				require.NoError(t, err)
				require.Len(t, results, 1)

				result := results[0]
				assert.Equal(t, types.StatusReady, result.Status)
				assert.Equal(t, "vim", result.Operation.Pack)
				assert.Equal(t, "symlink", result.Operation.PowerUp)
				assert.NotNil(t, result.Operation.TriggerInfo)
				assert.Equal(t, "FileName", result.Operation.TriggerInfo.TriggerName)
				assert.Equal(t, "vim-config", result.Operation.GroupID)
			},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			executor := NewSynthfsExecutorWithPaths(tt.dryRun, p)
			results, err := executor.ExecuteOperations(tt.operations)
			tt.validate(t, results, err)
		})
	}
}

func TestSynthfsExecutor_ExecuteOperations_ShellCommands(t *testing.T) {
	// Setup test paths
	tempDir := t.TempDir()
	t.Setenv("HOME", tempDir)
	t.Setenv("DOTFILES_ROOT", filepath.Join(tempDir, "dotfiles"))

	p, err := paths.New("")
	require.NoError(t, err)

	tests := []struct {
		name       string
		operations []types.Operation
		dryRun     bool
		validate   func(t *testing.T, results []types.OperationResult, err error)
	}{
		{
			name: "execute operations return results with output",
			operations: []types.Operation{
				{
					Type:        types.OperationExecute,
					Command:     "echo",
					Args:        []string{"hello world"},
					Description: "Echo test",
					Status:      types.StatusReady,
					Pack:        "test",
					PowerUp:     "execute",
				},
			},
			dryRun: false,
			validate: func(t *testing.T, results []types.OperationResult, err error) {
				require.NoError(t, err)
				require.Len(t, results, 1)

				result := results[0]
				assert.Equal(t, types.StatusReady, result.Status)
				assert.Nil(t, result.Error)
				assert.Contains(t, result.Output, "hello world")
				assert.Equal(t, "test", result.Operation.Pack)
				assert.Equal(t, "execute", result.Operation.PowerUp)
			},
		},
		{
			name: "mixed operations execute in order",
			operations: []types.Operation{
				{
					Type:        types.OperationCreateDir,
					Target:      filepath.Join(p.DataDir(), "shell-test"),
					Description: "Create directory",
					Status:      types.StatusReady,
				},
				{
					Type:        types.OperationExecute,
					Command:     "echo",
					Args:        []string{"test"},
					Description: "Echo test",
					Status:      types.StatusReady,
				},
			},
			dryRun: false,
			validate: func(t *testing.T, results []types.OperationResult, err error) {
				require.NoError(t, err)
				// Both operations should be in results
				assert.Len(t, results, 2)
				assert.Equal(t, types.OperationCreateDir, results[0].Operation.Type)
				assert.Equal(t, types.OperationExecute, results[1].Operation.Type)
			},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			executor := NewSynthfsExecutorWithPaths(tt.dryRun, p)
			results, err := executor.ExecuteOperations(tt.operations)
			tt.validate(t, results, err)
		})
	}
}

func TestSynthfsExecutor_MixedOperations_ReturnsResults(t *testing.T) {
	// Setup test paths
	tempDir := t.TempDir()
	t.Setenv("HOME", tempDir)
	t.Setenv("DOTFILES_ROOT", filepath.Join(tempDir, "dotfiles"))

	p, err := paths.New("")
	require.NoError(t, err)

	operations := []types.Operation{
		{
			Type:        types.OperationCreateDir,
			Target:      filepath.Join(p.DataDir(), "test"),
			Description: "Create directory",
			Status:      types.StatusReady,
			Pack:        "test",
			PowerUp:     "mkdir",
		},
		{
			Type:        types.OperationWriteFile,
			Target:      filepath.Join(p.DataDir(), "test", "file.txt"),
			Content:     "test",
			Description: "Write file",
			Status:      types.StatusReady,
			Pack:        "test",
			PowerUp:     "write",
		},
		{
			Type:        types.OperationExecute,
			Command:     "echo",
			Args:        []string{"done"},
			Description: "Echo completion",
			Status:      types.StatusReady,
			Pack:        "test",
			PowerUp:     "execute",
		},
	}

	executor := NewSynthfsExecutorWithPaths(true, p) // dry run
	results, err := executor.ExecuteOperations(operations)

	require.NoError(t, err)
	assert.Len(t, results, 3)

	// Verify all operations have results
	for _, result := range results {
		assert.NotNil(t, result.Operation)
		assert.Equal(t, types.StatusReady, result.Status)
		assert.Nil(t, result.Error)
		assert.False(t, result.StartTime.IsZero())
		assert.False(t, result.EndTime.IsZero())
		assert.Equal(t, "test", result.Operation.Pack)
	}

	// Verify timing makes sense
	for i := 1; i < len(results); i++ {
		// Later operations should start after or at the same time as earlier ones
		assert.True(t, results[i].StartTime.After(results[i-1].StartTime) ||
			results[i].StartTime.Equal(results[i-1].StartTime))
	}
}

func TestOperationResult_Timing(t *testing.T) {
	start := time.Now()

	// Simulate an operation that takes some time
	time.Sleep(10 * time.Millisecond)

	end := time.Now()

	result := types.OperationResult{
		Operation: &types.Operation{
			Type:        types.OperationCreateDir,
			Target:      "/tmp/test",
			Description: "Test operation",
		},
		Status:    types.StatusReady,
		StartTime: start,
		EndTime:   end,
	}

	// Verify timing
	assert.True(t, result.EndTime.After(result.StartTime))
	duration := result.EndTime.Sub(result.StartTime)
	assert.True(t, duration >= 10*time.Millisecond)
}
