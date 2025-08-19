package types_test

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/arthur-debert/dodot/pkg/testutil"
	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

// TestPackFileExists tests with filesystem operations
// This is an integration test because it uses filesystem operations
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

// TestPackFileExists_ErrorHandling tests that FileExists properly returns filesystem errors
// This is an integration test because it tests filesystem error handling
func TestPackFileExists_ErrorHandling(t *testing.T) {
	// For this test, we'll create a mock FS that returns specific errors
	// Since testutil.TestFS doesn't support permission errors, we'll test the logic
	// by verifying that non-IsNotExist errors are propagated

	pack := &types.Pack{
		Name: "test-pack",
		Path: "/some/path",
	}

	// Create a minimal mock that returns a custom error
	mockFS := &mockFSWithError{
		err: os.ErrPermission,
	}

	exists, err := pack.FileExists(mockFS, "file.txt")
	assert.False(t, exists)
	assert.Error(t, err)
	assert.Equal(t, os.ErrPermission, err)
}

// mockFSWithError is a minimal FS implementation for testing error handling
type mockFSWithError struct {
	err error
}

func (m *mockFSWithError) Stat(name string) (os.FileInfo, error) {
	return nil, m.err
}

func (m *mockFSWithError) ReadFile(name string) ([]byte, error) {
	return nil, m.err
}

func (m *mockFSWithError) WriteFile(name string, data []byte, perm os.FileMode) error {
	return m.err
}

func (m *mockFSWithError) MkdirAll(path string, perm os.FileMode) error {
	return m.err
}

func (m *mockFSWithError) ReadDir(name string) ([]os.DirEntry, error) {
	return nil, m.err
}

func (m *mockFSWithError) Symlink(oldname, newname string) error {
	return m.err
}

func (m *mockFSWithError) Readlink(name string) (string, error) {
	return "", m.err
}

func (m *mockFSWithError) Remove(name string) error {
	return m.err
}

func (m *mockFSWithError) RemoveAll(path string) error {
	return m.err
}

func (m *mockFSWithError) Lstat(name string) (os.FileInfo, error) {
	return nil, m.err
}

// TestPackCreateFile tests file creation
// This is an integration test because it creates files in the filesystem
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

// TestPackReadFile tests file reading
// This is an integration test because it reads files from the filesystem
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
