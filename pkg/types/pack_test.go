package types_test

import (
	"path/filepath"
	"testing"

	"github.com/arthur-debert/dodot/pkg/testutil"
	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

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

func TestPackFileExists(t *testing.T) {
	fs := testutil.NewTestFS()
	packPath := "dotfiles/test-pack"

	// Create pack directory and some files
	require.NoError(t, fs.MkdirAll(packPath, 0755))
	require.NoError(t, fs.WriteFile(filepath.Join(packPath, "existing.txt"), []byte("content"), 0644))
	require.NoError(t, fs.MkdirAll(filepath.Join(packPath, "subdir"), 0755))
	require.NoError(t, fs.WriteFile(filepath.Join(packPath, "subdir/nested.txt"), []byte("nested"), 0644))

	pack := &types.Pack{
		Name: "test-pack",
		Path: packPath,
	}

	tests := []struct {
		name     string
		filename string
		want     bool
		wantErr  bool
	}{
		{
			name:     "existing file",
			filename: "existing.txt",
			want:     true,
			wantErr:  false,
		},
		{
			name:     "non-existing file",
			filename: "missing.txt",
			want:     false,
			wantErr:  false,
		},
		{
			name:     "nested existing file",
			filename: "subdir/nested.txt",
			want:     true,
			wantErr:  false,
		},
		{
			name:     "directory exists",
			filename: "subdir",
			want:     true,
			wantErr:  false,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got, err := pack.FileExists(fs, tt.filename)
			if tt.wantErr {
				assert.Error(t, err)
			} else {
				assert.NoError(t, err)
				assert.Equal(t, tt.want, got)
			}
		})
	}
}

func TestPackCreateFile(t *testing.T) {
	fs := testutil.NewTestFS()
	packPath := "dotfiles/test-pack"

	// Create pack directory
	require.NoError(t, fs.MkdirAll(packPath, 0755))

	pack := &types.Pack{
		Name: "test-pack",
		Path: packPath,
	}

	tests := []struct {
		name     string
		filename string
		content  string
		wantErr  bool
	}{
		{
			name:     "create simple file",
			filename: "new.txt",
			content:  "hello world",
			wantErr:  false,
		},
		{
			name:     "create empty file",
			filename: "empty.txt",
			content:  "",
			wantErr:  false,
		},
		{
			name:     "overwrite existing file",
			filename: "new.txt", // same as first test
			content:  "updated content",
			wantErr:  false,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			err := pack.CreateFile(fs, tt.filename, tt.content)
			if tt.wantErr {
				assert.Error(t, err)
			} else {
				assert.NoError(t, err)
				// Verify file was created with correct content
				fullPath := pack.GetFilePath(tt.filename)
				content, err := fs.ReadFile(fullPath)
				assert.NoError(t, err)
				assert.Equal(t, tt.content, string(content))
			}
		})
	}
}

func TestPackReadFile(t *testing.T) {
	fs := testutil.NewTestFS()
	packPath := "dotfiles/test-pack"

	// Create pack directory and files
	require.NoError(t, fs.MkdirAll(packPath, 0755))
	require.NoError(t, fs.WriteFile(filepath.Join(packPath, "readable.txt"), []byte("file content"), 0644))
	require.NoError(t, fs.MkdirAll(filepath.Join(packPath, "subdir"), 0755))
	require.NoError(t, fs.WriteFile(filepath.Join(packPath, "subdir/nested.txt"), []byte("nested content"), 0644))

	pack := &types.Pack{
		Name: "test-pack",
		Path: packPath,
	}

	tests := []struct {
		name        string
		filename    string
		wantContent string
		wantErr     bool
	}{
		{
			name:        "read existing file",
			filename:    "readable.txt",
			wantContent: "file content",
			wantErr:     false,
		},
		{
			name:        "read nested file",
			filename:    "subdir/nested.txt",
			wantContent: "nested content",
			wantErr:     false,
		},
		{
			name:        "read non-existing file",
			filename:    "missing.txt",
			wantContent: "",
			wantErr:     true,
		},
		{
			name:        "read directory",
			filename:    "subdir",
			wantContent: "",
			wantErr:     true,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			content, err := pack.ReadFile(fs, tt.filename)
			if tt.wantErr {
				assert.Error(t, err)
			} else {
				assert.NoError(t, err)
				assert.Equal(t, tt.wantContent, string(content))
			}
		})
	}
}
