package state

import (
	"os"
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

func TestRemoveDanglingLink(t *testing.T) {
	tests := []struct {
		name           string
		setup          func(t *testing.T, fs types.FS, action types.Action, intermediatePath string, dl *DanglingLink)
		expectedRemove bool
		expectedError  bool
	}{
		{
			name: "removes dangling symlink with missing source",
			setup: func(t *testing.T, fs types.FS, action types.Action, intermediatePath string, dl *DanglingLink) {
				// Remove the source file to make it dangling
				err := fs.Remove(action.Source)
				require.NoError(t, err)
			},
			expectedRemove: true,
		},
		{
			name: "removes dangling symlink with missing intermediate",
			setup: func(t *testing.T, fs types.FS, action types.Action, intermediatePath string, dl *DanglingLink) {
				// Remove the intermediate symlink
				err := fs.Remove(intermediatePath)
				require.NoError(t, err)
			},
			expectedRemove: true,
		},
		{
			name: "does not remove if deployed symlink points elsewhere",
			setup: func(t *testing.T, fs types.FS, action types.Action, intermediatePath string, dl *DanglingLink) {
				// Make deployed symlink point to a different file
				otherFile := filepath.Join("home", "other-file")
				testutil.CreateFileT(t, fs, otherFile, "other content")

				// Re-create deployed symlink pointing elsewhere
				err := fs.Remove(action.Target)
				require.NoError(t, err)
				testutil.CreateSymlinkT(t, fs, otherFile, action.Target)
			},
			expectedRemove: false,
		},
		{
			name: "handles already removed deployed symlink gracefully",
			setup: func(t *testing.T, fs types.FS, action types.Action, intermediatePath string, dl *DanglingLink) {
				// Remove the deployed symlink before removal attempt
				err := fs.Remove(action.Target)
				require.NoError(t, err)
			},
			expectedRemove: false, // Already gone, nothing to remove
		},
		{
			name: "removes both deployed and intermediate symlinks",
			setup: func(t *testing.T, fs types.FS, action types.Action, intermediatePath string, dl *DanglingLink) {
				// Remove source to make it dangling
				err := fs.Remove(action.Source)
				require.NoError(t, err)
			},
			expectedRemove: true,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			// Setup
			fs := testutil.NewTestFS()
			dataDir := filepath.Join("data", "dodot")
			dotfilesRoot := filepath.Join("dotfiles")

			// Create a working deployment first
			action, intermediatePath := setupWorkingDeployment(t, fs, dataDir, dotfilesRoot, "vim", "vimrc", ".vimrc")

			// Create DanglingLink object
			dl := &DanglingLink{
				DeployedPath:     action.Target,
				IntermediatePath: intermediatePath,
				SourcePath:       action.Source,
				Pack:             action.Pack,
				Problem:          "test problem",
			}

			// Apply test-specific setup
			tt.setup(t, fs, action, intermediatePath, dl)

			// Test removal
			pather := &testutil.MockPaths{
				DataDirPath:      dataDir,
				DotfilesRootPath: dotfilesRoot,
			}
			detector := NewLinkDetector(fs, pather)
			err := detector.RemoveDanglingLink(dl)

			// Verify error expectation
			if tt.expectedError {
				assert.Error(t, err)
			} else {
				assert.NoError(t, err)
			}

			// Verify removal
			if tt.expectedRemove {
				// Deployed symlink should be gone
				_, err := fs.Lstat(action.Target)
				if err == nil {
					t.Errorf("deployed symlink still exists at %s", action.Target)
				}

				// If intermediate still existed before removal, it should be gone too
				_, err = fs.Lstat(intermediatePath)
				if err == nil {
					t.Errorf("intermediate symlink still exists at %s", intermediatePath)
				}
			} else {
				// Check if deployed symlink still exists (might have been removed in setup)
				if _, err := fs.Lstat(action.Target); err == nil {
					// It exists, verify it's not pointing to our intermediate
					target, err := fs.Readlink(action.Target)
					require.NoError(t, err)
					assert.NotEqual(t, intermediatePath, target, "deployed symlink should not point to our intermediate")
				}
			}
		})
	}
}

func TestRemoveDanglingLinkSafety(t *testing.T) {
	// This test ensures we never remove user-created symlinks
	fs := testutil.NewTestFS()
	dataDir := filepath.Join("data", "dodot")
	dotfilesRoot := filepath.Join("dotfiles")

	// Create a user's own symlink (not managed by dodot)
	userConfig := filepath.Join("home", "my-config")
	userSymlink := filepath.Join("home", ".myconfig")
	testutil.CreateFileT(t, fs, userConfig, "user's config")
	testutil.CreateSymlinkT(t, fs, userConfig, userSymlink)

	// Create a DanglingLink that refers to the user's symlink
	dl := &DanglingLink{
		DeployedPath:     userSymlink,
		IntermediatePath: filepath.Join(dataDir, "deployed", "symlink", ".myconfig"),
		SourcePath:       filepath.Join(dotfilesRoot, "mypack", "myconfig"),
		Pack:             "mypack",
		Problem:          "source missing",
	}

	// Try to remove it
	pather := &testutil.MockPaths{
		DataDirPath:      dataDir,
		DotfilesRootPath: dotfilesRoot,
	}
	detector := NewLinkDetector(fs, pather)
	err := detector.RemoveDanglingLink(dl)
	assert.NoError(t, err)

	// User's symlink should still exist
	info, err := fs.Lstat(userSymlink)
	require.NoError(t, err)
	assert.True(t, info.Mode()&os.ModeSymlink != 0, "user's symlink should still exist")

	// And it should still point to user's config
	target, err := fs.Readlink(userSymlink)
	require.NoError(t, err)
	assert.Equal(t, userConfig, target)
}

func TestRemoveMultipleDanglingLinks(t *testing.T) {
	// Setup
	fs := testutil.NewTestFS()
	dataDir := filepath.Join("data", "dodot")
	dotfilesRoot := filepath.Join("dotfiles")

	// Create multiple working deployments
	action1, intermediatePath1 := setupWorkingDeployment(t, fs, dataDir, dotfilesRoot, "vim", "vimrc", ".vimrc")
	action2, intermediatePath2 := setupWorkingDeployment(t, fs, dataDir, dotfilesRoot, "git", "gitconfig", ".gitconfig")
	action3, _ := setupWorkingDeployment(t, fs, dataDir, dotfilesRoot, "zsh", "zshrc", ".zshrc")

	// Break first two by removing source files
	err := fs.Remove(action1.Source)
	require.NoError(t, err)
	err = fs.Remove(action2.Source)
	require.NoError(t, err)

	// Detect dangling links
	pather := &testutil.MockPaths{
		DataDirPath:      dataDir,
		DotfilesRootPath: dotfilesRoot,
	}
	detector := NewLinkDetector(fs, pather)
	dangling, err := detector.DetectDanglingLinks([]types.Action{action1, action2, action3})
	require.NoError(t, err)
	assert.Len(t, dangling, 2, "should find 2 dangling links")

	// Remove all dangling links
	for _, dl := range dangling {
		err := detector.RemoveDanglingLink(&dl)
		assert.NoError(t, err)
	}

	// Verify removals
	_, err = fs.Lstat(action1.Target)
	if err == nil {
		t.Errorf("first deployed symlink still exists at %s", action1.Target)
	}
	_, err = fs.Lstat(intermediatePath1)
	if err == nil {
		t.Errorf("first intermediate still exists at %s", intermediatePath1)
	}

	_, err = fs.Lstat(action2.Target)
	if err == nil {
		t.Errorf("second deployed symlink still exists at %s", action2.Target)
	}
	_, err = fs.Lstat(intermediatePath2)
	if err == nil {
		t.Errorf("second intermediate still exists at %s", intermediatePath2)
	}

	// Third deployment should still exist
	info, err := fs.Lstat(action3.Target)
	require.NoError(t, err)
	assert.True(t, info.Mode()&os.ModeSymlink != 0, "third deployed symlink should still exist")
}
