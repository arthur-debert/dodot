package addignore

import (
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/arthur-debert/dodot/pkg/config"
	"github.com/arthur-debert/dodot/pkg/errors"
	_ "github.com/arthur-debert/dodot/pkg/matchers" // register default matchers
	"github.com/arthur-debert/dodot/pkg/testutil"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestAddIgnore(t *testing.T) {
	cfg := config.Default()

	tests := []struct {
		name              string
		setupFiles        map[string]string
		packName          string
		wantErr           bool
		errCode           errors.ErrorCode
		wantCreated       bool
		wantAlreadyExists bool
	}{
		{
			name: "successfully create ignore file",
			setupFiles: map[string]string{
				"dotfiles/vim/vimrc": "set number",
			},
			packName:          "vim",
			wantErr:           false,
			wantCreated:       true,
			wantAlreadyExists: false,
		},
		{
			name:       "pack does not exist",
			setupFiles: map[string]string{},
			packName:   "nonexistent",
			wantErr:    true,
			errCode:    errors.ErrNotFound,
		},
		{
			name: "pack name with trailing slash",
			setupFiles: map[string]string{
				"dotfiles/vim/vimrc": "set number",
			},
			packName:          "vim/",
			wantErr:           false,
			wantCreated:       true,
			wantAlreadyExists: false,
		},
		{
			name: "empty pack name",
			setupFiles: map[string]string{
				"dotfiles/vim/vimrc": "set number",
			},
			packName: "",
			wantErr:  true,
			errCode:  errors.ErrPackNotFound,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			// Setup test filesystem
			root := testutil.TempDir(t, "addignore-test")
			dotfilesPath := filepath.Join(root, "dotfiles")

			// Create files from setupFiles map
			for path, content := range tt.setupFiles {
				fullPath := filepath.Join(root, path)
				testutil.CreateFile(t, filepath.Dir(fullPath), filepath.Base(fullPath), content)
			}

			// Run AddIgnore
			result, err := AddIgnore(AddIgnoreOptions{
				DotfilesRoot: dotfilesPath,
				PackName:     tt.packName,
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
			// Pack name should be normalized (no trailing slash)
			expectedPackName := strings.TrimRight(tt.packName, "/")
			assert.Equal(t, expectedPackName, result.PackName)
			assert.Equal(t, tt.wantCreated, result.Created)
			assert.Equal(t, tt.wantAlreadyExists, result.AlreadyExisted)

			// Verify ignore file path
			expectedPath := filepath.Join(root, "dotfiles", expectedPackName, cfg.Patterns.SpecialFiles.IgnoreFile)
			assert.Equal(t, expectedPath, result.IgnoreFilePath)

			// Verify file exists on actual filesystem
			_, err = os.Stat(expectedPath)
			assert.NoError(t, err)
		})
	}
}

func TestAddIgnoreAlreadyExists(t *testing.T) {
	cfg := config.Default()

	// Setup test filesystem
	root := testutil.TempDir(t, "addignore-test")
	dotfilesPath := filepath.Join(root, "dotfiles")

	// Create pack
	packPath := filepath.Join(dotfilesPath, "vim")
	testutil.CreateFile(t, packPath, "vimrc", "set number")

	// First, create the ignore file
	result1, err := AddIgnore(AddIgnoreOptions{
		DotfilesRoot: dotfilesPath,
		PackName:     "vim",
	})
	require.NoError(t, err)
	require.NotNil(t, result1)
	assert.True(t, result1.Created)
	assert.False(t, result1.AlreadyExisted)

	// Try to create it again - now it should work even though pack is ignored
	result2, err := AddIgnore(AddIgnoreOptions{
		DotfilesRoot: dotfilesPath,
		PackName:     "vim",
	})
	require.NoError(t, err)
	require.NotNil(t, result2)
	assert.False(t, result2.Created)
	assert.True(t, result2.AlreadyExisted)

	// Verify paths are the same
	assert.Equal(t, result1.IgnoreFilePath, result2.IgnoreFilePath)

	// Verify file exists
	expectedPath := filepath.Join(packPath, cfg.Patterns.SpecialFiles.IgnoreFile)
	_, err = os.Stat(expectedPath)
	assert.NoError(t, err)
}
