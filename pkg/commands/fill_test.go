package commands

import (
	"path/filepath"
	"testing"

	"github.com/arthur-debert/dodot/pkg/testutil"
	"github.com/arthur-debert/dodot/pkg/types"
)

func TestFillPack(t *testing.T) {
	tests := []struct {
		name     string
		setup    func(t *testing.T) string
		packName string
		validate func(t *testing.T, result *types.FillResult, packPath string)
		wantErr  bool
	}{
		{
			name: "fill an empty pack",
			setup: func(t *testing.T) string {
				root := testutil.TempDir(t, "fill-test")
				testutil.CreateDir(t, root, "empty-pack")
				return root
			},
			packName: "empty-pack",
			validate: func(t *testing.T, result *types.FillResult, packPath string) {
				// Debug: print files created
				t.Logf("Files created: %v", result.FilesCreated)

				// Since we're not executing operations yet, we only check the reported files
				testutil.AssertEqual(t, 3, len(result.FilesCreated))

				// Check that all expected files are in the result
				expectedFiles := map[string]bool{
					"aliases.sh": false,
					"install.sh": false,
					"Brewfile":   false,
				}

				for _, file := range result.FilesCreated {
					if _, ok := expectedFiles[file]; ok {
						expectedFiles[file] = true
					}
				}

				for file, found := range expectedFiles {
					testutil.AssertTrue(t, found, "Expected file %s not in FilesCreated", file)
				}
			},
		},
		{
			name: "fill a partially filled pack",
			setup: func(t *testing.T) string {
				root := testutil.TempDir(t, "fill-test")
				pack := testutil.CreateDir(t, root, "partial-pack")
				testutil.CreateFile(t, pack, "aliases.sh", "# My aliases")
				return root
			},
			packName: "partial-pack",
			validate: func(t *testing.T, result *types.FillResult, packPath string) {
				// Since aliases.sh already exists, only 2 files should be created
				testutil.AssertEqual(t, 2, len(result.FilesCreated))

				// Check that existing file was not overwritten
				content := testutil.ReadFile(t, filepath.Join(packPath, "aliases.sh"))
				testutil.AssertEqual(t, "# My aliases", content)

				// Check that only missing files are in the result
				expectedFiles := map[string]bool{
					"install.sh": false,
					"Brewfile":   false,
				}

				for _, file := range result.FilesCreated {
					if _, ok := expectedFiles[file]; ok {
						expectedFiles[file] = true
					}
				}

				for file, found := range expectedFiles {
					testutil.AssertTrue(t, found, "Expected file %s not in FilesCreated", file)
				}
			},
		},
		{
			name: "non-existent pack",
			setup: func(t *testing.T) string {
				return testutil.TempDir(t, "fill-test")
			},
			packName: "fake-pack",
			wantErr:  true,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			root := tt.setup(t)

			opts := FillPackOptions{
				DotfilesRoot: root,
				PackName:     tt.packName,
			}

			result, err := FillPack(opts)

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
