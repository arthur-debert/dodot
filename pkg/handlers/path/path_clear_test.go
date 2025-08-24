package path_test

import (
	"testing"

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

func TestPathHandler_PreClear(t *testing.T) {
	handler := path.NewPathHandler()
	pack := types.Pack{
		Name: "testpack",
		Path: "/test/path",
	}
	dataStore := &mockDataStore{}

	clearedItems, err := handler.PreClear(pack, dataStore)
	require.NoError(t, err)

	// Path handler should return empty cleared items
	assert.Empty(t, clearedItems)
}

func TestPathHandler_ImplementsClearable(t *testing.T) {
	handler := path.NewPathHandler()

	// This will fail to compile if PathHandler doesn't implement Clearable
	var _ types.Clearable = handler
}
