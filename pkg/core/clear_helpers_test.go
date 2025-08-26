package core_test

import (
	"io/fs"
	"os"
	"testing"
	"time"

	"github.com/arthur-debert/dodot/pkg/core"
	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/stretchr/testify/assert"
)

// Mock FS for testing FilterHandlersByState
type mockFilterFS struct {
	existingDirs map[string]bool
}

func (m *mockFilterFS) Stat(name string) (fs.FileInfo, error) {
	if m.existingDirs[name] {
		return &mockFileInfo{name: name, isDir: true}, nil
	}
	return nil, os.ErrNotExist
}

func (m *mockFilterFS) Lstat(name string) (fs.FileInfo, error)                     { return nil, nil }
func (m *mockFilterFS) ReadDir(name string) ([]fs.DirEntry, error)                 { return nil, nil }
func (m *mockFilterFS) ReadFile(name string) ([]byte, error)                       { return nil, nil }
func (m *mockFilterFS) WriteFile(name string, data []byte, perm fs.FileMode) error { return nil }
func (m *mockFilterFS) MkdirAll(path string, perm fs.FileMode) error               { return nil }
func (m *mockFilterFS) Remove(name string) error                                   { return nil }
func (m *mockFilterFS) RemoveAll(path string) error                                { return nil }
func (m *mockFilterFS) Symlink(oldname, newname string) error                      { return nil }
func (m *mockFilterFS) Readlink(name string) (string, error)                       { return "", nil }

// Mock FileInfo
type mockFileInfo struct {
	name  string
	isDir bool
}

func (m *mockFileInfo) Name() string       { return m.name }
func (m *mockFileInfo) Size() int64        { return 0 }
func (m *mockFileInfo) Mode() fs.FileMode  { return 0755 }
func (m *mockFileInfo) ModTime() time.Time { return time.Now() }
func (m *mockFileInfo) IsDir() bool        { return m.isDir }
func (m *mockFileInfo) Sys() interface{}   { return nil }

// Mock Paths for testing
type mockFilterPaths struct{}

func (m *mockFilterPaths) PackHandlerDir(packName, handlerName string) string {
	// Use the actual state directory name (e.g., "symlinks" for "symlink" handler)
	stateDirName := handlerName
	switch handlerName {
	case "symlink":
		stateDirName = "symlinks"
	}
	return "/data/packs/" + packName + "/" + stateDirName
}

func (m *mockFilterPaths) MapPackFileToSystem(pack *types.Pack, relPath string) string {
	return "/home/" + relPath
}

func TestFilterHandlersByState(t *testing.T) {
	tests := []struct {
		name         string
		handlers     map[string]types.Clearable
		existingDirs map[string]bool
		expected     []string
	}{
		{
			name: "all handlers have state",
			handlers: map[string]types.Clearable{
				"symlink": &mockClearableHandler{name: "symlink"},
				"path":    &mockClearableHandler{name: "path"},
			},
			existingDirs: map[string]bool{
				"/data/packs/testpack/symlinks": true, // symlink handler uses "symlinks" directory
				"/data/packs/testpack/path":     true,
			},
			expected: []string{"symlink", "path"},
		},
		{
			name: "some handlers have state",
			handlers: map[string]types.Clearable{
				"symlink": &mockClearableHandler{name: "symlink"},
				"path":    &mockClearableHandler{name: "path"},
				"shell":   &mockClearableHandler{name: "shell"},
			},
			existingDirs: map[string]bool{
				"/data/packs/testpack/symlinks": true, // symlink handler uses "symlinks" directory
				// path has no state
				"/data/packs/testpack/shell": true,
			},
			expected: []string{"symlink", "shell"},
		},
		{
			name: "no handlers have state",
			handlers: map[string]types.Clearable{
				"symlink": &mockClearableHandler{name: "symlink"},
				"path":    &mockClearableHandler{name: "path"},
			},
			existingDirs: map[string]bool{},
			expected:     []string{},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			ctx := types.ClearContext{
				Pack: types.Pack{
					Name: "testpack",
				},
				FS:    &mockFilterFS{existingDirs: tt.existingDirs},
				Paths: &mockFilterPaths{},
			}

			filtered := core.FilterHandlersByState(ctx, tt.handlers)

			// Check we got the expected handlers
			assert.Len(t, filtered, len(tt.expected))
			for _, expectedName := range tt.expected {
				_, ok := filtered[expectedName]
				assert.True(t, ok, "Expected handler %s to be in filtered results", expectedName)
			}
		})
	}
}

func TestGetClearableHandlersByMode_Integration(t *testing.T) {
	// This test would require the registry to be populated with actual handlers
	// For now, we'll skip it in unit tests as it requires global state
	t.Skip("Integration test - requires populated registry")
}

func TestGetAllClearableHandlers_Integration(t *testing.T) {
	// This test would require the registry to be populated with actual handlers
	// For now, we'll skip it in unit tests as it requires global state
	t.Skip("Integration test - requires populated registry")
}
