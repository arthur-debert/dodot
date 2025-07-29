package core

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/arthur-debert/dodot/pkg/errors"
	"github.com/arthur-debert/dodot/pkg/testutil"
	"github.com/arthur-debert/dodot/pkg/types"
)

func TestInitPack(t *testing.T) {
	tests := []struct {
		name     string
		setup    func(t *testing.T) string
		packName string
		validate func(t *testing.T, result *types.InitResult, root string)
		wantErr  bool
		errCode  errors.ErrorCode
	}{
		{
			name: "init new pack",
			setup: func(t *testing.T) string {
				return testutil.TempDir(t, "init-test")
			},
			packName: "test-pack",
			validate: func(t *testing.T, result *types.InitResult, root string) {
				testutil.AssertEqual(t, "test-pack", result.PackName)
				testutil.AssertEqual(t, filepath.Join(root, "test-pack"), result.Path)
				
				// Should create 6 files: .dodot.toml, README.txt, and 4 template files
				testutil.AssertEqual(t, 6, len(result.FilesCreated))
				
				// Check that pack directory was created
				info, err := os.Stat(result.Path)
				testutil.AssertNoError(t, err)
				testutil.AssertTrue(t, info.IsDir())
				
				// Check that all expected files exist
				expectedFiles := []string{
					".dodot.toml", "README.txt", "aliases.sh", 
					"install.sh", "Brewfile", "path.sh",
				}
				for _, expected := range expectedFiles {
					filePath := filepath.Join(result.Path, expected)
					_, err := os.Stat(filePath)
					testutil.AssertNoError(t, err, "expected file %s to exist", expected)
				}
			},
		},
		{
			name: "init pack with existing directory",
			setup: func(t *testing.T) string {
				root := testutil.TempDir(t, "init-test")
				// Create existing pack
				testutil.CreateDir(t, root, "existing-pack")
				return root
			},
			packName: "existing-pack",
			wantErr:  true,
			errCode:  errors.ErrPackExists,
		},
		{
			name: "init pack with empty name",
			setup: func(t *testing.T) string {
				return testutil.TempDir(t, "init-test")
			},
			packName: "",
			wantErr:  true,
			errCode:  errors.ErrInvalidInput,
		},
		{
			name: "init pack with invalid characters",
			setup: func(t *testing.T) string {
				return testutil.TempDir(t, "init-test")
			},
			packName: "test/pack",
			wantErr:  true,
			errCode:  errors.ErrInvalidInput,
		},
		{
			name: "init pack with special characters",
			setup: func(t *testing.T) string {
				return testutil.TempDir(t, "init-test")
			},
			packName: "test*pack?",
			wantErr:  true,
			errCode:  errors.ErrInvalidInput,
		},
		{
			name: "init pack with spaces",
			setup: func(t *testing.T) string {
				return testutil.TempDir(t, "init-test")
			},
			packName: "my test pack",
			validate: func(t *testing.T, result *types.InitResult, root string) {
				// Spaces should be allowed
				testutil.AssertEqual(t, "my test pack", result.PackName)
				testutil.AssertEqual(t, 6, len(result.FilesCreated))
			},
		},
		{
			name: "init pack with hyphens and underscores",
			setup: func(t *testing.T) string {
				return testutil.TempDir(t, "init-test")
			},
			packName: "my-test_pack",
			validate: func(t *testing.T, result *types.InitResult, root string) {
				testutil.AssertEqual(t, "my-test_pack", result.PackName)
				testutil.AssertEqual(t, 6, len(result.FilesCreated))
			},
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
				if tt.errCode != "" {
					testutil.AssertTrue(t, errors.IsErrorCode(err, tt.errCode),
						"expected error code %s, got %s", tt.errCode, errors.GetErrorCode(err))
				}
				return
			}
			
			testutil.AssertNoError(t, err)
			testutil.AssertNotNil(t, result)
			
			if tt.validate != nil {
				tt.validate(t, result, root)
			}
		})
	}
}

func TestInitPackFileContents(t *testing.T) {
	// Test that generated files have appropriate content
	root := testutil.TempDir(t, "init-content-test")
	
	opts := InitPackOptions{
		DotfilesRoot: root,
		PackName:     "content-test-pack",
	}
	
	result, err := InitPack(opts)
	testutil.AssertNoError(t, err)
	
	packPath := result.Path
	
	// Check .dodot.toml content
	configContent, err := os.ReadFile(filepath.Join(packPath, ".dodot.toml"))
	testutil.AssertNoError(t, err)
	testutil.AssertContains(t, string(configContent), "# dodot configuration for content-test-pack pack")
	testutil.AssertContains(t, string(configContent), "# skip = true")
	testutil.AssertContains(t, string(configContent), "[files]")
	testutil.AssertContains(t, string(configContent), "# \"*.bak\" = \"ignore\"")
	
	// Check README.txt content
	readmeContent, err := os.ReadFile(filepath.Join(packPath, "README.txt"))
	testutil.AssertNoError(t, err)
	testutil.AssertContains(t, string(readmeContent), "dodot Pack: content-test-pack")
	testutil.AssertContains(t, string(readmeContent), "This pack was created by dodot init")
	testutil.AssertContains(t, string(readmeContent), "dodot deploy content-test-pack")
	
	// Check that template files were created by FillPack
	aliasesContent, err := os.ReadFile(filepath.Join(packPath, "aliases.sh"))
	testutil.AssertNoError(t, err)
	testutil.AssertContains(t, string(aliasesContent), "content-test-pack")
}

func TestInitPackCreatesValidPack(t *testing.T) {
	// Test that the created pack can be loaded by GetPacks
	root := testutil.TempDir(t, "init-valid-test")
	
	opts := InitPackOptions{
		DotfilesRoot: root,
		PackName:     "valid-pack",
	}
	
	result, err := InitPack(opts)
	testutil.AssertNoError(t, err)
	
	// Now try to load the pack
	candidates := []string{result.Path}
	packs, err := GetPacks(candidates)
	testutil.AssertNoError(t, err)
	testutil.AssertEqual(t, 1, len(packs))
	testutil.AssertEqual(t, "valid-pack", packs[0].Name)
	testutil.AssertEqual(t, result.Path, packs[0].Path)
}