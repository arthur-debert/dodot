package types_test

import (
	"testing"

	"github.com/arthur-debert/dodot/pkg/testutil"
	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

// TestCheckStatus_MissingActionTypes tests status checking for action types that were missing coverage
func TestCheckStatus_MissingActionTypes(t *testing.T) {
	tests := []struct {
		name          string
		action        types.Action
		setupFS       func(types.FS, string)
		expectedState types.StatusState
		expectedMsg   string
	}{
		{
			name: "ActionTypeRun always returns pending",
			action: types.Action{
				Type:        types.ActionTypeRun,
				Command:     "echo",
				Args:        []string{"hello"},
				Description: "Run echo command",
			},
			expectedState: types.StatusStatePending,
			expectedMsg:   "⏵ script",
		},
		{
			name: "ActionTypeWrite file doesn't exist",
			action: types.Action{
				Type:    types.ActionTypeWrite,
				Target:  "home/user/newfile.txt",
				Content: "content",
			},
			expectedState: types.StatusStatePending,
			expectedMsg:   "will create file",
		},
		{
			name: "ActionTypeWrite file exists",
			action: types.Action{
				Type:    types.ActionTypeWrite,
				Target:  "home/user/existing.txt",
				Content: "content",
			},
			setupFS: func(fs types.FS, dataDir string) {
				testutil.CreateFileT(t, fs, "home/user/existing.txt", "content")
			},
			expectedState: types.StatusStateSuccess,
			expectedMsg:   "file created",
		},
		{
			name: "ActionTypeAppend target doesn't exist",
			action: types.Action{
				Type:    types.ActionTypeAppend,
				Target:  "home/user/file.txt",
				Content: "appended",
			},
			expectedState: types.StatusStatePending,
			expectedMsg:   "will append content",
		},
		{
			name: "ActionTypeAppend target exists",
			action: types.Action{
				Type:    types.ActionTypeAppend,
				Target:  "home/user/existing.txt",
				Content: "appended",
			},
			setupFS: func(fs types.FS, dataDir string) {
				testutil.CreateFileT(t, fs, "home/user/existing.txt", "original")
			},
			expectedState: types.StatusStateSuccess,
			expectedMsg:   "content appended",
		},
		{
			name: "ActionTypeCopy source doesn't exist",
			action: types.Action{
				Type:   types.ActionTypeCopy,
				Source: "home/user/source.txt",
				Target: "home/user/dest.txt",
			},
			expectedState: types.StatusStatePending,
			expectedMsg:   "unknown action type",
		},
		{
			name: "ActionTypeCopy target exists",
			action: types.Action{
				Type:   types.ActionTypeCopy,
				Source: "home/user/source.txt",
				Target: "home/user/dest.txt",
			},
			setupFS: func(fs types.FS, dataDir string) {
				testutil.CreateFileT(t, fs, "home/user/source.txt", "content")
				testutil.CreateFileT(t, fs, "home/user/dest.txt", "content")
			},
			expectedState: types.StatusStatePending,
			expectedMsg:   "unknown action type",
		},
		{
			name: "ActionTypeMkdir directory doesn't exist",
			action: types.Action{
				Type:   types.ActionTypeMkdir,
				Target: "home/user/newdir",
			},
			expectedState: types.StatusStatePending,
			expectedMsg:   "will create directory",
		},
		{
			name: "ActionTypeMkdir directory exists",
			action: types.Action{
				Type:   types.ActionTypeMkdir,
				Target: "home/user/existingdir",
			},
			setupFS: func(fs types.FS, dataDir string) {
				testutil.CreateDirT(t, fs, "home/user/existingdir")
			},
			expectedState: types.StatusStateSuccess,
			expectedMsg:   "directory created",
		},
		{
			name: "ActionTypeRead always returns pending",
			action: types.Action{
				Type:   types.ActionTypeRead,
				Source: "home/user/file.txt",
			},
			expectedState: types.StatusStatePending,
			expectedMsg:   "unknown action type",
		},
		{
			name: "ActionTypeChecksum always returns pending",
			action: types.Action{
				Type:   types.ActionTypeChecksum,
				Source: "home/user/file.txt",
			},
			expectedState: types.StatusStatePending,
			expectedMsg:   "unknown action type",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			// Setup
			fs := testutil.NewTestFS()
			dataDir := "data/dodot"
			testutil.CreateDirT(t, fs, dataDir)

			if tt.setupFS != nil {
				tt.setupFS(fs, dataDir)
			}

			mockPaths := &testutil.MockPaths{
				DataDirPath: dataDir,
			}

			// Execute
			status, err := tt.action.CheckStatus(fs, mockPaths)

			// Assert
			require.NoError(t, err)
			assert.Equal(t, tt.expectedState, status.State)
			assert.Equal(t, tt.expectedMsg, status.Message)
		})
	}
}
