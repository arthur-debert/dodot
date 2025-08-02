package dodot

import (
	"bytes"
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	// Import to register triggers and powerups
	_ "github.com/arthur-debert/dodot/pkg/powerups"
	_ "github.com/arthur-debert/dodot/pkg/triggers"
)

func TestListCommand(t *testing.T) {
	tests := []struct {
		name           string
		setup          func(t *testing.T) string // returns dotfilesRoot
		expectedOutput []string
		notExpected    []string
		wantErr        bool
		expectedErr    string
	}{
		{
			name: "list with no packs",
			setup: func(t *testing.T) string {
				tmpDir := t.TempDir()
				dotfilesRoot := filepath.Join(tmpDir, "dotfiles")
				require.NoError(t, os.MkdirAll(dotfilesRoot, 0755))
				return dotfilesRoot
			},
			expectedOutput: []string{
				"No packs found.",
			},
			wantErr: false,
		},
		{
			name: "list with single pack",
			setup: func(t *testing.T) string {
				tmpDir := t.TempDir()
				dotfilesRoot := filepath.Join(tmpDir, "dotfiles")
				require.NoError(t, os.MkdirAll(dotfilesRoot, 0755))

				// Create vim pack
				vimDir := filepath.Join(dotfilesRoot, "vim")
				require.NoError(t, os.MkdirAll(vimDir, 0755))
				require.NoError(t, os.WriteFile(
					filepath.Join(vimDir, ".vimrc"),
					[]byte("\" vim config"),
					0644,
				))

				return dotfilesRoot
			},
			expectedOutput: []string{
				"Available packs:",
				"vim",
			},
			wantErr: false,
		},
		{
			name: "list with multiple packs (sorted)",
			setup: func(t *testing.T) string {
				tmpDir := t.TempDir()
				dotfilesRoot := filepath.Join(tmpDir, "dotfiles")
				require.NoError(t, os.MkdirAll(dotfilesRoot, 0755))

				// Create packs in non-alphabetical order
				packs := []string{"zsh", "vim", "git", "tmux", "bash"}
				for _, pack := range packs {
					packDir := filepath.Join(dotfilesRoot, pack)
					require.NoError(t, os.MkdirAll(packDir, 0755))
					// Add a file to make it a valid pack
					require.NoError(t, os.WriteFile(
						filepath.Join(packDir, "config"),
						[]byte("# "+pack+" config"),
						0644,
					))
				}

				return dotfilesRoot
			},
			expectedOutput: []string{
				"Available packs:",
				"bash",
				"git",
				"tmux",
				"vim",
				"zsh",
			},
			wantErr: false,
		},
		{
			name: "list ignores hidden directories",
			setup: func(t *testing.T) string {
				tmpDir := t.TempDir()
				dotfilesRoot := filepath.Join(tmpDir, "dotfiles")
				require.NoError(t, os.MkdirAll(dotfilesRoot, 0755))

				// Create regular packs
				vimDir := filepath.Join(dotfilesRoot, "vim")
				require.NoError(t, os.MkdirAll(vimDir, 0755))
				require.NoError(t, os.WriteFile(
					filepath.Join(vimDir, ".vimrc"),
					[]byte("\" vim config"),
					0644,
				))

				// Create hidden directory (should be ignored)
				hiddenDir := filepath.Join(dotfilesRoot, ".hidden")
				require.NoError(t, os.MkdirAll(hiddenDir, 0755))
				require.NoError(t, os.WriteFile(
					filepath.Join(hiddenDir, "config"),
					[]byte("# hidden config"),
					0644,
				))

				// Create .git directory (should be ignored)
				gitDir := filepath.Join(dotfilesRoot, ".git")
				require.NoError(t, os.MkdirAll(gitDir, 0755))

				return dotfilesRoot
			},
			expectedOutput: []string{
				"Available packs:",
				"vim",
			},
			notExpected: []string{
				".hidden",
				".git",
			},
			wantErr: false,
		},
		{
			name: "list respects .dodotignore",
			setup: func(t *testing.T) string {
				tmpDir := t.TempDir()
				dotfilesRoot := filepath.Join(tmpDir, "dotfiles")
				require.NoError(t, os.MkdirAll(dotfilesRoot, 0755))

				// Create normal pack
				vimDir := filepath.Join(dotfilesRoot, "vim")
				require.NoError(t, os.MkdirAll(vimDir, 0755))
				require.NoError(t, os.WriteFile(
					filepath.Join(vimDir, ".vimrc"),
					[]byte("\" vim config"),
					0644,
				))

				// Create ignored pack
				ignoredDir := filepath.Join(dotfilesRoot, "ignored")
				require.NoError(t, os.MkdirAll(ignoredDir, 0755))
				require.NoError(t, os.WriteFile(
					filepath.Join(ignoredDir, ".dodotignore"),
					[]byte(""),
					0644,
				))
				require.NoError(t, os.WriteFile(
					filepath.Join(ignoredDir, "config"),
					[]byte("# ignored config"),
					0644,
				))

				return dotfilesRoot
			},
			expectedOutput: []string{
				"Available packs:",
				"vim",
			},
			notExpected: []string{
				"ignored",
			},
			wantErr: false,
		},
		{
			name: "list with empty packs (directories without files)",
			setup: func(t *testing.T) string {
				tmpDir := t.TempDir()
				dotfilesRoot := filepath.Join(tmpDir, "dotfiles")
				require.NoError(t, os.MkdirAll(dotfilesRoot, 0755))

				// Create empty directories
				emptyDir := filepath.Join(dotfilesRoot, "empty")
				require.NoError(t, os.MkdirAll(emptyDir, 0755))

				// Create pack with file
				vimDir := filepath.Join(dotfilesRoot, "vim")
				require.NoError(t, os.MkdirAll(vimDir, 0755))
				require.NoError(t, os.WriteFile(
					filepath.Join(vimDir, ".vimrc"),
					[]byte("\" vim config"),
					0644,
				))

				return dotfilesRoot
			},
			expectedOutput: []string{
				"Available packs:",
				"empty", // Empty directories are still considered packs
				"vim",
			},
			wantErr: false,
		},
		{
			name: "list ignores files (not directories)",
			setup: func(t *testing.T) string {
				tmpDir := t.TempDir()
				dotfilesRoot := filepath.Join(tmpDir, "dotfiles")
				require.NoError(t, os.MkdirAll(dotfilesRoot, 0755))

				// Create a file in dotfiles root
				require.NoError(t, os.WriteFile(
					filepath.Join(dotfilesRoot, "README.md"),
					[]byte("# Dotfiles"),
					0644,
				))

				// Create pack directory
				vimDir := filepath.Join(dotfilesRoot, "vim")
				require.NoError(t, os.MkdirAll(vimDir, 0755))
				require.NoError(t, os.WriteFile(
					filepath.Join(vimDir, ".vimrc"),
					[]byte("\" vim config"),
					0644,
				))

				return dotfilesRoot
			},
			expectedOutput: []string{
				"Available packs:",
				"vim",
			},
			notExpected: []string{
				"README.md",
			},
			wantErr: false,
		},
		{
			name: "list with invalid dotfiles root",
			setup: func(t *testing.T) string {
				return "/nonexistent/path"
			},
			wantErr:     true,
			expectedErr: "dotfiles root does not exist",
		},
		{
			name: "list with file as dotfiles root",
			setup: func(t *testing.T) string {
				tmpDir := t.TempDir()
				filePath := filepath.Join(tmpDir, "file")
				require.NoError(t, os.WriteFile(filePath, []byte("content"), 0644))
				return filePath
			},
			wantErr:     true,
			expectedErr: "dotfiles root is not a directory",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			dotfilesRoot := tt.setup(t)

			// Set environment variables
			t.Setenv("DOTFILES_ROOT", dotfilesRoot)
			t.Setenv("DODOT_TEST_MODE", "true")

			// Execute command and capture output
			var output string
			var cmdErr error

			output, err := captureOutput(func() {
				// Create command
				cmd := NewRootCmd()
				cmd.SetArgs([]string{"list"})

				// Execute command
				cmdErr = cmd.Execute()
			})
			require.NoError(t, err, "Failed to capture output")

			if tt.wantErr {
				assert.Error(t, cmdErr)
				if tt.expectedErr != "" {
					assert.Contains(t, strings.ToLower(fmt.Sprintf("%v", cmdErr)), strings.ToLower(tt.expectedErr))
				}
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

func TestListCommandVerbose(t *testing.T) {
	// Test that verbose flag works with list command
	tmpDir := t.TempDir()
	dotfilesRoot := filepath.Join(tmpDir, "dotfiles")
	require.NoError(t, os.MkdirAll(dotfilesRoot, 0755))

	// Create a pack
	vimDir := filepath.Join(dotfilesRoot, "vim")
	require.NoError(t, os.MkdirAll(vimDir, 0755))
	require.NoError(t, os.WriteFile(
		filepath.Join(vimDir, ".vimrc"),
		[]byte("\" vim config"),
		0644,
	))

	t.Setenv("DOTFILES_ROOT", dotfilesRoot)
	t.Setenv("DODOT_TEST_MODE", "true")

	// Run with -v flag to get INFO level logs
	var errBuf bytes.Buffer
	output, err := captureOutput(func() {
		cmd := NewRootCmd()
		cmd.SetArgs([]string{"-v", "list"})
		cmd.SetErr(&errBuf)
		_ = cmd.Execute()
	})
	require.NoError(t, err)

	// Should see the packs
	assert.Contains(t, output, "vim")

	// With verbose flag, we might see log messages in stderr
	// (depending on how logging is configured)
	errOutput := errBuf.String()
	_ = errOutput // Available for debugging if needed
}

func TestListCommandWithFallbackWarning(t *testing.T) {
	// Test list command when DOTFILES_ROOT is not set (uses fallback)
	tmpDir := t.TempDir()

	// Change to tmpDir so it becomes the fallback
	oldWd, err := os.Getwd()
	require.NoError(t, err)
	require.NoError(t, os.Chdir(tmpDir))
	defer func() { _ = os.Chdir(oldWd) }()

	// Create a pack in current directory
	vimDir := filepath.Join(tmpDir, "vim")
	require.NoError(t, os.MkdirAll(vimDir, 0755))
	require.NoError(t, os.WriteFile(
		filepath.Join(vimDir, ".vimrc"),
		[]byte("\" vim config"),
		0644,
	))

	// Unset DOTFILES_ROOT to trigger fallback
	t.Setenv("DOTFILES_ROOT", "")
	t.Setenv("DODOT_TEST_MODE", "true")

	// Capture both stdout and stderr
	oldStderr := os.Stderr
	r, w, _ := os.Pipe()
	os.Stderr = w

	var errOutput string
	done := make(chan bool)
	go func() {
		buf := new(bytes.Buffer)
		_, _ = buf.ReadFrom(r)
		errOutput = buf.String()
		done <- true
	}()

	output, err := captureOutput(func() {
		cmd := NewRootCmd()
		cmd.SetArgs([]string{"list"})
		_ = cmd.Execute()
	})
	require.NoError(t, err)

	// Restore stderr
	os.Stderr = oldStderr
	_ = w.Close()
	<-done

	// Should see the pack
	assert.Contains(t, output, "vim")

	// Should see warning about fallback in stderr
	assert.Contains(t, errOutput, "Warning") // Fallback warning
}
