package dodot

import (
	"bytes"
	"io"
	"os"
	"strings"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

// captureStdout captures stdout during command execution
func captureStdout(t *testing.T, fn func()) string {
	oldStdout := os.Stdout
	r, w, err := os.Pipe()
	require.NoError(t, err)

	os.Stdout = w

	// Execute the function
	fn()

	// Restore stdout
	_ = w.Close()
	os.Stdout = oldStdout

	// Read captured output
	var buf bytes.Buffer
	_, err = io.Copy(&buf, r)
	require.NoError(t, err)

	return buf.String()
}

// TestSnippetCommandDefaultBash tests the snippet command with default bash output
func TestSnippetCommandDefaultBash(t *testing.T) {
	output := captureStdout(t, func() {
		rootCmd := NewRootCmd()
		rootCmd.SetArgs([]string{"snippet"})
		err := rootCmd.Execute()
		require.NoError(t, err)
	})

	// Verify the output contains expected bash/zsh snippet
	assert.Contains(t, output, `[ -f "$HOME/.local/share/dodot/shell/dodot-init.sh" ]`)
	assert.Contains(t, output, `source "$HOME/.local/share/dodot/shell/dodot-init.sh"`)
}

// TestSnippetCommandBashExplicit tests the snippet command with explicit bash flag
func TestSnippetCommandBashExplicit(t *testing.T) {
	output := captureStdout(t, func() {
		rootCmd := NewRootCmd()
		rootCmd.SetArgs([]string{"snippet", "--shell", "bash"})
		err := rootCmd.Execute()
		require.NoError(t, err)
	})

	assert.Contains(t, output, `[ -f "$HOME/.local/share/dodot/shell/dodot-init.sh" ]`)
	assert.Contains(t, output, `source "$HOME/.local/share/dodot/shell/dodot-init.sh"`)
}

// TestSnippetCommandZsh tests the snippet command with zsh flag
func TestSnippetCommandZsh(t *testing.T) {
	output := captureStdout(t, func() {
		rootCmd := NewRootCmd()
		rootCmd.SetArgs([]string{"snippet", "--shell", "zsh"})
		err := rootCmd.Execute()
		require.NoError(t, err)
	})

	assert.Contains(t, output, `[ -f "$HOME/.local/share/dodot/shell/dodot-init.sh" ]`)
	assert.Contains(t, output, `source "$HOME/.local/share/dodot/shell/dodot-init.sh"`)
}

// TestSnippetCommandFish tests the snippet command with fish flag
func TestSnippetCommandFish(t *testing.T) {
	output := captureStdout(t, func() {
		rootCmd := NewRootCmd()
		rootCmd.SetArgs([]string{"snippet", "--shell", "fish"})
		err := rootCmd.Execute()
		require.NoError(t, err)
	})

	assert.Contains(t, output, `if test -f "$HOME/.local/share/dodot/shell/dodot-init.fish"`)
	assert.Contains(t, output, `source "$HOME/.local/share/dodot/shell/dodot-init.fish"`)
	assert.Contains(t, output, "end")
}

// TestSnippetCommandShortFlag tests the snippet command with short shell flag
func TestSnippetCommandShortFlag(t *testing.T) {
	output := captureStdout(t, func() {
		rootCmd := NewRootCmd()
		rootCmd.SetArgs([]string{"snippet", "-s", "fish"})
		err := rootCmd.Execute()
		require.NoError(t, err)
	})

	assert.Contains(t, output, `if test -f "$HOME/.local/share/dodot/shell/dodot-init.fish"`)
}

// TestSnippetCommandCustomDataDir tests the snippet command with custom DODOT_DATA_DIR
func TestSnippetCommandCustomDataDir(t *testing.T) {
	// Set custom data directory
	customDir := "/custom/dodot/path"
	require.NoError(t, os.Setenv("DODOT_DATA_DIR", customDir))
	defer func() { _ = os.Unsetenv("DODOT_DATA_DIR") }()

	output := captureStdout(t, func() {
		rootCmd := NewRootCmd()
		rootCmd.SetArgs([]string{"snippet"})
		err := rootCmd.Execute()
		require.NoError(t, err)
	})

	assert.Contains(t, output, customDir+"/shell/dodot-init.sh")
	assert.NotContains(t, output, "$HOME/.local/share/dodot")
}

// TestSnippetCommandCustomDataDirFish tests fish snippet with custom data directory
func TestSnippetCommandCustomDataDirFish(t *testing.T) {
	customDir := "/custom/dodot/path"
	require.NoError(t, os.Setenv("DODOT_DATA_DIR", customDir))
	defer func() { _ = os.Unsetenv("DODOT_DATA_DIR") }()

	output := captureStdout(t, func() {
		rootCmd := NewRootCmd()
		rootCmd.SetArgs([]string{"snippet", "--shell", "fish"})
		err := rootCmd.Execute()
		require.NoError(t, err)
	})

	assert.Contains(t, output, customDir+"/shell/dodot-init.fish")
	assert.NotContains(t, output, "$HOME/.local/share/dodot")
}

// TestSnippetCommandOutput tests that the snippet output is correct
func TestSnippetCommandOutput(t *testing.T) {
	output := captureStdout(t, func() {
		rootCmd := NewRootCmd()
		rootCmd.SetArgs([]string{"snippet"})
		err := rootCmd.Execute()
		require.NoError(t, err)
	})

	// The output should be exactly the snippet
	expectedSnippet := `[ -f "$HOME/.local/share/dodot/shell/dodot-init.sh" ] && source "$HOME/.local/share/dodot/shell/dodot-init.sh"`
	assert.Equal(t, expectedSnippet, strings.TrimSpace(output))
}

// TestSnippetCommandFishOutput tests fish-specific output format
func TestSnippetCommandFishOutput(t *testing.T) {
	output := captureStdout(t, func() {
		rootCmd := NewRootCmd()
		rootCmd.SetArgs([]string{"snippet", "--shell", "fish"})
		err := rootCmd.Execute()
		require.NoError(t, err)
	})

	expectedSnippet := `if test -f "$HOME/.local/share/dodot/shell/dodot-init.fish"
    source "$HOME/.local/share/dodot/shell/dodot-init.fish"
end`
	assert.Equal(t, expectedSnippet, strings.TrimSpace(output))
}
