package status

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/arthur-debert/dodot/pkg/datastore"
	"github.com/arthur-debert/dodot/pkg/filesystem"
	"github.com/arthur-debert/dodot/pkg/internal/hashutil"
	"github.com/arthur-debert/dodot/pkg/paths"
	"github.com/arthur-debert/dodot/pkg/testutil_old"
	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestStatusPacks(t *testing.T) {
	tests := []struct {
		name          string
		setupFS       func(fs types.FS, rootDir string)
		packNames     []string
		wantPackCount int
		wantErr       bool
		checkResult   func(t *testing.T, result *types.DisplayResult)
	}{
		{
			name: "status of all packs",
			setupFS: func(fs types.FS, rootDir string) {
				// Create some test packs
				testutil.CreateDirT(t, fs, rootDir+"/vim")
				testutil.CreateFileT(t, fs, rootDir+"/vim/.vimrc", "vim config")

				testutil.CreateDirT(t, fs, rootDir+"/zsh")
				testutil.CreateFileT(t, fs, rootDir+"/zsh/.zshrc", "zsh config")
			},
			packNames:     []string{}, // Empty means all packs
			wantPackCount: 2,
			checkResult: func(t *testing.T, result *types.DisplayResult) {
				assert.Equal(t, "status", result.Command)
				assert.False(t, result.DryRun)
				assert.Len(t, result.Packs, 2)

				// Check pack names
				packNames := make(map[string]bool)
				for _, pack := range result.Packs {
					packNames[pack.Name] = true
				}
				assert.True(t, packNames["vim"])
				assert.True(t, packNames["zsh"])

				// Each pack should have files listed
				for _, pack := range result.Packs {
					// Should have at least one file
					assert.NotEmpty(t, pack.Files)
				}
			},
		},
		{
			name: "status of specific pack",
			setupFS: func(fs types.FS, rootDir string) {
				// Create test packs
				testutil.CreateDirT(t, fs, rootDir+"/vim")
				testutil.CreateFileT(t, fs, rootDir+"/vim/.vimrc", "vim config")

				testutil.CreateDirT(t, fs, rootDir+"/zsh")
				testutil.CreateFileT(t, fs, rootDir+"/zsh/.zshrc", "zsh config")
			},
			packNames:     []string{"vim"},
			wantPackCount: 1,
			checkResult: func(t *testing.T, result *types.DisplayResult) {
				assert.Len(t, result.Packs, 1)
				assert.Equal(t, "vim", result.Packs[0].Name)
			},
		},
		{
			name: "status with ignored pack",
			setupFS: func(fs types.FS, rootDir string) {
				// Create normal pack
				testutil.CreateDirT(t, fs, rootDir+"/vim")
				testutil.CreateFileT(t, fs, rootDir+"/vim/.vimrc", "vim config")

				// Create ignored pack
				testutil.CreateDirT(t, fs, rootDir+"/temp")
				testutil.CreateFileT(t, fs, rootDir+"/temp/.dodotignore", "")
			},
			packNames:     []string{},
			wantPackCount: 2,
			checkResult: func(t *testing.T, result *types.DisplayResult) {
				assert.Len(t, result.Packs, 2)

				// Find the ignored pack
				var ignoredPack *types.DisplayPack
				for i := range result.Packs {
					if result.Packs[i].Name == "temp" {
						ignoredPack = &result.Packs[i]
						break
					}
				}

				require.NotNil(t, ignoredPack)
				assert.True(t, ignoredPack.IsIgnored)
				assert.Equal(t, "ignored", ignoredPack.Status)
			},
		},
		{
			name: "status with pack config",
			setupFS: func(fs types.FS, rootDir string) {
				// Create pack with config
				testutil.CreateDirT(t, fs, rootDir+"/configured")
				testutil.CreateFileT(t, fs, rootDir+"/configured/.dodot.toml", "")
				testutil.CreateFileT(t, fs, rootDir+"/configured/file.txt", "content")
			},
			packNames:     []string{"configured"},
			wantPackCount: 1,
			checkResult: func(t *testing.T, result *types.DisplayResult) {
				assert.Len(t, result.Packs, 1)
				pack := result.Packs[0]
				assert.True(t, pack.HasConfig)

				// Should have config file in display
				var hasConfigFile bool
				for _, file := range pack.Files {
					if file.Path == ".dodot.toml" && file.Status == "config" {
						hasConfigFile = true
						break
					}
				}
				assert.True(t, hasConfigFile, "Should have .dodot.toml in files")
			},
		},
		// "non-existent pack" test case removed - tested in pipeline_test.go
		{
			name: "empty dotfiles directory",
			setupFS: func(fs types.FS, rootDir string) {
				// Just create the root directory, no packs
			},
			packNames:     []string{},
			wantPackCount: 0,
			checkResult: func(t *testing.T, result *types.DisplayResult) {
				assert.Empty(t, result.Packs)
			},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			// Setup test filesystem
			fs := testutil.NewTestFS()
			rootDir := "dotfiles"
			dataDir := "data/dodot"

			// Set environment variables to use test paths
			t.Setenv("DOTFILES_ROOT", rootDir)
			t.Setenv("DODOT_DATA_DIR", dataDir)
			t.Setenv("HOME", "test-home")

			// Create directories
			testutil.CreateDirT(t, fs, rootDir)
			testutil.CreateDirT(t, fs, dataDir)

			// Run test setup
			if tt.setupFS != nil {
				tt.setupFS(fs, rootDir)
			}

			// Create test paths
			testPaths, err := paths.New(rootDir)
			require.NoError(t, err)

			// Run status command
			result, err := StatusPacks(StatusPacksOptions{
				DotfilesRoot: rootDir,
				PackNames:    tt.packNames,
				Paths:        testPaths,
				FileSystem:   fs,
			})

			if tt.wantErr {
				require.Error(t, err)
				return
			}

			require.NoError(t, err)
			require.NotNil(t, result)

			assert.Equal(t, tt.wantPackCount, len(result.Packs))

			if tt.checkResult != nil {
				tt.checkResult(t, result)
			}
		})
	}
}

func TestStatusPacks_Integration(t *testing.T) {
	// This test verifies status checking after actual deployment
	tmpDir := t.TempDir()
	homeDir := filepath.Join(tmpDir, "home")
	require.NoError(t, os.MkdirAll(homeDir, 0755))

	// Set HOME to a predictable value for consistent symlink resolution
	t.Setenv("HOME", homeDir)

	// Create a pack with various files
	packDir := filepath.Join(tmpDir, "test")
	require.NoError(t, os.MkdirAll(packDir, 0755))
	require.NoError(t, os.WriteFile(filepath.Join(packDir, ".vimrc"), []byte("vim config"), 0644))
	require.NoError(t, os.WriteFile(filepath.Join(packDir, "install.sh"), []byte("#!/bin/sh\necho installed"), 0755))
	require.NoError(t, os.MkdirAll(filepath.Join(packDir, "bin"), 0755))
	require.NoError(t, os.WriteFile(filepath.Join(packDir, "aliases.sh"), []byte("alias ll='ls -l'"), 0644))

	// Create paths instance
	testPaths, err := paths.New(tmpDir)
	require.NoError(t, err)

	// Create datastore
	fs := filesystem.NewOS()
	dataStore := datastore.New(fs, testPaths)

	// Deploy some files using the actual datastore methods
	// 1. Create a symlink deployment
	_, err = dataStore.Link("test", filepath.Join(packDir, ".vimrc"))
	require.NoError(t, err)

	// 2. Add a directory to PATH
	err = dataStore.AddToPath("test", filepath.Join(packDir, "bin"))
	require.NoError(t, err)

	// 3. Record a provisioning run
	checksum, err := hashutil.CalculateFileChecksum(filepath.Join(packDir, "install.sh"))
	require.NoError(t, err)
	err = dataStore.RecordProvisioning("test", "install.sh.sentinel", checksum)
	require.NoError(t, err)

	// Now run status command to check what was deployed
	result, err := StatusPacks(StatusPacksOptions{
		DotfilesRoot: tmpDir,
		PackNames:    []string{"test"},
		Paths:        testPaths,
		FileSystem:   fs,
	})

	require.NoError(t, err)
	require.NotNil(t, result)

	// Verify results
	require.Len(t, result.Packs, 1)
	pack := result.Packs[0]
	assert.Equal(t, "test", pack.Name)

	// Check file statuses
	fileStatuses := make(map[string]string)
	for _, file := range pack.Files {
		fileStatuses[file.Path] = file.Status
		t.Logf("File: %s, Status: %s, Message: %s", file.Path, file.Status, file.Message)
	}

	// These should show as deployed/ready
	assert.Equal(t, "success", fileStatuses[".vimrc"], "Symlink should be deployed")
	assert.Equal(t, "success", fileStatuses["bin"], "Directory should be in PATH")
	assert.Equal(t, "success", fileStatuses["install.sh"], "Provision script should show as run")

	// This wasn't deployed, so should be missing/queue
	assert.Equal(t, "queue", fileStatuses["aliases.sh"], "Shell profile not deployed")
}

func TestStatusPacksOptions(t *testing.T) {
	// Test that StatusPacksOptions properly initializes defaults
	testPaths, err := paths.New("/non/existent/path")
	require.NoError(t, err)

	result, err := StatusPacks(StatusPacksOptions{
		DotfilesRoot: "/non/existent/path",
		PackNames:    []string{"test"},
		Paths:        testPaths,
	})

	// Should get an error about non-existent path
	assert.Error(t, err)
	assert.Nil(t, result)
	assert.Contains(t, err.Error(), "dotfiles root does not exist")
}

func TestStatusPacksEmptyDir(t *testing.T) {
	// Create a temporary directory for testing
	tmpDir := t.TempDir()

	testPaths, err := paths.New(tmpDir)
	require.NoError(t, err)

	result, err := StatusPacks(StatusPacksOptions{
		DotfilesRoot: tmpDir,
		PackNames:    []string{},
		Paths:        testPaths,
	})

	// Should succeed with empty result
	require.NoError(t, err)
	require.NotNil(t, result)
	assert.Equal(t, "status", result.Command)
	assert.Empty(t, result.Packs)
}

func TestStatusPacksRealFS(t *testing.T) {
	// Test with real filesystem
	tmpDir := t.TempDir()

	// Create some test packs
	vimDir := filepath.Join(tmpDir, "vim")
	require.NoError(t, os.MkdirAll(vimDir, 0755))
	require.NoError(t, os.WriteFile(filepath.Join(vimDir, ".vimrc"), []byte("vim config"), 0644))

	zshDir := filepath.Join(tmpDir, "zsh")
	require.NoError(t, os.MkdirAll(zshDir, 0755))
	require.NoError(t, os.WriteFile(filepath.Join(zshDir, ".zshrc"), []byte("zsh config"), 0644))

	// Create ignored pack
	ignoredDir := filepath.Join(tmpDir, "ignored")
	require.NoError(t, os.MkdirAll(ignoredDir, 0755))
	require.NoError(t, os.WriteFile(filepath.Join(ignoredDir, ".dodotignore"), []byte(""), 0644))

	// Test all packs
	testPaths, err := paths.New(tmpDir)
	require.NoError(t, err)

	result, err := StatusPacks(StatusPacksOptions{
		DotfilesRoot: tmpDir,
		PackNames:    []string{},
		Paths:        testPaths,
	})

	require.NoError(t, err)
	require.NotNil(t, result)
	assert.Len(t, result.Packs, 3)

	// Check pack names and statuses
	packMap := make(map[string]*types.DisplayPack)
	for i := range result.Packs {
		packMap[result.Packs[i].Name] = &result.Packs[i]
	}

	assert.Contains(t, packMap, "vim")
	assert.Contains(t, packMap, "zsh")
	assert.Contains(t, packMap, "ignored")

	// Check ignored pack
	assert.True(t, packMap["ignored"].IsIgnored)
	assert.Equal(t, "ignored", packMap["ignored"].Status)

	// Test specific pack
	result2, err := StatusPacks(StatusPacksOptions{
		DotfilesRoot: tmpDir,
		PackNames:    []string{"vim"},
		Paths:        testPaths,
	})

	require.NoError(t, err)
	require.NotNil(t, result2)
	assert.Len(t, result2.Packs, 1)
	assert.Equal(t, "vim", result2.Packs[0].Name)
}

func TestStatusPacksAdditionalInfo(t *testing.T) {
	// Test that AdditionalInfo field is properly populated for different handler types
	tmpDir := t.TempDir()
	homeDir := filepath.Join(tmpDir, "home")
	require.NoError(t, os.MkdirAll(homeDir, 0755))

	// Create a pack with different file types
	packDir := filepath.Join(tmpDir, "test-pack")
	require.NoError(t, os.MkdirAll(packDir, 0755))

	// Create files that trigger different handlers
	require.NoError(t, os.WriteFile(filepath.Join(packDir, ".vimrc"), []byte("vim config"), 0644))
	require.NoError(t, os.WriteFile(filepath.Join(packDir, "install.sh"), []byte("#!/bin/sh\necho test"), 0755))
	require.NoError(t, os.WriteFile(filepath.Join(packDir, "Brewfile"), []byte("brew \"git\""), 0644))
	require.NoError(t, os.WriteFile(filepath.Join(packDir, "aliases.sh"), []byte("alias ll='ls -l'"), 0644))
	require.NoError(t, os.MkdirAll(filepath.Join(packDir, "bin"), 0755))
	require.NoError(t, os.WriteFile(filepath.Join(packDir, "bin/mytool"), []byte("#!/bin/sh\necho tool"), 0755))

	// Set HOME to our test directory
	t.Setenv("HOME", homeDir)

	testPaths, err := paths.New(tmpDir)
	require.NoError(t, err)

	result, err := StatusPacks(StatusPacksOptions{
		DotfilesRoot: tmpDir,
		PackNames:    []string{"test-pack"},
		Paths:        testPaths,
	})

	require.NoError(t, err)
	require.NotNil(t, result)
	require.Len(t, result.Packs, 1)

	pack := result.Packs[0]
	assert.Equal(t, "test-pack", pack.Name)

	// Check that each file has appropriate AdditionalInfo
	fileInfoMap := make(map[string]types.DisplayFile)
	for _, file := range pack.Files {
		fileInfoMap[file.Path] = file
	}

	// Test symlink handler - should show target path with ~ for home
	vimrcFile, ok := fileInfoMap[".vimrc"]
	assert.True(t, ok, ".vimrc should be in status")
	assert.Equal(t, "symlink", vimrcFile.Handler)
	assert.Equal(t, "~/.vimrc", vimrcFile.AdditionalInfo, "Symlink should show target path with ~ for home")

	// Test install handler - should show "run script"
	installFile, ok := fileInfoMap["install.sh"]
	assert.True(t, ok, "install.sh should be in status")
	assert.Equal(t, "install", installFile.Handler)
	assert.Equal(t, "run script", installFile.AdditionalInfo, "Install script should show 'run script'")

	// Test homebrew handler - should show "brew install"
	brewFile, ok := fileInfoMap["Brewfile"]
	assert.True(t, ok, "Brewfile should be in status")
	assert.Equal(t, "homebrew", brewFile.Handler)
	assert.Equal(t, "brew install", brewFile.AdditionalInfo, "Brewfile should show 'brew install'")

	// Test shell handler - should show "shell source"
	aliasesFile, ok := fileInfoMap["aliases.sh"]
	assert.True(t, ok, "aliases.sh should be in status")
	assert.Equal(t, "shell", aliasesFile.Handler)
	assert.Equal(t, "shell source", aliasesFile.AdditionalInfo, "Shell profile should show 'shell source'")

	// Test path handler - should show "add to $PATH"
	binDir, ok := fileInfoMap["bin"]
	assert.True(t, ok, "bin directory should be in status")
	assert.Equal(t, "path", binDir.Handler)
	assert.Equal(t, "add to $PATH", binDir.AdditionalInfo, "Path handler should show 'add to $PATH'")
}
