// Test Type: Unit Test
// Description: Tests for the paths package - main Paths struct and functions

package paths_test

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/arthur-debert/dodot/pkg/paths"
	"github.com/stretchr/testify/assert"
)

func TestNew(t *testing.T) {
	t.Skip("TestNew uses real filesystem - this is an integration test")
}

func TestPathsGetters(t *testing.T) {
	t.Skip("TestPathsGetters uses real filesystem - this is an integration test")
}

func TestNormalizePath(t *testing.T) {
	t.Skip("TestNormalizePath uses real filesystem - this is an integration test")
}

func TestIsInDotfiles(t *testing.T) {
	t.Skip("TestIsInDotfiles uses real filesystem - this is an integration test")
}

func TestExpandHome(t *testing.T) {
	homeDir := os.Getenv("HOME")
	if homeDir == "" {
		homeDir = os.Getenv("USERPROFILE") // Windows
	}

	tests := []struct {
		name     string
		path     string
		expected string
	}{
		{
			name:     "tilde_at_start",
			path:     "~/dotfiles",
			expected: filepath.Join(homeDir, "dotfiles"),
		},
		{
			name:     "no_tilde",
			path:     "/home/user/dotfiles",
			expected: "/home/user/dotfiles",
		},
		{
			name:     "tilde_not_at_start",
			path:     "/home/~user/dotfiles",
			expected: "/home/~user/dotfiles",
		},
		{
			name:     "just_tilde",
			path:     "~",
			expected: homeDir,
		},
		{
			name:     "empty_path",
			path:     "",
			expected: "",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result := paths.ExpandHome(tt.path)
			assert.Equal(t, tt.expected, result)
		})
	}
}
