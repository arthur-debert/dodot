package symlink_test

import (
	"path/filepath"
	"testing"

	"github.com/arthur-debert/dodot/pkg/handlers/lib/symlink"
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

func (m *MockStatusChecker) SetDataLink(packName, handlerName, relativePath string, exists bool) {
	key := packName + ":" + handlerName + ":" + relativePath
	m.hasDataLink[key] = exists
}

func TestSymlinkHandler_CheckStatus(t *testing.T) {
	tests := []struct {
		name          string
		file          operations.FileInput
		linkExists    bool
		dataLinkErr   error
		expectedState operations.StatusState
		expectedMsg   string
		expectError   bool
	}{
		{
			name: "link exists",
			file: operations.FileInput{
				PackName:     "vim",
				SourcePath:   "/dotfiles/vim/vimrc",
				RelativePath: "vimrc",
			},
			linkExists:    true,
			expectedState: operations.StatusStateReady,
			expectedMsg:   "linked to $HOME/vimrc",
			expectError:   false,
		},
		{
			name: "link does not exist",
			file: operations.FileInput{
				PackName:     "vim",
				SourcePath:   "/dotfiles/vim/vimrc",
				RelativePath: "vimrc",
			},
			linkExists:    false,
			expectedState: operations.StatusStatePending,
			expectedMsg:   "will be linked to $HOME/vimrc",
			expectError:   false,
		},
		{
			name: "nested file - link exists",
			file: operations.FileInput{
				PackName:     "vim",
				SourcePath:   "/dotfiles/vim/.config/nvim/init.lua",
				RelativePath: ".config/nvim/init.lua",
			},
			linkExists:    true,
			expectedState: operations.StatusStateReady,
			expectedMsg:   "linked to $HOME/init.lua",
			expectError:   false,
		},
		{
			name: "error checking link",
			file: operations.FileInput{
				PackName:     "vim",
				SourcePath:   "/dotfiles/vim/vimrc",
				RelativePath: "vimrc",
			},
			dataLinkErr:   assert.AnError,
			expectedState: operations.StatusStateError,
			expectedMsg:   "Failed to check link status: assert.AnError general error for testing",
			expectError:   true,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			// Create handler
			handler := symlink.NewHandler()

			// Create mock status checker
			checker := NewMockStatusChecker()
			if tt.dataLinkErr != nil {
				checker.dataLinkErr = tt.dataLinkErr
			} else {
				checker.SetDataLink(tt.file.PackName, handler.Name(), tt.file.RelativePath, tt.linkExists)
			}

			// Check status
			status, err := handler.CheckStatus(tt.file, checker)

			if tt.expectError {
				assert.Error(t, err)
			} else {
				assert.NoError(t, err)
			}

			assert.Equal(t, tt.expectedState, status.State)
			assert.Equal(t, tt.expectedMsg, status.Message)
		})
	}
}

func TestSymlinkHandler_CheckStatus_Integration(t *testing.T) {
	// This test uses the testutil environment to test with the actual datastore
	env := testutil.NewTestEnvironment(t, testutil.EnvMemoryOnly)
	defer env.Cleanup()

	// Set up a pack with a file
	env.SetupPack("vim", testutil.PackConfig{
		Files: map[string]string{
			"vimrc": "set number",
		},
	})

	// Create handler
	handler := symlink.NewHandler()

	// Create real status checker
	checker := operations.NewDataStoreStatusChecker(env.DataStore, env.FS, env.Paths)

	// Create file input
	file := operations.FileInput{
		PackName:     "vim",
		SourcePath:   env.DotfilesRoot + "/vim/vimrc",
		RelativePath: "vimrc",
	}

	// Initially, link should not exist
	status, err := handler.CheckStatus(file, checker)
	require.NoError(t, err)
	assert.Equal(t, operations.StatusStatePending, status.State)
	assert.Equal(t, "will be linked to $HOME/vimrc", status.Message)

	// Create the link in the datastore
	linkPath := filepath.Join(env.Paths.DataDir(), "packs", "vim", "symlink")
	targetPath := linkPath + "/vimrc"
	err = env.FS.MkdirAll(linkPath, 0755)
	require.NoError(t, err)
	err = env.FS.WriteFile(targetPath, []byte("linked"), 0644)
	require.NoError(t, err)

	// Debug: Check if file really exists
	exists, err := checker.HasDataLink("vim", "symlink", "vimrc")
	require.NoError(t, err)
	assert.True(t, exists, "Link should exist after creation")

	// Now link should exist
	status, err = handler.CheckStatus(file, checker)
	require.NoError(t, err)
	assert.Equal(t, operations.StatusStateReady, status.State)
	assert.Equal(t, "linked to $HOME/vimrc", status.Message)
}
