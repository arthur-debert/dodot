package testutil

import (
	"path/filepath"
	"testing"

	"github.com/arthur-debert/dodot/pkg/types"
)

// CreateFileT creates a file in the given synthfs filesystem
func CreateFileT(t *testing.T, fs types.FS, path, content string) {
	t.Helper()

	// Create parent directories if needed
	dir := filepath.Dir(path)
	if err := fs.MkdirAll(dir, 0755); err != nil {
		t.Fatalf("Failed to create parent directories for %s: %v", path, err)
	}

	// Write the file
	if err := fs.WriteFile(path, []byte(content), 0644); err != nil {
		t.Fatalf("Failed to create file %s: %v", path, err)
	}
}

// CreateDirT creates a directory in the given synthfs filesystem
func CreateDirT(t *testing.T, fs types.FS, path string) {
	t.Helper()

	if err := fs.MkdirAll(path, 0755); err != nil {
		t.Fatalf("Failed to create directory %s: %v", path, err)
	}
}
