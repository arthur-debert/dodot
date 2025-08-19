package adopt

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/arthur-debert/dodot/pkg/errors"
	_ "github.com/arthur-debert/dodot/pkg/matchers" // register default matchers
	"github.com/arthur-debert/dodot/pkg/testutil"
	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestAdoptFiles(t *testing.T) {
	tests := []struct {
		name             string
		setupFiles       map[string]string
		packName         string
		sourcePaths      []string
		force            bool
		wantErr          bool
		errCode          errors.ErrorCode
		wantAdoptedCount int
		checkResults     func(t *testing.T, result *types.AdoptResult, root string)
	}{
		{
			name: "adopt single file from home",
			setupFiles: map[string]string{
				"home/.gitconfig": "user.name = Test",
			},
			packName:         "git",
			sourcePaths:      []string{"$HOME/.gitconfig"},
			wantErr:          false,
			wantAdoptedCount: 1,
			checkResults: func(t *testing.T, result *types.AdoptResult, root string) {
				// Check file was moved and symlinked
				adopted := result.AdoptedFiles[0]
				assert.Contains(t, adopted.NewPath, "git/gitconfig")

				// Verify symlink exists and points to new location
				target, err := os.Readlink(adopted.SymlinkPath)
				require.NoError(t, err)
				assert.Equal(t, adopted.NewPath, target)
			},
		},
		{
			name: "adopt multiple files",
			setupFiles: map[string]string{
				"home/.bashrc":       "# bashrc",
				"home/.bash_profile": "# profile",
			},
			packName:         "shell",
			sourcePaths:      []string{"$HOME/.bashrc", "$HOME/.bash_profile"},
			wantErr:          false,
			wantAdoptedCount: 2,
		},
		{
			name: "adopt file from XDG config",
			setupFiles: map[string]string{
				"home/.config/starship/starship.toml": "format = \"$all$character\"",
			},
			packName:         "starship",
			sourcePaths:      []string{"$HOME/.config/starship/starship.toml"},
			wantErr:          false,
			wantAdoptedCount: 1,
			checkResults: func(t *testing.T, result *types.AdoptResult, root string) {
				// Check XDG structure is preserved
				adopted := result.AdoptedFiles[0]
				assert.Contains(t, adopted.NewPath, "starship/starship/starship.toml")
			},
		},
		{
			name:        "adopt non-existent file",
			setupFiles:  map[string]string{},
			packName:    "test",
			sourcePaths: []string{"$HOME/.nonexistent"},
			wantErr:     true,
		},
		{
			name: "adopt to non-existent pack creates it",
			setupFiles: map[string]string{
				"home/.vimrc": "set number",
			},
			packName:         "newpack",
			sourcePaths:      []string{"$HOME/.vimrc"},
			wantErr:          false,
			wantAdoptedCount: 1,
			checkResults: func(t *testing.T, result *types.AdoptResult, root string) {
				// Verify pack was created
				packPath := filepath.Join(root, "dotfiles", "newpack")
				info, err := os.Stat(packPath)
				require.NoError(t, err)
				assert.True(t, info.IsDir())
			},
		},
		{
			name: "destination already exists without force",
			setupFiles: map[string]string{
				"home/.gitconfig":        "new content",
				"dotfiles/git/gitconfig": "old content",
			},
			packName:    "git",
			sourcePaths: []string{"$HOME/.gitconfig"},
			force:       false,
			wantErr:     true,
		},
		{
			name: "destination already exists with force",
			setupFiles: map[string]string{
				"home/.gitconfig":        "new content",
				"dotfiles/git/gitconfig": "old content",
			},
			packName:         "git",
			sourcePaths:      []string{"$HOME/.gitconfig"},
			force:            true,
			wantErr:          false,
			wantAdoptedCount: 1,
		},
		{
			name: "empty pack name",
			setupFiles: map[string]string{
				"home/.gitconfig": "content",
			},
			packName:    "",
			sourcePaths: []string{"$HOME/.gitconfig"},
			wantErr:     true,
			errCode:     errors.ErrPackNotFound,
		},
		{
			name: "pack name with trailing slash",
			setupFiles: map[string]string{
				"home/.gitconfig": "content",
			},
			packName:         "git/",
			sourcePaths:      []string{"$HOME/.gitconfig"},
			wantErr:          false,
			wantAdoptedCount: 1,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			// Setup test filesystem
			root := testutil.TempDir(t, "adopt-test")
			dotfilesPath := filepath.Join(root, "dotfiles")
			homePath := filepath.Join(root, "home")

			// Create dotfiles directory
			require.NoError(t, os.MkdirAll(dotfilesPath, 0755))

			// Set HOME to test directory
			oldHome := os.Getenv("HOME")
			require.NoError(t, os.Setenv("HOME", homePath))
			defer func() { _ = os.Setenv("HOME", oldHome) }()

			// Set XDG_CONFIG_HOME
			require.NoError(t, os.Setenv("XDG_CONFIG_HOME", filepath.Join(homePath, ".config")))
			defer func() { _ = os.Unsetenv("XDG_CONFIG_HOME") }()

			// Create files from setupFiles map
			for path, content := range tt.setupFiles {
				fullPath := filepath.Join(root, path)
				testutil.CreateFile(t, filepath.Dir(fullPath), filepath.Base(fullPath), content)
			}

			// Expand source paths
			expandedPaths := make([]string, len(tt.sourcePaths))
			for i, path := range tt.sourcePaths {
				expandedPaths[i] = os.ExpandEnv(path)
			}

			// Run AdoptFiles
			result, err := AdoptFiles(AdoptFilesOptions{
				DotfilesRoot: dotfilesPath,
				PackName:     tt.packName,
				SourcePaths:  expandedPaths,
				Force:        tt.force,
			})

			// Check error
			if tt.wantErr {
				require.Error(t, err)
				if tt.errCode != "" {
					var dodotErr *errors.DodotError
					require.ErrorAs(t, err, &dodotErr)
					assert.Equal(t, tt.errCode, dodotErr.Code)
				}
				return
			}

			require.NoError(t, err)
			require.NotNil(t, result)

			// Verify result
			assert.Equal(t, tt.wantAdoptedCount, len(result.AdoptedFiles))

			// Run custom checks
			if tt.checkResults != nil {
				tt.checkResults(t, result, root)
			}
		})
	}
}

func TestAdoptIdempotency(t *testing.T) {
	// Setup test filesystem
	root := testutil.TempDir(t, "adopt-idempotent-test")
	dotfilesPath := filepath.Join(root, "dotfiles")
	homePath := filepath.Join(root, "home")

	// Create dotfiles directory
	require.NoError(t, os.MkdirAll(dotfilesPath, 0755))

	// Set HOME to test directory
	oldHome := os.Getenv("HOME")
	require.NoError(t, os.Setenv("HOME", homePath))
	defer func() { _ = os.Setenv("HOME", oldHome) }()

	// Create a file
	testutil.CreateFile(t, homePath, ".gitconfig", "user.name = Test")

	// First adoption
	result1, err := AdoptFiles(AdoptFilesOptions{
		DotfilesRoot: dotfilesPath,
		PackName:     "git",
		SourcePaths:  []string{filepath.Join(homePath, ".gitconfig")},
		Force:        false,
	})
	require.NoError(t, err)
	assert.Equal(t, 1, len(result1.AdoptedFiles))

	// Second adoption (should be idempotent - no files adopted)
	result2, err := AdoptFiles(AdoptFilesOptions{
		DotfilesRoot: dotfilesPath,
		PackName:     "git",
		SourcePaths:  []string{filepath.Join(homePath, ".gitconfig")},
		Force:        false,
	})
	require.NoError(t, err)
	assert.Equal(t, 0, len(result2.AdoptedFiles))
}

func TestDetermineDestinationPath(t *testing.T) {
	oldHome := os.Getenv("HOME")
	testHome := "/home/testuser"
	require.NoError(t, os.Setenv("HOME", testHome))
	defer func() { _ = os.Setenv("HOME", oldHome) }()

	require.NoError(t, os.Setenv("XDG_CONFIG_HOME", filepath.Join(testHome, ".config")))
	defer func() { _ = os.Unsetenv("XDG_CONFIG_HOME") }()

	tests := []struct {
		name       string
		packPath   string
		sourcePath string
		want       string
	}{
		{
			name:       "file in home directory",
			packPath:   "/dotfiles/git",
			sourcePath: filepath.Join(testHome, ".gitconfig"),
			want:       "/dotfiles/git/gitconfig",
		},
		{
			name:       "file in XDG config",
			packPath:   "/dotfiles/starship",
			sourcePath: filepath.Join(testHome, ".config/starship/starship.toml"),
			want:       "/dotfiles/starship/starship/starship.toml",
		},
		{
			name:       "nested hidden directory",
			packPath:   "/dotfiles/app",
			sourcePath: "/opt/.myapp/config/settings.json",
			want:       "/dotfiles/app/config/settings.json",
		},
		{
			name:       "file without leading dot",
			packPath:   "/dotfiles/misc",
			sourcePath: filepath.Join(testHome, "myconfig"),
			want:       "/dotfiles/misc/myconfig",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got := determineDestinationPath(tt.packPath, tt.sourcePath)
			assert.Equal(t, tt.want, got)
		})
	}
}
