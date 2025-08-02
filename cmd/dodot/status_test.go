package dodot

import (
	"bytes"
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"testing"
	"time"

	"github.com/arthur-debert/dodot/pkg/paths"
	"github.com/arthur-debert/dodot/pkg/testutil"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	// Import to register triggers and powerups
	_ "github.com/arthur-debert/dodot/pkg/powerups"
	_ "github.com/arthur-debert/dodot/pkg/triggers"
)

func captureOutput(f func()) (string, error) {
	// Create a pipe to capture stdout
	r, w, err := os.Pipe()
	if err != nil {
		return "", err
	}

	// Save the original stdout
	oldStdout := os.Stdout
	os.Stdout = w

	// Create a channel to capture the output
	outputChan := make(chan string)
	go func() {
		var buf bytes.Buffer
		_, _ = buf.ReadFrom(r)
		outputChan <- buf.String()
	}()

	// Execute the function
	f()

	// Restore stdout and close the writer
	os.Stdout = oldStdout
	_ = w.Close()

	// Get the captured output
	output := <-outputChan
	return output, nil
}

func TestStatusCommand(t *testing.T) {
	tests := []struct {
		name           string
		setup          func(t *testing.T) (string, string) // returns dotfilesRoot, homeDir
		args           []string
		expectedOutput []string
		notExpected    []string
		wantErr        bool
	}{
		{
			name: "status with no packs",
			setup: func(t *testing.T) (string, string) {
				tmpDir := t.TempDir()
				dotfilesRoot := filepath.Join(tmpDir, "dotfiles")
				homeDir := filepath.Join(tmpDir, "home")
				require.NoError(t, os.MkdirAll(dotfilesRoot, 0755))
				require.NoError(t, os.MkdirAll(homeDir, 0755))
				return dotfilesRoot, homeDir
			},
			args:           []string{},
			expectedOutput: []string{}, // No packs to show
			wantErr:        false,
		},
		{
			name: "status of single pack with install script",
			setup: func(t *testing.T) (string, string) {
				tmpDir := t.TempDir()
				dotfilesRoot := filepath.Join(tmpDir, "dotfiles")
				homeDir := filepath.Join(tmpDir, "home")

				// Create vim pack with install script
				vimDir := filepath.Join(dotfilesRoot, "vim")
				require.NoError(t, os.MkdirAll(vimDir, 0755))

				installScript := `#!/bin/bash
echo "Installing vim plugins..."
mkdir -p ~/.vim/bundle`
				require.NoError(t, os.WriteFile(
					filepath.Join(vimDir, "install.sh"),
					[]byte(installScript),
					0755,
				))

				return dotfilesRoot, homeDir
			},
			args: []string{"vim"},
			expectedOutput: []string{
				"vim:",
				"install: Not Installed",
				"Install script not yet executed",
				"symlink: Unknown",
			},
			wantErr: false,
		},
		{
			name: "status of pack with executed install script",
			setup: func(t *testing.T) (string, string) {
				tmpDir := t.TempDir()
				dotfilesRoot := filepath.Join(tmpDir, "dotfiles")
				homeDir := filepath.Join(tmpDir, "home")

				// Create vim pack with install script
				vimDir := filepath.Join(dotfilesRoot, "vim")
				require.NoError(t, os.MkdirAll(vimDir, 0755))

				installScript := `#!/bin/bash
echo "Installing vim plugins..."`
				scriptPath := filepath.Join(vimDir, "install.sh")
				require.NoError(t, os.WriteFile(scriptPath, []byte(installScript), 0755))

				// Calculate checksum and create sentinel file
				checksum, err := testutil.CalculateFileChecksum(scriptPath)
				require.NoError(t, err)

				sentinelPath := filepath.Join(paths.GetInstallDir(), "vim")
				require.NoError(t, os.MkdirAll(filepath.Dir(sentinelPath), 0755))
				require.NoError(t, os.WriteFile(sentinelPath, []byte(checksum), 0644))

				return dotfilesRoot, homeDir
			},
			args: []string{"vim"},
			expectedOutput: []string{
				"vim:",
				"install: Installed",
				"Installed on",
			},
			wantErr: false,
		},
		{
			name: "status of pack with brewfile",
			setup: func(t *testing.T) (string, string) {
				tmpDir := t.TempDir()
				dotfilesRoot := filepath.Join(tmpDir, "dotfiles")
				homeDir := filepath.Join(tmpDir, "home")

				// Create tools pack with Brewfile
				toolsDir := filepath.Join(dotfilesRoot, "tools")
				require.NoError(t, os.MkdirAll(toolsDir, 0755))

				brewfile := `brew 'git'
brew 'tmux'
brew 'neovim'`
				require.NoError(t, os.WriteFile(
					filepath.Join(toolsDir, "Brewfile"),
					[]byte(brewfile),
					0644,
				))

				return dotfilesRoot, homeDir
			},
			args: []string{"tools"},
			expectedOutput: []string{
				"tools:",
				"brewfile: Not Installed",
				"Brewfile not yet executed",
			},
			wantErr: false,
		},
		{
			name: "status of all packs when no args provided",
			setup: func(t *testing.T) (string, string) {
				tmpDir := t.TempDir()
				dotfilesRoot := filepath.Join(tmpDir, "dotfiles")
				homeDir := filepath.Join(tmpDir, "home")

				// Create multiple packs
				vimDir := filepath.Join(dotfilesRoot, "vim")
				zshDir := filepath.Join(dotfilesRoot, "zsh")
				gitDir := filepath.Join(dotfilesRoot, "git")

				require.NoError(t, os.MkdirAll(vimDir, 0755))
				require.NoError(t, os.MkdirAll(zshDir, 0755))
				require.NoError(t, os.MkdirAll(gitDir, 0755))

				// Add some files to make them valid packs
				require.NoError(t, os.WriteFile(
					filepath.Join(vimDir, ".vimrc"),
					[]byte("\" vim config"),
					0644,
				))
				require.NoError(t, os.WriteFile(
					filepath.Join(zshDir, ".zshrc"),
					[]byte("# zsh config"),
					0644,
				))
				require.NoError(t, os.WriteFile(
					filepath.Join(gitDir, ".gitconfig"),
					[]byte("[user]\n  name = Test"),
					0644,
				))

				return dotfilesRoot, homeDir
			},
			args: []string{}, // No args = all packs
			expectedOutput: []string{
				"git:",
				"vim:",
				"zsh:",
			},
			wantErr: false,
		},
		{
			name: "status of non-existent pack",
			setup: func(t *testing.T) (string, string) {
				tmpDir := t.TempDir()
				dotfilesRoot := filepath.Join(tmpDir, "dotfiles")
				homeDir := filepath.Join(tmpDir, "home")

				// Create one valid pack
				vimDir := filepath.Join(dotfilesRoot, "vim")
				require.NoError(t, os.MkdirAll(vimDir, 0755))
				require.NoError(t, os.WriteFile(
					filepath.Join(vimDir, ".vimrc"),
					[]byte("\" vim config"),
					0644,
				))

				return dotfilesRoot, homeDir
			},
			args:    []string{"nonexistent"},
			wantErr: true,
		},
		{
			name: "status of pack with changed install script",
			setup: func(t *testing.T) (string, string) {
				tmpDir := t.TempDir()
				dotfilesRoot := filepath.Join(tmpDir, "dotfiles")
				homeDir := filepath.Join(tmpDir, "home")

				// Create vim pack with install script
				vimDir := filepath.Join(dotfilesRoot, "vim")
				require.NoError(t, os.MkdirAll(vimDir, 0755))

				// First create sentinel with old checksum
				sentinelPath := filepath.Join(paths.GetInstallDir(), "vim")
				require.NoError(t, os.MkdirAll(filepath.Dir(sentinelPath), 0755))
				require.NoError(t, os.WriteFile(sentinelPath, []byte("old-checksum"), 0644))

				// Now create install script with different content
				installScript := `#!/bin/bash
echo "New install script content"`
				require.NoError(t, os.WriteFile(
					filepath.Join(vimDir, "install.sh"),
					[]byte(installScript),
					0755,
				))

				// Touch the sentinel file to give it a valid timestamp
				require.NoError(t, os.Chtimes(sentinelPath, time.Now(), time.Now()))

				return dotfilesRoot, homeDir
			},
			args: []string{"vim"},
			expectedOutput: []string{
				"vim:",
				"install: Changed",
				"script has changed since execution",
			},
			wantErr: false,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			// Clean up any existing sentinel files
			_ = os.RemoveAll(paths.GetInstallDir())
			_ = os.RemoveAll(paths.GetBrewfileDir())

			dotfilesRoot, homeDir := tt.setup(t)

			// Set environment variables
			t.Setenv("DOTFILES_ROOT", dotfilesRoot)
			t.Setenv("HOME", homeDir)
			t.Setenv("DODOT_TEST_MODE", "true")

			// Execute command and capture output
			var output string
			var cmdErr error

			output, err := captureOutput(func() {
				// Create command
				cmd := NewRootCmd()

				// Prepare arguments
				args := append([]string{"status"}, tt.args...)
				cmd.SetArgs(args)

				// Execute command
				cmdErr = cmd.Execute()
			})
			require.NoError(t, err, "Failed to capture output")

			if tt.wantErr {
				assert.Error(t, cmdErr)
				return
			}

			require.NoError(t, cmdErr)

			// Check expected output
			for _, expected := range tt.expectedOutput {
				assert.Contains(t, output, expected,
					"Expected output to contain %q, but got:\n%s", expected, output)
			}

			// Check not expected output
			for _, notExpected := range tt.notExpected {
				assert.NotContains(t, output, notExpected,
					"Expected output NOT to contain %q, but got:\n%s", notExpected, output)
			}
		})
	}
}

func TestStatusCommandWithSymlinks(t *testing.T) {
	// This test specifically checks symlink status reporting
	// Currently symlinks are reported as "Unknown" - this test documents that behavior

	tmpDir := t.TempDir()
	dotfilesRoot := filepath.Join(tmpDir, "dotfiles")
	homeDir := filepath.Join(tmpDir, "home")

	// Create vim pack with a file to symlink
	vimDir := filepath.Join(dotfilesRoot, "vim")
	require.NoError(t, os.MkdirAll(vimDir, 0755))

	vimrcContent := "\" Test vimrc"
	vimrcPath := filepath.Join(vimDir, ".vimrc")
	require.NoError(t, os.WriteFile(vimrcPath, []byte(vimrcContent), 0644))

	// Don't create the symlink - just test that status reports it as Unknown
	// When symlink status checking is implemented, we can add tests for actual symlinks

	// Set environment variables
	t.Setenv("DOTFILES_ROOT", dotfilesRoot)
	t.Setenv("HOME", homeDir)
	t.Setenv("DODOT_TEST_MODE", "true")

	// Execute and capture output
	output, err := captureOutput(func() {
		cmd := NewRootCmd()
		cmd.SetArgs([]string{"status", "vim"})
		_ = cmd.Execute()
	})
	require.NoError(t, err)

	// Currently symlinks are reported as "Unknown"
	assert.Contains(t, output, "symlink: Unknown")
	assert.Contains(t, output, "Symlink status checking not yet implemented")

	// TODO: When symlink checking is implemented, update this test to verify:
	// - Existing symlinks are reported as "Linked"
	// - Missing symlinks are reported as "Not Linked"
	// - Broken symlinks are reported as "Broken"
}

func TestStatusCommandErrorCases(t *testing.T) {
	tests := []struct {
		name        string
		setup       func(t *testing.T) string
		args        []string
		expectedErr string
	}{
		{
			name: "invalid dotfiles root",
			setup: func(t *testing.T) string {
				return "/nonexistent/path"
			},
			args:        []string{},
			expectedErr: "dotfiles root does not exist",
		},
		{
			name: "pack not found",
			setup: func(t *testing.T) string {
				tmpDir := t.TempDir()
				dotfilesRoot := filepath.Join(tmpDir, "dotfiles")
				require.NoError(t, os.MkdirAll(dotfilesRoot, 0755))
				return dotfilesRoot
			},
			args:        []string{"nonexistent"},
			expectedErr: "pack(s) not found",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			dotfilesRoot := tt.setup(t)

			t.Setenv("DOTFILES_ROOT", dotfilesRoot)
			t.Setenv("DODOT_TEST_MODE", "true")

			var errorOutput string
			var cmdErr error

			// Try to capture both stdout and stderr
			_, captureErr := captureOutput(func() {
				cmd := NewRootCmd()
				args := append([]string{"status"}, tt.args...)
				cmd.SetArgs(args)

				// Capture stderr
				var errBuf bytes.Buffer
				cmd.SetErr(&errBuf)

				cmdErr = cmd.Execute()
				errorOutput = errBuf.String()
			})

			require.NoError(t, captureErr)
			assert.Error(t, cmdErr)

			if tt.expectedErr != "" {
				combinedError := fmt.Sprintf("%s %v", errorOutput, cmdErr)
				assert.Contains(t, strings.ToLower(combinedError), strings.ToLower(tt.expectedErr))
			}
		})
	}
}
