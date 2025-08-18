package addignore

import (
	"os"
	"path/filepath"
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
			name: "ignore file already exists",
			setupFiles: map[string]string{
				"dotfiles/vim/vimrc": "set number",
				"dotfiles/vim/" + cfg.Patterns.SpecialFiles.IgnoreFile: "",
			},
			packName:          "vim",
			wantErr:           false,
			wantCreated:       false,
			wantAlreadyExists: true,
		},
		{
			name:       "pack does not exist",
			setupFiles: map[string]string{},
			packName:   "nonexistent",
			wantErr:    true,
			errCode:    errors.ErrPackNotFound,
		},
		{
			name: "empty pack name",
			setupFiles: map[string]string{
				"dotfiles/vim/vimrc": "set number",
			},
			packName: "",
			wantErr:  true,
			errCode:  errors.ErrInvalidInput,
		},
		{
			name: "pack name with slash",
			setupFiles: map[string]string{
				"dotfiles/vim/vimrc": "set number",
			},
			packName: "tools/nested",
			wantErr:  true,
			errCode:  errors.ErrInvalidInput,
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
			assert.Equal(t, tt.packName, result.PackName)
			assert.Equal(t, tt.wantCreated, result.Created)
			assert.Equal(t, tt.wantAlreadyExists, result.AlreadyExisted)

			// Verify ignore file path
			expectedPath := filepath.Join(root, "dotfiles", tt.packName, cfg.Patterns.SpecialFiles.IgnoreFile)
			assert.Equal(t, expectedPath, result.IgnoreFilePath)

			// Verify file exists on actual filesystem
			_, err = os.Stat(expectedPath)
			assert.NoError(t, err)
		})
	}
}
