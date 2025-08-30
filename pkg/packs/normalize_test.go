// Test Type: Unit Test
// Description: Tests for the packs package - pack name normalization functions

package packs_test

import (
	"testing"

	"github.com/arthur-debert/dodot/pkg/packs"
	"github.com/stretchr/testify/assert"
)

func TestNormalizePackName(t *testing.T) {
	tests := []struct {
		name     string
		input    string
		expected string
	}{
		{
			name:     "no_trailing_slash",
			input:    "vim",
			expected: "vim",
		},
		{
			name:     "single_trailing_slash",
			input:    "vim/",
			expected: "vim",
		},
		{
			name:     "multiple_trailing_slashes",
			input:    "vim///",
			expected: "vim",
		},
		{
			name:     "empty_string",
			input:    "",
			expected: "",
		},
		{
			name:     "only_slashes",
			input:    "///",
			expected: "",
		},
		{
			name:     "path_with_internal_slashes",
			input:    "path/to/vim/",
			expected: "path/to/vim",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result := packs.NormalizePackName(tt.input)
			assert.Equal(t, tt.expected, result)
		})
	}
}

func TestNormalizePackNames(t *testing.T) {
	tests := []struct {
		name     string
		input    []string
		expected []string
	}{
		{
			name:     "empty_slice",
			input:    []string{},
			expected: []string{},
		},
		{
			name:     "single_pack_no_slash",
			input:    []string{"vim"},
			expected: []string{"vim"},
		},
		{
			name:     "single_pack_with_slash",
			input:    []string{"vim/"},
			expected: []string{"vim"},
		},
		{
			name:     "multiple_packs_mixed",
			input:    []string{"vim/", "bash", "git//", "zsh/"},
			expected: []string{"vim", "bash", "git", "zsh"},
		},
		{
			name:     "preserves_order",
			input:    []string{"zsh/", "vim/", "bash/", "git/"},
			expected: []string{"zsh", "vim", "bash", "git"},
		},
		{
			name:     "handles_empty_strings",
			input:    []string{"vim/", "", "bash/"},
			expected: []string{"vim", "", "bash"},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result := packs.NormalizePackNames(tt.input)
			assert.Equal(t, tt.expected, result)
		})
	}
}
