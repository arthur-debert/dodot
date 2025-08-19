package core

import (
	"testing"

	"github.com/arthur-debert/dodot/pkg/testutil"
)

// TestIsPathWithin tests the isPathWithin helper function
// This is a unit test - it tests pure logic without any filesystem operations
func TestIsPathWithin(t *testing.T) {
	tests := []struct {
		name     string
		path     string
		parent   string
		expected bool
	}{
		{
			name:     "simple subdirectory",
			path:     "/home/user/dotfiles/vim/vimrc",
			parent:   "/home/user/dotfiles",
			expected: true,
		},
		{
			name:     "exact match",
			path:     "/home/user/dotfiles",
			parent:   "/home/user/dotfiles",
			expected: true,
		},
		{
			name:     "outside parent",
			path:     "/home/user/other",
			parent:   "/home/user/dotfiles",
			expected: false,
		},
		{
			name:     "parent traversal attempt",
			path:     "/home/user/dotfiles/../other",
			parent:   "/home/user/dotfiles",
			expected: false,
		},
		{
			name:     "relative path within",
			path:     "dotfiles/vim/vimrc",
			parent:   "dotfiles",
			expected: true,
		},
		{
			name:     "relative path outside",
			path:     "../other",
			parent:   "dotfiles",
			expected: false,
		},
		{
			name:     "trailing slashes",
			path:     "/home/user/dotfiles/vim/",
			parent:   "/home/user/dotfiles/",
			expected: true,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result := isPathWithin(tt.path, tt.parent)
			testutil.AssertEqual(t, tt.expected, result)
		})
	}
}
