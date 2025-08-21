package shell

import (
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"testing"

	"github.com/arthur-debert/dodot/pkg/testutil"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestShellInitScript_HandlesMissingFiles(t *testing.T) {
	tests := []struct {
		name           string
		setupFiles     func(t *testing.T, dataDir, dotfilesRoot string)
		expectedOutput string
		shouldNotHave  []string
	}{
		{
			name: "handles missing shell profile source files",
			setupFiles: func(t *testing.T, dataDir, dotfilesRoot string) {
				// Create shell_profile directory with symlink to non-existent file
				deployedDir := filepath.Join(dataDir, "deployed", "shell_profile")
				testutil.CreateDir(t, dataDir, "deployed/shell_profile")

				// Create symlink to non-existent file
				nonExistentSource := filepath.Join(dotfilesRoot, "vim", "aliases.sh")
				symlinkPath := filepath.Join(deployedDir, "aliases.sh")
				require.NoError(t, os.Symlink(nonExistentSource, symlinkPath))
			},
			expectedOutput: "",
			shouldNotHave: []string{
				"No such file or directory",
				"cannot open",
				"aliases.sh",
			},
		},
		{
			name: "handles missing PATH directories",
			setupFiles: func(t *testing.T, dataDir, dotfilesRoot string) {
				// Create path directory with symlink to non-existent directory
				deployedDir := filepath.Join(dataDir, "deployed", "path")
				testutil.CreateDir(t, dataDir, "deployed/path")

				// Create symlink to non-existent directory
				nonExistentDir := filepath.Join(dotfilesRoot, "tools", "bin")
				symlinkPath := filepath.Join(deployedDir, "tools-bin")
				require.NoError(t, os.Symlink(nonExistentDir, symlinkPath))
			},
			expectedOutput: "",
			shouldNotHave: []string{
				"No such file or directory",
				"cannot open",
				"tools-bin",
			},
		},
		{
			name: "handles missing shell source files",
			setupFiles: func(t *testing.T, dataDir, dotfilesRoot string) {
				// Create shell_source directory with symlink to non-existent file
				deployedDir := filepath.Join(dataDir, "deployed", "shell_source")
				testutil.CreateDir(t, dataDir, "deployed/shell_source")

				// Create symlink to non-existent file
				nonExistentSource := filepath.Join(dotfilesRoot, "zsh", "functions.sh")
				symlinkPath := filepath.Join(deployedDir, "functions.sh")
				require.NoError(t, os.Symlink(nonExistentSource, symlinkPath))
			},
			expectedOutput: "",
			shouldNotHave: []string{
				"No such file or directory",
				"cannot open",
				"functions.sh",
			},
		},
		{
			name: "sources existing files successfully",
			setupFiles: func(t *testing.T, dataDir, dotfilesRoot string) {
				// Create shell_profile directory with valid symlink
				deployedDir := filepath.Join(dataDir, "deployed", "shell_profile")
				testutil.CreateDir(t, dataDir, "deployed/shell_profile")

				// Create actual source file
				testutil.CreateFile(t, dotfilesRoot, "vim/aliases.sh", "echo 'Aliases loaded'")

				// Create symlink to existing file
				sourcePath := filepath.Join(dotfilesRoot, "vim", "aliases.sh")
				symlinkPath := filepath.Join(deployedDir, "aliases.sh")
				require.NoError(t, os.Symlink(sourcePath, symlinkPath))
			},
			expectedOutput: "Aliases loaded",
			shouldNotHave: []string{
				"No such file or directory",
				"cannot open",
			},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			// Setup test environment
			tempDir := testutil.TempDir(t, "shell-init-test")
			dataDir := filepath.Join(tempDir, "data")
			dotfilesRoot := filepath.Join(tempDir, "dotfiles")

			testutil.CreateDir(t, tempDir, "data")
			testutil.CreateDir(t, tempDir, "dotfiles")

			// Setup test files
			tt.setupFiles(t, dataDir, dotfilesRoot)

			// Read the shell init script
			scriptPath := filepath.Join("..", "..", "pkg", "shell", "dodot-init.sh")
			scriptContent, err := os.ReadFile(scriptPath)
			require.NoError(t, err)

			// Create a test script that sources dodot-init.sh
			testScript := fmt.Sprintf(`#!/bin/bash
set -e  # Exit on error to catch any issues
export DODOT_DATA_DIR="%s"
export DODOT_DEPLOYMENT_ROOT="%s"

# Source the init script
%s

# If we get here, no errors occurred
echo "Script completed successfully"
`, dataDir, dotfilesRoot, string(scriptContent))

			testScriptPath := filepath.Join(tempDir, "test-init.sh")
			require.NoError(t, os.WriteFile(testScriptPath, []byte(testScript), 0755))

			// Execute the test script
			cmd := exec.Command("bash", testScriptPath)
			output, err := cmd.CombinedOutput()

			// Should not error even with missing files
			require.NoError(t, err, "Script failed with output: %s", string(output))

			outputStr := string(output)

			// Check expected output
			if tt.expectedOutput != "" {
				assert.Contains(t, outputStr, tt.expectedOutput)
			}

			// Check that error messages are not present
			for _, shouldNotHave := range tt.shouldNotHave {
				assert.NotContains(t, outputStr, shouldNotHave,
					"Output should not contain error message: %s", shouldNotHave)
			}

			// Always check for successful completion
			assert.Contains(t, outputStr, "Script completed successfully")
		})
	}
}

func TestShellInitScript_ConditionalSourcing(t *testing.T) {
	// Setup test environment
	tempDir := testutil.TempDir(t, "shell-conditional-test")
	dataDir := filepath.Join(tempDir, "data")
	dotfilesRoot := filepath.Join(tempDir, "dotfiles")

	testutil.CreateDir(t, tempDir, "data")
	testutil.CreateDir(t, tempDir, "dotfiles")
	testutil.CreateDir(t, dataDir, "deployed/shell_profile")
	testutil.CreateDir(t, dataDir, "deployed/path")

	// Create a mix of existing and non-existing files
	// Existing file
	testutil.CreateFile(t, dotfilesRoot, "bash/aliases.sh",
		"export TEST_ALIAS_LOADED=1")
	existingSource := filepath.Join(dotfilesRoot, "bash", "aliases.sh")
	existingSymlink := filepath.Join(dataDir, "deployed", "shell_profile", "aliases.sh")
	require.NoError(t, os.Symlink(existingSource, existingSymlink))

	// Non-existing file
	nonExistingSource := filepath.Join(dotfilesRoot, "zsh", "functions.sh")
	nonExistingSymlink := filepath.Join(dataDir, "deployed", "shell_profile", "functions.sh")
	require.NoError(t, os.Symlink(nonExistingSource, nonExistingSymlink))

	// Existing directory for PATH
	testutil.CreateDir(t, dotfilesRoot, "tools/bin")
	existingDir := filepath.Join(dotfilesRoot, "tools", "bin")
	existingDirSymlink := filepath.Join(dataDir, "deployed", "path", "tools-bin")
	require.NoError(t, os.Symlink(existingDir, existingDirSymlink))

	// Non-existing directory for PATH
	nonExistingDir := filepath.Join(dotfilesRoot, "missing", "bin")
	nonExistingDirSymlink := filepath.Join(dataDir, "deployed", "path", "missing-bin")
	require.NoError(t, os.Symlink(nonExistingDir, nonExistingDirSymlink))

	// Read the shell init script
	scriptPath := filepath.Join("..", "..", "pkg", "shell", "dodot-init.sh")
	scriptContent, err := os.ReadFile(scriptPath)
	require.NoError(t, err)

	// Create a test script
	testScript := fmt.Sprintf(`#!/bin/bash
export DODOT_DATA_DIR="%s"
export DODOT_DEPLOYMENT_ROOT="%s"
export PATH_BEFORE="$PATH"

# Source the init script
%s

# Check what was loaded
if [ -n "$TEST_ALIAS_LOADED" ]; then
    echo "SUCCESS: Existing alias file was loaded"
fi

# Check PATH modifications
if [[ "$PATH" == *"tools-bin"* ]]; then
    echo "SUCCESS: Existing directory added to PATH"
fi

if [[ "$PATH" == *"missing"* ]]; then
    echo "ERROR: Non-existing directory added to PATH"
fi

# Count PATH entries
IFS=':' read -ra PATH_ARRAY <<< "$PATH"
IFS=':' read -ra PATH_BEFORE_ARRAY <<< "$PATH_BEFORE"
PATH_DIFF=$((${#PATH_ARRAY[@]} - ${#PATH_BEFORE_ARRAY[@]}))
echo "PATH entries added: $PATH_DIFF"
`, dataDir, dotfilesRoot, string(scriptContent))

	testScriptPath := filepath.Join(tempDir, "test-conditional.sh")
	require.NoError(t, os.WriteFile(testScriptPath, []byte(testScript), 0755))

	// Execute the test script
	cmd := exec.Command("bash", testScriptPath)
	output, err := cmd.CombinedOutput()
	require.NoError(t, err, "Script failed with output: %s", string(output))

	outputStr := string(output)

	// Verify only existing files were sourced
	assert.Contains(t, outputStr, "SUCCESS: Existing alias file was loaded")
	assert.Contains(t, outputStr, "SUCCESS: Existing directory added to PATH")
	assert.NotContains(t, outputStr, "ERROR: Non-existing directory added to PATH")

	// Verify only one PATH entry was added (the existing one)
	assert.Contains(t, outputStr, "PATH entries added: 1")
}

func TestShellInitScript_SuppressesErrors(t *testing.T) {
	// Setup test environment
	tempDir := testutil.TempDir(t, "shell-error-suppression-test")
	dataDir := filepath.Join(tempDir, "data")
	dotfilesRoot := filepath.Join(tempDir, "dotfiles")

	testutil.CreateDir(t, tempDir, "data")
	testutil.CreateDir(t, tempDir, "dotfiles")
	testutil.CreateDir(t, dataDir, "deployed/shell_profile")

	// Create a symlink to a non-existent file
	nonExistentSource := filepath.Join(dotfilesRoot, "bash", "missing.sh")
	brokenSymlink := filepath.Join(dataDir, "deployed", "shell_profile", "missing.sh")
	require.NoError(t, os.Symlink(nonExistentSource, brokenSymlink))

	// Read the shell init script
	scriptPath := filepath.Join("..", "..", "pkg", "shell", "dodot-init.sh")
	scriptContent, err := os.ReadFile(scriptPath)
	require.NoError(t, err)

	// Create a test script that captures stderr
	testScript := fmt.Sprintf(`#!/bin/bash
export DODOT_DATA_DIR="%s"
export DODOT_DEPLOYMENT_ROOT="%s"

# Redirect stderr to a file to capture any errors
STDERR_FILE="%s/stderr.txt"

# Source the init script and capture stderr
{
%s
} 2>"$STDERR_FILE"

# Output the captured stderr
echo "=== STDERR ==="
cat "$STDERR_FILE"
echo "=== END STDERR ==="

# Check if stderr is empty
if [ -s "$STDERR_FILE" ]; then
    echo "ERROR: stderr is not empty"
    exit 1
else
    echo "SUCCESS: No error output"
fi
`, dataDir, dotfilesRoot, tempDir, string(scriptContent))

	testScriptPath := filepath.Join(tempDir, "test-errors.sh")
	require.NoError(t, os.WriteFile(testScriptPath, []byte(testScript), 0755))

	// Execute the test script
	cmd := exec.Command("bash", testScriptPath)
	output, err := cmd.CombinedOutput()
	require.NoError(t, err, "Script failed with output: %s", string(output))

	outputStr := string(output)

	// Verify no error output
	assert.Contains(t, outputStr, "SUCCESS: No error output")
	assert.NotContains(t, outputStr, "No such file or directory")
	assert.NotContains(t, outputStr, "cannot open")
}
