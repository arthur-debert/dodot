package shell

import (
	"fmt"
	"os"
	"os/exec"
	"path/filepath"
	"strings"
	"testing"

	"github.com/arthur-debert/dodot/pkg/testutil"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestShellInitScriptEnvironmentVariables(t *testing.T) {
	// Skip on Windows as shell scripts don't work there
	testutil.SkipOnWindows(t)

	tests := []struct {
		name         string
		setupFixture func(t *testing.T, dataDir, dotfilesRoot string)
		expectedVars map[string][]string // Environment variable name -> expected values
	}{
		{
			name: "symlinks_tracked",
			setupFixture: func(t *testing.T, dataDir, dotfilesRoot string) {
				// Create deployed symlinks
				deployedSymlink := filepath.Join(dataDir, "deployed", "symlink")
				testutil.CreateDir(t, dataDir, "deployed/symlink")

				// Create fake dotfiles to link to
				vimrcPath := filepath.Join(dotfilesRoot, "vim", ".vimrc")
				testutil.CreateDir(t, dotfilesRoot, "vim")
				testutil.CreateFile(t, filepath.Dir(vimrcPath), ".vimrc", "\" vim config")

				bashrcPath := filepath.Join(dotfilesRoot, "shell", ".bashrc")
				testutil.CreateDir(t, dotfilesRoot, "shell")
				testutil.CreateFile(t, filepath.Dir(bashrcPath), ".bashrc", "# bashrc")

				// Create symlinks
				testutil.CreateSymlink(t, vimrcPath, filepath.Join(deployedSymlink, ".vimrc"))
				testutil.CreateSymlink(t, bashrcPath, filepath.Join(deployedSymlink, ".bashrc"))
			},
			expectedVars: map[string][]string{
				"DODOT_SYMLINKS": {"vim/.vimrc", "shell/.bashrc"},
			},
		},
		{
			name: "shells_tracked",
			setupFixture: func(t *testing.T, dataDir, dotfilesRoot string) {
				// Create deployed shell profiles
				deployedProfile := filepath.Join(dataDir, "deployed", "shell")
				testutil.CreateDir(t, dataDir, "deployed/shell")

				// Create fake shell scripts
				aliasesPath := filepath.Join(dotfilesRoot, "base", "aliases.sh")
				testutil.CreateDir(t, dotfilesRoot, "base")
				testutil.CreateFile(t, filepath.Dir(aliasesPath), "aliases.sh", "alias ll='ls -l'")

				gitPath := filepath.Join(dotfilesRoot, "git", "git-profile.sh")
				testutil.CreateDir(t, dotfilesRoot, "git")
				testutil.CreateFile(t, filepath.Dir(gitPath), "git-profile.sh", "alias gs='git status'")

				// Create symlinks
				testutil.CreateSymlink(t, aliasesPath, filepath.Join(deployedProfile, "aliases.sh"))
				testutil.CreateSymlink(t, gitPath, filepath.Join(deployedProfile, "git-profile.sh"))
			},
			expectedVars: map[string][]string{
				"DODOT_SHELL_PROFILES": {"base/aliases.sh", "git/git-profile.sh"},
			},
		},
		{
			name: "path_dirs_tracked",
			setupFixture: func(t *testing.T, dataDir, dotfilesRoot string) {
				// Create deployed path directories
				deployedPath := filepath.Join(dataDir, "deployed", "path")
				testutil.CreateDir(t, dataDir, "deployed/path")

				// Create fake bin directories
				toolsBin := filepath.Join(dotfilesRoot, "tools", "bin")
				testutil.CreateDir(t, dotfilesRoot, "tools/bin")
				testutil.CreateFile(t, toolsBin, "mytool", "#!/bin/bash\necho tool")

				scriptsBin := filepath.Join(dotfilesRoot, "scripts", "bin")
				testutil.CreateDir(t, dotfilesRoot, "scripts/bin")
				testutil.CreateFile(t, scriptsBin, "myscript", "#!/bin/bash\necho script")

				// Create symlinks with pack-prefixed names
				testutil.CreateSymlink(t, toolsBin, filepath.Join(deployedPath, "tools-bin"))
				testutil.CreateSymlink(t, scriptsBin, filepath.Join(deployedPath, "scripts-bin"))
			},
			expectedVars: map[string][]string{
				"DODOT_PATH_DIRS": {"tools/bin", "scripts/bin"},
			},
		},
		{
			name: "shell_sources_tracked",
			setupFixture: func(t *testing.T, dataDir, dotfilesRoot string) {
				// Create deployed shell sources
				deployedSource := filepath.Join(dataDir, "deployed", "shell_source")
				testutil.CreateDir(t, dataDir, "deployed/shell_source")

				// Create fake shell sources
				envPath := filepath.Join(dotfilesRoot, "dev", "env.sh")
				testutil.CreateDir(t, dotfilesRoot, "dev")
				testutil.CreateFile(t, filepath.Dir(envPath), "env.sh", "export DEV=1")

				// Create symlinks
				testutil.CreateSymlink(t, envPath, filepath.Join(deployedSource, "env.sh"))
			},
			expectedVars: map[string][]string{
				"DODOT_SHELL_SOURCES": {"dev/env.sh"},
			},
		},
		{
			name: "run_once_handlers_tracked",
			setupFixture: func(t *testing.T, dataDir, dotfilesRoot string) {
				// Create provision script sentinels
				provisionSentinels := filepath.Join(dataDir, "provision", "sentinels")
				testutil.CreateDir(t, dataDir, "provision/sentinels")
				testutil.CreateFile(t, provisionSentinels, "vim", "abc123checksum")
				testutil.CreateFile(t, provisionSentinels, "node", "def456checksum")

				// Create homebrew sentinels
				homebrewDir := filepath.Join(dataDir, "homebrew")
				testutil.CreateDir(t, dataDir, "homebrew")
				testutil.CreateFile(t, homebrewDir, "base", "ghi789checksum")
				testutil.CreateFile(t, homebrewDir, "dev", "jkl012checksum")
			},
			expectedVars: map[string][]string{
				"DODOT_PROVISION_SCRIPTS": {"vim", "node"},
				"DODOT_BREWFILES":         {"base", "dev"},
			},
		},
		{
			name: "all_handlers_combined",
			setupFixture: func(t *testing.T, dataDir, dotfilesRoot string) {
				// Create all types of deployments
				testutil.CreateDir(t, dataDir, "deployed/symlink")
				testutil.CreateDir(t, dataDir, "deployed/shell")
				testutil.CreateDir(t, dataDir, "deployed/path")
				testutil.CreateDir(t, dataDir, "deployed/shell_source")
				testutil.CreateDir(t, dataDir, "provision/sentinels")
				testutil.CreateDir(t, dataDir, "homebrew")

				// Symlinks
				vimrcPath := filepath.Join(dotfilesRoot, "vim", ".vimrc")
				testutil.CreateDir(t, dotfilesRoot, "vim")
				testutil.CreateFile(t, filepath.Dir(vimrcPath), ".vimrc", "\" vim")
				testutil.CreateSymlink(t, vimrcPath, filepath.Join(dataDir, "deployed/symlink/.vimrc"))

				// Shell profiles
				aliasesPath := filepath.Join(dotfilesRoot, "base", "aliases.sh")
				testutil.CreateDir(t, dotfilesRoot, "base")
				testutil.CreateFile(t, filepath.Dir(aliasesPath), "aliases.sh", "alias ll='ls -l'")
				testutil.CreateSymlink(t, aliasesPath, filepath.Join(dataDir, "deployed/shell/aliases.sh"))

				// Path dirs
				toolsBin := filepath.Join(dotfilesRoot, "tools", "bin")
				testutil.CreateDir(t, dotfilesRoot, "tools/bin")
				testutil.CreateSymlink(t, toolsBin, filepath.Join(dataDir, "deployed/path/tools-bin"))

				// Shell sources
				envPath := filepath.Join(dotfilesRoot, "dev", "env.sh")
				testutil.CreateDir(t, dotfilesRoot, "dev")
				testutil.CreateFile(t, filepath.Dir(envPath), "env.sh", "export DEV=1")
				testutil.CreateSymlink(t, envPath, filepath.Join(dataDir, "deployed/shell_source/env.sh"))

				// Run-once sentinels
				testutil.CreateFile(t, filepath.Join(dataDir, "provision/sentinels"), "vim", "checksum1")
				testutil.CreateFile(t, filepath.Join(dataDir, "homebrew"), "base", "checksum2")
			},
			expectedVars: map[string][]string{
				"DODOT_SYMLINKS":          {"vim/.vimrc"},
				"DODOT_SHELL_PROFILES":    {"base/aliases.sh"},
				"DODOT_PATH_DIRS":         {"tools/bin"},
				"DODOT_SHELL_SOURCES":     {"dev/env.sh"},
				"DODOT_PROVISION_SCRIPTS": {"vim"},
				"DODOT_BREWFILES":         {"base"},
			},
		},
		{
			name: "empty_deployment",
			setupFixture: func(t *testing.T, dataDir, dotfilesRoot string) {
				// Create empty deployment directories
				testutil.CreateDir(t, dataDir, "deployed")
			},
			expectedVars: map[string][]string{
				// All variables should be empty but exported
				"DODOT_SYMLINKS":          {},
				"DODOT_SHELL_PROFILES":    {},
				"DODOT_PATH_DIRS":         {},
				"DODOT_SHELL_SOURCES":     {},
				"DODOT_PROVISION_SCRIPTS": {},
				"DODOT_BREWFILES":         {},
			},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			// Create temporary directories
			tempDir := testutil.TempDir(t, "shell-init-test")
			dataDir := filepath.Join(tempDir, "data")
			dotfilesRoot := filepath.Join(tempDir, "dotfiles")

			// Create base directories
			require.NoError(t, os.MkdirAll(dataDir, 0755))
			require.NoError(t, os.MkdirAll(dotfilesRoot, 0755))

			// Create deployment metadata
			metadata := fmt.Sprintf("export DODOT_DEPLOYMENT_ROOT=\"%s\"\n", dotfilesRoot)
			testutil.CreateFile(t, dataDir, "deployment-metadata", metadata)

			// Setup test fixtures
			tt.setupFixture(t, dataDir, dotfilesRoot)

			// Get the path to dodot-init.sh
			shellInitPath, err := filepath.Abs("dodot-init.sh")
			require.NoError(t, err)
			require.True(t, testutil.FileExists(t, shellInitPath), "dodot-init.sh should exist")

			// Create a test script that sources dodot-init.sh and prints environment variables
			testScript := fmt.Sprintf(`#!/bin/bash
set -e

# Set DODOT_DATA_DIR to our test directory
export DODOT_DATA_DIR="%s"

# Source the init script
source "%s"

# Print the environment variables we care about
echo "DODOT_SYMLINKS=$DODOT_SYMLINKS"
echo "DODOT_SHELL_PROFILES=$DODOT_SHELL_PROFILES"
echo "DODOT_PATH_DIRS=$DODOT_PATH_DIRS"
echo "DODOT_SHELL_SOURCES=$DODOT_SHELL_SOURCES"
echo "DODOT_PROVISION_SCRIPTS=$DODOT_PROVISION_SCRIPTS"
echo "DODOT_BREWFILES=$DODOT_BREWFILES"
echo "DODOT_DATA_DIR=$DODOT_DATA_DIR"
echo "DODOT_DEPLOYMENT_ROOT=$DODOT_DEPLOYMENT_ROOT"
`, dataDir, shellInitPath)

			testScriptPath := filepath.Join(tempDir, "test.sh")
			testutil.CreateFile(t, tempDir, "test.sh", testScript)
			testutil.Chmod(t, testScriptPath, 0755)

			// Run the test script
			cmd := exec.Command("bash", testScriptPath)
			output, err := cmd.CombinedOutput()
			require.NoError(t, err, "Script execution failed: %s", string(output))

			// Parse the output
			envVars := make(map[string]string)
			for _, line := range strings.Split(string(output), "\n") {
				if strings.Contains(line, "=") {
					parts := strings.SplitN(line, "=", 2)
					if len(parts) == 2 {
						envVars[parts[0]] = parts[1]
					}
				}
			}

			// Verify the environment variables
			assert.Equal(t, dataDir, envVars["DODOT_DATA_DIR"])
			assert.Equal(t, dotfilesRoot, envVars["DODOT_DEPLOYMENT_ROOT"])

			// Check each expected variable
			for varName, expectedValues := range tt.expectedVars {
				actualValue := envVars[varName]

				if len(expectedValues) == 0 {
					// Should be empty
					assert.Empty(t, actualValue, "Variable %s should be empty", varName)
				} else {
					// Split by colon and check values
					actualValues := strings.Split(actualValue, ":")
					assert.ElementsMatch(t, expectedValues, actualValues,
						"Variable %s has unexpected values.\nExpected: %v\nActual: %v",
						varName, expectedValues, actualValues)
				}
			}
		})
	}
}

func TestShellInitScriptHelperFunctions(t *testing.T) {
	// Skip on Windows
	testutil.SkipOnWindows(t)

	// Create temporary directories
	tempDir := testutil.TempDir(t, "shell-helpers-test")
	dataDir := filepath.Join(tempDir, "data")
	dotfilesRoot := filepath.Join(tempDir, "dotfiles")

	require.NoError(t, os.MkdirAll(dataDir, 0755))
	require.NoError(t, os.MkdirAll(dotfilesRoot, 0755))

	// Create deployment metadata
	metadata := fmt.Sprintf("export DODOT_DEPLOYMENT_ROOT=\"%s\"\n", dotfilesRoot)
	testutil.CreateFile(t, dataDir, "deployment-metadata", metadata)

	// Setup some test data
	testutil.CreateDir(t, dataDir, "deployed/symlink")
	vimrcPath := filepath.Join(dotfilesRoot, "vim", ".vimrc")
	testutil.CreateDir(t, dotfilesRoot, "vim")
	testutil.CreateFile(t, filepath.Dir(vimrcPath), ".vimrc", "\" vim config")
	testutil.CreateSymlink(t, vimrcPath, filepath.Join(dataDir, "deployed/symlink/.vimrc"))

	// Get the path to dodot-init.sh
	shellInitPath, err := filepath.Abs("dodot-init.sh")
	require.NoError(t, err)

	// Test dodot_tracked function
	t.Run("dodot_tracked_function", func(t *testing.T) {
		testScript := fmt.Sprintf(`#!/bin/bash
set -e
export DODOT_DATA_DIR="%s"
source "%s"

# Call the helper function
dodot_tracked
`, dataDir, shellInitPath)

		testScriptPath := filepath.Join(tempDir, "test_tracked.sh")
		testutil.CreateFile(t, tempDir, "test_tracked.sh", testScript)
		testutil.Chmod(t, testScriptPath, 0755)

		cmd := exec.Command("bash", testScriptPath)
		output, err := cmd.CombinedOutput()
		require.NoError(t, err, "Script execution failed: %s", string(output))

		// Verify the output contains expected strings
		outputStr := string(output)
		assert.Contains(t, outputStr, "dodot tracked deployments")
		assert.Contains(t, outputStr, "Symlinks:")
		assert.Contains(t, outputStr, "vim/.vimrc")
	})

	// Test dodot_should_run_once function
	t.Run("dodot_should_run_once_function", func(t *testing.T) {
		// Create a sentinel file
		testutil.CreateDir(t, dataDir, "install/sentinels")
		testutil.CreateFile(t, filepath.Join(dataDir, "install/sentinels"), "testpack", "abc123")

		testScript := fmt.Sprintf(`#!/bin/bash
set -e
export DODOT_DATA_DIR="%s"
source "%s"

# Test cases
# Should run - no sentinel
if dodot_should_run_once "provision" "newpack" "xyz789"; then
    echo "PASS: newpack should run"
else
    echo "FAIL: newpack should run"
fi

# Should not run - same checksum
if dodot_should_run_once "install/sentinels" "testpack" "abc123"; then
    echo "FAIL: testpack should not run (same checksum)"
else
    echo "PASS: testpack should not run (same checksum)"
fi

# Should run - different checksum
if dodot_should_run_once "install/sentinels" "testpack" "def456"; then
    echo "PASS: testpack should run (different checksum)"
else
    echo "FAIL: testpack should run (different checksum)"
fi
`, dataDir, shellInitPath)

		testScriptPath := filepath.Join(tempDir, "test_should_run.sh")
		testutil.CreateFile(t, tempDir, "test_should_run.sh", testScript)
		testutil.Chmod(t, testScriptPath, 0755)

		cmd := exec.Command("bash", testScriptPath)
		output, err := cmd.CombinedOutput()
		require.NoError(t, err, "Script execution failed: %s", string(output))

		// Check all tests passed
		outputStr := string(output)
		assert.NotContains(t, outputStr, "FAIL:", "Some tests failed:\n%s", outputStr)
		assert.Contains(t, outputStr, "PASS: newpack should run")
		assert.Contains(t, outputStr, "PASS: testpack should not run")
		assert.Contains(t, outputStr, "PASS: testpack should run")
	})
}
