package core

import (
	"path/filepath"
	"testing"

	"github.com/arthur-debert/dodot/pkg/testutil"
	"github.com/arthur-debert/dodot/pkg/types"
)

func TestInitPack(t *testing.T) {
	tests := []struct {
		name     string
		setup    func(t *testing.T) string
		packName string
		validate func(t *testing.T, result *types.InitResult, packPath string)
		wantErr  bool
	}{
		{
			name: "create a new pack",
			setup: func(t *testing.T) string {
				return testutil.TempDir(t, "init-test")
			},
			packName: "new-pack",
			validate: func(t *testing.T, result *types.InitResult, packPath string) {
				testutil.AssertEqual(t, 6, len(result.FilesCreated))
				testutil.AssertDirExists(t, packPath)

				// Check that all files were created
				for _, f := range []string{".dodot.toml", "README.txt", "aliases.sh", "install.sh", "Brewfile", "path.sh"} {
					testutil.AssertFileExists(t, filepath.Join(packPath, f))
				}
			},
		},
		{
			name: "pack already exists",
			setup: func(t *testing.T) string {
				root := testutil.TempDir(t, "init-test")
				testutil.CreateDir(t, root, "existing-pack")
				return root
			},
			packName: "existing-pack",
			wantErr:  true,
		},
		{
			name: "invalid pack name",
			setup: func(t *testing.T) string {
				return testutil.TempDir(t, "init-test")
			},
			packName: "invalid/pack",
			wantErr:  true,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			root := tt.setup(t)

			opts := InitPackOptions{
				DotfilesRoot: root,
				PackName:     tt.packName,
			}

			result, err := InitPack(opts)

			if tt.wantErr {
				testutil.AssertError(t, err)
				return
			}

			testutil.AssertNoError(t, err)
			testutil.AssertNotNil(t, result)

			if tt.validate != nil {
				packPath := filepath.Join(root, tt.packName)
				tt.validate(t, result, packPath)
			}
		})
	}
}