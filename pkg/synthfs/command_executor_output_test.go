package synthfs_test

import (
	"bytes"
	"io"
	"os"
	"path/filepath"
	"testing"

	"github.com/arthur-debert/dodot/pkg/synthfs"
	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestCommandExecutorDisplaysOutput(t *testing.T) {
	t.Run("displays stdout to console", func(t *testing.T) {
		// Create a simple script that outputs to stdout
		tempDir := t.TempDir()
		scriptPath := filepath.Join(tempDir, "test.sh")
		scriptContent := `#!/bin/sh
echo "Hello from script"
echo "Line 2 of output"
`
		require.NoError(t, os.WriteFile(scriptPath, []byte(scriptContent), 0755))

		// Capture stdout
		oldStdout := os.Stdout
		r, w, _ := os.Pipe()
		os.Stdout = w

		// Create executor and run script
		executor := synthfs.NewCommandExecutor(false)
		op := types.Operation{
			Type:        types.OperationExecute,
			Command:     "/bin/sh",
			Args:        []string{scriptPath},
			Description: "Test script",
			Status:      types.StatusReady,
		}

		_, err := executor.ExecuteOperations([]types.Operation{op})
		require.NoError(t, err)

		// Restore stdout and read captured output
		require.NoError(t, w.Close())
		os.Stdout = oldStdout
		var buf bytes.Buffer
		_, err = io.Copy(&buf, r)
		require.NoError(t, err)

		// Verify output was displayed
		output := buf.String()
		assert.Contains(t, output, "Hello from script")
		assert.Contains(t, output, "Line 2 of output")
	})

	t.Run("displays stderr to console", func(t *testing.T) {
		// Create a script that outputs to stderr
		tempDir := t.TempDir()
		scriptPath := filepath.Join(tempDir, "error.sh")
		scriptContent := `#!/bin/sh
echo "Error message" >&2
echo "Another error" >&2
exit 0
`
		require.NoError(t, os.WriteFile(scriptPath, []byte(scriptContent), 0755))

		// Capture stderr
		oldStderr := os.Stderr
		r, w, _ := os.Pipe()
		os.Stderr = w

		// Create executor and run script
		executor := synthfs.NewCommandExecutor(false)
		op := types.Operation{
			Type:        types.OperationExecute,
			Command:     "/bin/sh",
			Args:        []string{scriptPath},
			Description: "Test error script",
			Status:      types.StatusReady,
		}

		_, err := executor.ExecuteOperations([]types.Operation{op})
		require.NoError(t, err)

		// Restore stderr and read captured output
		require.NoError(t, w.Close())
		os.Stderr = oldStderr
		var buf bytes.Buffer
		_, err = io.Copy(&buf, r)
		require.NoError(t, err)

		// Verify error output was displayed
		output := buf.String()
		assert.Contains(t, output, "Error message")
		assert.Contains(t, output, "Another error")
	})

	t.Run("no output in dry run mode", func(t *testing.T) {
		// Create a simple script
		tempDir := t.TempDir()
		scriptPath := filepath.Join(tempDir, "test.sh")
		scriptContent := `#!/bin/sh
echo "Should not see this in dry run"
`
		require.NoError(t, os.WriteFile(scriptPath, []byte(scriptContent), 0755))

		// Capture stdout
		oldStdout := os.Stdout
		r, w, _ := os.Pipe()
		os.Stdout = w

		// Create executor in dry run mode
		executor := synthfs.NewCommandExecutor(true)
		op := types.Operation{
			Type:        types.OperationExecute,
			Command:     "/bin/sh",
			Args:        []string{scriptPath},
			Description: "Test script",
			Status:      types.StatusReady,
		}

		_, err := executor.ExecuteOperations([]types.Operation{op})
		require.NoError(t, err)

		// Restore stdout and read captured output
		require.NoError(t, w.Close())
		os.Stdout = oldStdout
		var buf bytes.Buffer
		_, err = io.Copy(&buf, r)
		require.NoError(t, err)

		// Verify no script output in dry run
		output := buf.String()
		assert.NotContains(t, output, "Should not see this")
	})
}
