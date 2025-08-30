// pkg/types/pack_test.go
// TEST TYPE: Unit Tests
// DEPENDENCIES: None
// PURPOSE: Test Pack type methods

package types_test

import (
	"path/filepath"
	"testing"

	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/stretchr/testify/assert"
)

func TestPack_GetFilePath(t *testing.T) {
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
			name:     "simple_filename",
			filename: "config.toml",
			want:     "/home/user/dotfiles/test-pack/config.toml",
		},
		{
			name:     "filename_with_directory",
			filename: "subdir/file.txt",
			want:     "/home/user/dotfiles/test-pack/subdir/file.txt",
		},
		{
			name:     "empty_filename",
			filename: "",
			want:     "/home/user/dotfiles/test-pack",
		},
		{
			name:     "absolute_path_gets_joined",
			filename: "/absolute/path",
			want:     "/home/user/dotfiles/test-pack/absolute/path",
		},
		{
			name:     "dotfile",
			filename: ".vimrc",
			want:     "/home/user/dotfiles/test-pack/.vimrc",
		},
		{
			name:     "nested_dotfile",
			filename: ".config/nvim/init.vim",
			want:     "/home/user/dotfiles/test-pack/.config/nvim/init.vim",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got := pack.GetFilePath(tt.filename)
			assert.Equal(t, filepath.Clean(tt.want), filepath.Clean(got))
		})
	}
}