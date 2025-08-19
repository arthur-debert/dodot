package packs

import (
	"testing"

	"github.com/stretchr/testify/assert"
)

func TestShouldIgnoreWithPatterns(t *testing.T) {
	tests := []struct {
		name        string
		fileName    string
		patterns    []string
		wantIgnored bool
	}{
		{
			name:        "exact match",
			fileName:    ".git",
			patterns:    []string{".git"},
			wantIgnored: true,
		},
		{
			name:        "no match",
			fileName:    "mypack",
			patterns:    []string{".git"},
			wantIgnored: false,
		},
		{
			name:        "glob pattern match",
			fileName:    "test.tmp",
			patterns:    []string{"*.tmp"},
			wantIgnored: true,
		},
		{
			name:        "glob pattern no match",
			fileName:    "test.txt",
			patterns:    []string{"*.tmp"},
			wantIgnored: false,
		},
		{
			name:        "multiple patterns - match first",
			fileName:    ".git",
			patterns:    []string{".git", "*.tmp", "node_modules"},
			wantIgnored: true,
		},
		{
			name:        "multiple patterns - match last",
			fileName:    "node_modules",
			patterns:    []string{".git", "*.tmp", "node_modules"},
			wantIgnored: true,
		},
		{
			name:        "multiple patterns - no match",
			fileName:    "mypack",
			patterns:    []string{".git", "*.tmp", "node_modules"},
			wantIgnored: false,
		},
		{
			name:        "empty patterns",
			fileName:    ".git",
			patterns:    []string{},
			wantIgnored: false,
		},
		{
			name:        "complex glob pattern",
			fileName:    "backup.2023.tar",
			patterns:    []string{"backup.*.tar"},
			wantIgnored: true,
		},
		{
			name:        "case sensitive",
			fileName:    ".GIT",
			patterns:    []string{".git"},
			wantIgnored: false,
		},
		{
			name:        "question mark glob",
			fileName:    "test1.tmp",
			patterns:    []string{"test?.tmp"},
			wantIgnored: true,
		},
		{
			name:        "character class glob",
			fileName:    "test5.tmp",
			patterns:    []string{"test[0-9].tmp"},
			wantIgnored: true,
		},
		{
			name:        "character class no match",
			fileName:    "testA.tmp",
			patterns:    []string{"test[0-9].tmp"},
			wantIgnored: false,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got := shouldIgnoreWithPatterns(tt.fileName, tt.patterns)
			assert.Equal(t, tt.wantIgnored, got, "shouldIgnoreWithPatterns(%q, %v)", tt.fileName, tt.patterns)
		})
	}
}

func TestShouldIgnoreWithPatterns_EdgeCases(t *testing.T) {
	tests := []struct {
		name        string
		fileName    string
		patterns    []string
		wantIgnored bool
		description string
	}{
		{
			name:        "empty filename",
			fileName:    "",
			patterns:    []string{"*"},
			wantIgnored: true,
			description: "empty filename should match wildcard",
		},
		{
			name:        "dot file",
			fileName:    ".hidden",
			patterns:    []string{".*"},
			wantIgnored: true,
			description: "dot files should match .* pattern",
		},
		{
			name:        "invalid pattern",
			fileName:    "test",
			patterns:    []string{"["},
			wantIgnored: false,
			description: "invalid pattern should not match (filepath.Match returns error)",
		},
		{
			name:        "path separator in name - match",
			fileName:    "dir/file",
			patterns:    []string{"dir/*"},
			wantIgnored: true,
			description: "on Unix, filepath.Match allows * to match path separators",
		},
		{
			name:        "path separator in pattern - match",
			fileName:    "dir/file",
			patterns:    []string{"dir/file"},
			wantIgnored: true,
			description: "exact match with path separator works",
		},
		{
			name:        "nil patterns",
			fileName:    "test",
			patterns:    nil,
			wantIgnored: false,
			description: "nil patterns should not match anything",
		},
		{
			name:        "empty pattern string",
			fileName:    "test",
			patterns:    []string{""},
			wantIgnored: false,
			description: "empty pattern should not match non-empty filename",
		},
		{
			name:        "empty pattern matches empty filename",
			fileName:    "",
			patterns:    []string{""},
			wantIgnored: true,
			description: "empty pattern should match empty filename",
		},
		{
			name:        "special characters in filename",
			fileName:    "test[1].tmp",
			patterns:    []string{"test[[]1].tmp"},
			wantIgnored: true,
			description: "escaped brackets should match literal brackets",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got := shouldIgnoreWithPatterns(tt.fileName, tt.patterns)
			assert.Equal(t, tt.wantIgnored, got, tt.description)
		})
	}
}
