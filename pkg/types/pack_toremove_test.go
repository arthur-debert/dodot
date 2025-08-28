package types_test

import (
	"path/filepath"
	"testing"

	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/stretchr/testify/assert"
)

// TestPackGetFilePath tests path concatenation logic
// This is a unit test - it tests pure logic without any filesystem operations
func TestPackGetFilePath(t *testing.T) {
	pack := &types.Pack{
		Name: "test-pack",
		Path: "/home/user/dotfiles/test-pack",
	}

	tests := []struct {
		name     string
		filename string
		want     string
	}{
		{
			name:     "simple filename",
			filename: "config.toml",
			want:     "/home/user/dotfiles/test-pack/config.toml",
		},
		{
			name:     "filename with directory",
			filename: "subdir/file.txt",
			want:     "/home/user/dotfiles/test-pack/subdir/file.txt",
		},
		{
			name:     "empty filename",
			filename: "",
			want:     "/home/user/dotfiles/test-pack",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got := pack.GetFilePath(tt.filename)
			assert.Equal(t, filepath.Clean(tt.want), filepath.Clean(got))
		})
	}
}
