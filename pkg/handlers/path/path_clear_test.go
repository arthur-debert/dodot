package path_test

import (
	"io/fs"
	"testing"

	"github.com/arthur-debert/dodot/pkg/handlers"
	"github.com/arthur-debert/dodot/pkg/handlers/path"
	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

// Mock DataStore for testing
type mockDataStore struct{}

func (m *mockDataStore) Link(pack, sourceFile string) (string, error) {
	return "", nil
}

func (m *mockDataStore) Unlink(pack, sourceFile string) error {
	return nil
}

func (m *mockDataStore) AddToPath(pack, dirPath string) error {
	return nil
}

func (m *mockDataStore) AddToShellProfile(pack, scriptPath string) error {
	return nil
}

func (m *mockDataStore) RecordProvisioning(pack, sentinelName, checksum string) error {
	return nil
}

func (m *mockDataStore) NeedsProvisioning(pack, sentinelName, checksum string) (bool, error) {
	return false, nil
}

func (m *mockDataStore) GetStatus(pack, sourceFile string) (types.Status, error) {
	return types.Status{}, nil
}

func (m *mockDataStore) GetSymlinkStatus(pack, sourceFile string) (types.Status, error) {
	return types.Status{}, nil
}

func (m *mockDataStore) GetPathStatus(pack, dirPath string) (types.Status, error) {
	return types.Status{}, nil
}

func (m *mockDataStore) GetShellProfileStatus(pack, scriptPath string) (types.Status, error) {
	return types.Status{}, nil
}

func (m *mockDataStore) GetProvisioningStatus(pack, sentinelName, currentChecksum string) (types.Status, error) {
	return types.Status{}, nil
}

func (m *mockDataStore) GetBrewStatus(pack, brewfilePath, currentChecksum string) (types.Status, error) {
	return types.Status{}, nil
}

func (m *mockDataStore) DeleteProvisioningState(packName, handlerName string) error {
	return nil
}

func (m *mockDataStore) GetProvisioningHandlers(packName string) ([]string, error) {
	return []string{}, nil
}

func (m *mockDataStore) ListProvisioningState(packName string) (map[string][]string, error) {
	return map[string][]string{}, nil
}

func TestPathHandler_Clear(t *testing.T) {
	handler := path.NewPathHandler()

	// Create a mock context
	ctx := types.ClearContext{
		Pack: types.Pack{
			Name: "testpack",
			Path: "/test/path",
		},
		DataStore: &mockDataStore{},
		FS:        &mockFS{},
		Paths:     &mockPaths{},
		DryRun:    false,
	}

	clearedItems, err := handler.Clear(ctx)
	require.NoError(t, err)

	// Path handler should return one item indicating state removal
	assert.Len(t, clearedItems, 1)
	assert.Equal(t, "path_state", clearedItems[0].Type)
	assert.Contains(t, clearedItems[0].Description, "PATH entries will be removed")
}

func TestPathHandler_Clear_DryRun(t *testing.T) {
	handler := path.NewPathHandler()

	// Create a mock context with dry run
	ctx := types.ClearContext{
		Pack: types.Pack{
			Name: "testpack",
			Path: "/test/path",
		},
		DataStore: &mockDataStore{},
		FS:        &mockFS{},
		Paths:     &mockPaths{},
		DryRun:    true,
	}

	clearedItems, err := handler.Clear(ctx)
	require.NoError(t, err)

	// Should indicate what would be removed
	assert.Len(t, clearedItems, 1)
	assert.Equal(t, "path_state", clearedItems[0].Type)
	assert.Contains(t, clearedItems[0].Description, "Would remove PATH entries")
}

func TestPathHandler_ImplementsClearable(t *testing.T) {
	handler := path.NewPathHandler()

	// This will fail to compile if PathHandler doesn't implement Clearable
	var _ handlers.Clearable = handler
}

// Mock FS for testing
type mockFS struct{}

func (m *mockFS) Stat(name string) (fs.FileInfo, error)                      { return nil, nil }
func (m *mockFS) Lstat(name string) (fs.FileInfo, error)                     { return nil, nil }
func (m *mockFS) ReadDir(name string) ([]fs.DirEntry, error)                 { return nil, nil }
func (m *mockFS) ReadFile(name string) ([]byte, error)                       { return nil, nil }
func (m *mockFS) WriteFile(name string, data []byte, perm fs.FileMode) error { return nil }
func (m *mockFS) MkdirAll(path string, perm fs.FileMode) error               { return nil }
func (m *mockFS) Remove(name string) error                                   { return nil }
func (m *mockFS) RemoveAll(path string) error                                { return nil }
func (m *mockFS) Symlink(oldname, newname string) error                      { return nil }
func (m *mockFS) Readlink(name string) (string, error)                       { return "", nil }

// Mock Paths for testing
type mockPaths struct{}

func (m *mockPaths) PackHandlerDir(packName, handlerName string) string {
	return "/test/data/packs/" + packName + "/" + handlerName
}

func (m *mockPaths) MapPackFileToSystem(pack *types.Pack, relPath string) string {
	return "/home/user/" + relPath
}
