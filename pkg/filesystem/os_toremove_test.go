package filesystem

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestNewOS(t *testing.T) {
	// Test that NewOS returns a valid filesystem
	fs := NewOS()
	assert.NotNil(t, fs)

	// Test basic operations
	tmpDir := t.TempDir()
	testFile := filepath.Join(tmpDir, "test.txt")
	testContent := []byte("hello world")

	// Test WriteFile
	err := fs.WriteFile(testFile, testContent, 0644)
	require.NoError(t, err)

	// Test Stat
	info, err := fs.Stat(testFile)
	require.NoError(t, err)
	assert.Equal(t, "test.txt", info.Name())
	assert.Equal(t, int64(len(testContent)), info.Size())

	// Test ReadFile
	content, err := fs.ReadFile(testFile)
	require.NoError(t, err)
	assert.Equal(t, testContent, content)

	// Test MkdirAll
	subDir := filepath.Join(tmpDir, "sub", "dir")
	err = fs.MkdirAll(subDir, 0755)
	require.NoError(t, err)

	// Test ReadDir
	entries, err := fs.ReadDir(tmpDir)
	require.NoError(t, err)
	assert.Len(t, entries, 2) // test.txt and sub/

	// Test Remove
	err = fs.Remove(testFile)
	require.NoError(t, err)
	_, err = fs.Stat(testFile)
	assert.True(t, os.IsNotExist(err))
}
