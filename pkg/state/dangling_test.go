package state

import (
	"path/filepath"
	"testing"

	"github.com/arthur-debert/dodot/pkg/testutil"
	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

// setupWorkingDeployment creates a complete, working symlink deployment
func setupWorkingDeployment(t *testing.T, fs types.FS, dataDir, dotfilesRoot, pack, sourceFile, targetFile string) (types.Action, string) {
	t.Helper()

	// Create the action
	sourcePath := filepath.Join(dotfilesRoot, pack, sourceFile)
	targetPath := filepath.Join("home", targetFile)

	action := types.Action{
		Type:   types.ActionTypeLink,
		Source: sourcePath,
		Target: targetPath,
		Pack:   pack,
	}

	// Get intermediate path using the API
	pather := &testutil.MockPaths{
		DataDirPath:      dataDir,
		DotfilesRootPath: dotfilesRoot,
	}
	intermediatePath, err := action.GetDeployedSymlinkPath(pather)
	require.NoError(t, err)

	// Create a complete working deployment:
	// 1. Source file exists
	testutil.CreateFileT(t, fs, sourcePath, "file content")

	// 2. Intermediate symlink points to source
	testutil.CreateDirT(t, fs, filepath.Dir(intermediatePath))
	testutil.CreateSymlinkT(t, fs, sourcePath, intermediatePath)

	// 3. Deployed symlink points to intermediate
	testutil.CreateSymlinkT(t, fs, intermediatePath, targetPath)

	return action, intermediatePath
}

func TestDetectDanglingLinks(t *testing.T) {
	tests := []struct {
		name     string
		breakage func(t *testing.T, fs types.FS, action types.Action, intermediatePath string)
		expected []DanglingLink
	}{
		{
			name: "no dangling links - all healthy",
			breakage: func(t *testing.T, fs types.FS, action types.Action, intermediatePath string) {
				// Don't break anything - deployment should be healthy
			},
			expected: []DanglingLink{},
		},
		{
			name: "source file missing",
			breakage: func(t *testing.T, fs types.FS, action types.Action, intermediatePath string) {
				// Remove the source file to create a dangling link
				err := fs.Remove(action.Source)
				require.NoError(t, err)
			},
			expected: []DanglingLink{
				{
					Problem: "source file missing",
					Pack:    "vim",
				},
			},
		},
		{
			name: "intermediate symlink missing",
			breakage: func(t *testing.T, fs types.FS, action types.Action, intermediatePath string) {
				// Remove the intermediate symlink
				err := fs.Remove(intermediatePath)
				require.NoError(t, err)
			},
			expected: []DanglingLink{
				{
					Problem: "intermediate symlink missing",
					Pack:    "vim",
				},
			},
		},
		{
			name: "intermediate is not a symlink",
			breakage: func(t *testing.T, fs types.FS, action types.Action, intermediatePath string) {
				// Replace intermediate symlink with a regular file
				err := fs.Remove(intermediatePath)
				require.NoError(t, err)
				testutil.CreateFileT(t, fs, intermediatePath, "not a symlink")
			},
			expected: []DanglingLink{
				{
					Problem: "intermediate is not a symlink",
					Pack:    "vim",
				},
			},
		},
		{
			name: "intermediate points to wrong file",
			breakage: func(t *testing.T, fs types.FS, action types.Action, intermediatePath string) {
				// Create a wrong file and make intermediate point to it
				wrongPath := filepath.Join(filepath.Dir(action.Source), "wrong")
				testutil.CreateFileT(t, fs, wrongPath, "wrong file")

				// Re-create intermediate pointing to wrong file
				err := fs.Remove(intermediatePath)
				require.NoError(t, err)
				testutil.CreateSymlinkT(t, fs, wrongPath, intermediatePath)
			},
			expected: []DanglingLink{
				{
					Problem: "intermediate points to wrong file",
					Pack:    "vim",
				},
			},
		},
		{
			name: "deployed symlink points elsewhere - not managed by us",
			breakage: func(t *testing.T, fs types.FS, action types.Action, intermediatePath string) {
				// Make deployed symlink point to a different file
				otherFile := filepath.Join("home", "other-vimrc")
				testutil.CreateFileT(t, fs, otherFile, "other config")

				// Re-create deployed symlink pointing elsewhere
				err := fs.Remove(action.Target)
				require.NoError(t, err)
				testutil.CreateSymlinkT(t, fs, otherFile, action.Target)
			},
			expected: []DanglingLink{}, // Not managed by us, so not dangling
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			// Setup
			fs := testutil.NewTestFS()
			dataDir := filepath.Join("data", "dodot")
			dotfilesRoot := filepath.Join("dotfiles")

			// Create a working deployment
			action, intermediatePath := setupWorkingDeployment(t, fs, dataDir, dotfilesRoot, "vim", "vimrc", ".vimrc")

			// Break it according to test case
			tt.breakage(t, fs, action, intermediatePath)

			// Test detection
			pather := &testutil.MockPaths{
				DataDirPath:      dataDir,
				DotfilesRootPath: dotfilesRoot,
			}
			detector := NewLinkDetector(fs, pather)
			dangling, err := detector.DetectDanglingLinks([]types.Action{action})
			require.NoError(t, err)

			// Verify
			assert.Len(t, dangling, len(tt.expected))
			for i, expected := range tt.expected {
				if i < len(dangling) {
					assert.Equal(t, expected.Problem, dangling[i].Problem)
					assert.Equal(t, expected.Pack, dangling[i].Pack)
				}
			}
		})
	}
}

func TestDetectMultipleDanglingLinks(t *testing.T) {
	// Setup
	fs := testutil.NewTestFS()
	dataDir := filepath.Join("data", "dodot")
	dotfilesRoot := filepath.Join("dotfiles")

	// Create two working deployments
	action1, _ := setupWorkingDeployment(t, fs, dataDir, dotfilesRoot, "vim", "vimrc", ".vimrc")
	action2, intermediatePath2 := setupWorkingDeployment(t, fs, dataDir, dotfilesRoot, "git", "gitconfig", ".gitconfig")

	// Break them in different ways
	// 1. Remove source file for vim
	err := fs.Remove(action1.Source)
	require.NoError(t, err)

	// 2. Remove intermediate symlink for git
	err = fs.Remove(intermediatePath2)
	require.NoError(t, err)

	// Test detection
	pather := &testutil.MockPaths{
		DataDirPath:      dataDir,
		DotfilesRootPath: dotfilesRoot,
	}
	detector := NewLinkDetector(fs, pather)
	dangling, err := detector.DetectDanglingLinks([]types.Action{action1, action2})
	require.NoError(t, err)

	// Should find both issues
	assert.Len(t, dangling, 2)

	// Check specific problems (order might vary)
	problems := map[string]string{}
	for _, d := range dangling {
		problems[d.Pack] = d.Problem
	}

	assert.Equal(t, "source file missing", problems["vim"])
	assert.Equal(t, "intermediate symlink missing", problems["git"])
}

// TestRemoveDanglingLink will be added in a separate commit for Release 1
