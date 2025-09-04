package install_test

import (
	"fmt"
	"path/filepath"
	"testing"

	"github.com/arthur-debert/dodot/pkg/handlers/lib/install"
	"github.com/arthur-debert/dodot/pkg/operations"
	"github.com/arthur-debert/dodot/pkg/testutil"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

// MockStatusChecker implements operations.StatusChecker for testing
type MockStatusChecker struct {
	hasDataLink map[string]bool
	hasSentinel map[string]bool
	dataLinkErr error
	sentinelErr error
}

func NewMockStatusChecker() *MockStatusChecker {
	return &MockStatusChecker{
		hasDataLink: make(map[string]bool),
		hasSentinel: make(map[string]bool),
	}
}

func (m *MockStatusChecker) HasDataLink(packName, handlerName, relativePath string) (bool, error) {
	if m.dataLinkErr != nil {
		return false, m.dataLinkErr
	}
	key := packName + ":" + handlerName + ":" + relativePath
	return m.hasDataLink[key], nil
}

func (m *MockStatusChecker) HasSentinel(packName, handlerName, sentinel string) (bool, error) {
	if m.sentinelErr != nil {
		return false, m.sentinelErr
	}
	key := packName + ":" + handlerName + ":" + sentinel
	return m.hasSentinel[key], nil
}

func (m *MockStatusChecker) GetMetadata(packName, handlerName, key string) (string, error) {
	return "", nil
}

func (m *MockStatusChecker) SetSentinel(packName, handlerName, sentinel string, exists bool) {
	key := packName + ":" + handlerName + ":" + sentinel
	m.hasSentinel[key] = exists
}

func TestInstallHandler_CheckStatus(t *testing.T) {
	// Skip this test since it requires filesystem access for checksum calculation
	// The integration test below covers the functionality
	t.Skip("Skipping unit test that requires filesystem access")
}

func TestInstallHandler_CheckStatus_MissingFile(t *testing.T) {
	handler := install.NewHandler()
	checker := NewMockStatusChecker()

	// File that doesn't exist
	file := operations.FileInput{
		PackName:     "myapp",
		SourcePath:   "/nonexistent/install.sh",
		RelativePath: "install.sh",
	}

	status, err := handler.CheckStatus(file, checker)

	// Should error when calculating checksum
	assert.Error(t, err)
	assert.Equal(t, operations.StatusStateError, status.State)
	assert.Contains(t, status.Message, "Failed to calculate checksum")
}

func TestInstallHandler_CheckStatus_Integration(t *testing.T) {
	// This test uses the testutil environment with real filesystem
	// because utils.CalculateFileChecksum uses os.Open directly
	env := testutil.NewTestEnvironment(t, testutil.EnvIsolated)
	defer env.Cleanup()

	// Set up a pack with an install script
	scriptContent := "#!/bin/bash\necho 'Installing...'"
	env.SetupPack("myapp", testutil.PackConfig{
		Files: map[string]string{
			"install.sh": scriptContent,
		},
	})

	// Create handler
	handler := install.NewHandler()

	// Create real status checker
	checker := operations.NewDataStoreStatusChecker(env.DataStore, env.FS, env.Paths)

	// Create file input
	scriptPath := filepath.Join(env.DotfilesRoot, "myapp", "install.sh")
	file := operations.FileInput{
		PackName:     "myapp",
		SourcePath:   scriptPath,
		RelativePath: "install.sh",
	}

	// Calculate expected sentinel using the test helper
	checksum := testutil.GetTestChecksum(scriptContent)
	expectedSentinel := fmt.Sprintf("install.sh-%s", checksum)

	// Initially, sentinel should not exist
	status, err := handler.CheckStatus(file, checker)
	require.NoError(t, err)
	assert.Equal(t, operations.StatusStatePending, status.State)
	assert.Equal(t, "never run", status.Message)

	// Create the sentinel in the datastore
	// Use the mock datastore method to set sentinel without executing command
	mockDS, ok := env.DataStore.(*testutil.MockDataStore)
	if ok {
		// Using mock datastore
		err = mockDS.RunAndRecord("myapp", "install", "echo 'done'", expectedSentinel)
	} else {
		// Using real datastore - would need to actually run command
		// For this test, we'll skip since we're testing status, not execution
		t.Skip("Cannot test with real datastore that executes commands")
	}
	require.NoError(t, err)

	// Now sentinel should exist
	status, err = handler.CheckStatus(file, checker)
	require.NoError(t, err)
	assert.Equal(t, operations.StatusStateReady, status.State)
	assert.Equal(t, "installed", status.Message)
}
