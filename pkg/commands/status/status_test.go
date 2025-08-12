package status

import (
	"crypto/sha256"
	"encoding/hex"
	"os"
	"path/filepath"
	"testing"

	"github.com/arthur-debert/dodot/pkg/testutil"
	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

// testPaths implements types.Pather for testing
type testPaths struct {
	dataDir string
}

func (p *testPaths) DotfilesRoot() string {
	return "dotfiles"
}

func (p *testPaths) DataDir() string {
	return p.dataDir
}

func (p *testPaths) ConfigDir() string {
	return filepath.Join(p.dataDir, "config")
}

func (p *testPaths) CacheDir() string {
	return filepath.Join(p.dataDir, "cache")
}

func (p *testPaths) StateDir() string {
	return filepath.Join(p.dataDir, "state")
}

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
		{
			name: "non-existent pack",
			setupFS: func(fs types.FS, rootDir string) {
				// Create one pack
				testutil.CreateDirT(t, fs, rootDir+"/vim")
			},
			packNames: []string{"nonexistent"},
			wantErr:   true,
		},
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

			// Create directories
			testutil.CreateDirT(t, fs, rootDir)
			testutil.CreateDirT(t, fs, dataDir)

			// Run test setup
			if tt.setupFS != nil {
				tt.setupFS(fs, rootDir)
			}

			// Create test paths
			testPaths := &testPaths{dataDir: dataDir}

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
	// This test verifies the full status checking with deployed files
	fs := testutil.NewTestFS()
	rootDir := "dotfiles"
	dataDir := "data/dodot"

	// Create directories
	testutil.CreateDirT(t, fs, rootDir)
	testutil.CreateDirT(t, fs, dataDir)

	// Create a pack with various files
	packDir := rootDir + "/test"
	testutil.CreateDirT(t, fs, packDir)
	testutil.CreateFileT(t, fs, packDir+"/.vimrc", "vim config")
	testutil.CreateFileT(t, fs, packDir+"/install.sh", "#!/bin/sh\necho installed")
	testutil.CreateDirT(t, fs, packDir+"/bin")
	testutil.CreateFileT(t, fs, packDir+"/aliases.sh", "alias ll='ls -l'")

	// Simulate some deployed files
	// Deployed symlink
	deployedSymlink := dataDir + "/deployed/symlink/.vimrc"
	testutil.CreateDirT(t, fs, dataDir+"/deployed/symlink")
	require.NoError(t, fs.Symlink(packDir+"/.vimrc", deployedSymlink))

	// Install script sentinel
	installSentinel := dataDir + "/install/test_install.sh.sentinel"
	testutil.CreateDirT(t, fs, dataDir+"/install")
	checksum := calculateChecksum([]byte("#!/bin/sh\necho installed"))
	testutil.CreateFileT(t, fs, installSentinel, checksum+":2024-01-15T10:00:00Z")

	// Create test paths that return our test directories
	testPaths := &testPaths{dataDir: dataDir}

	// Run status command
	result, err := StatusPacks(StatusPacksOptions{
		DotfilesRoot: rootDir,
		PackNames:    []string{"test"},
		Paths:        testPaths,
		FileSystem:   fs,
	})

	require.NoError(t, err)
	require.NotNil(t, result)

	// Verify results
	assert.Len(t, result.Packs, 1)
	pack := result.Packs[0]
	assert.Equal(t, "test", pack.Name)

	// Check file statuses
	fileStatuses := make(map[string]string)
	for _, file := range pack.Files {
		fileStatuses[file.Path] = file.Status
	}

	// Symlink should be deployed (success)
	assert.Equal(t, "success", fileStatuses[".vimrc"])

	// Install script should be executed (success)
	assert.Equal(t, "success", fileStatuses["install.sh"])

	// PATH and shell source should be pending
	assert.Equal(t, "queue", fileStatuses["bin"])
	assert.Equal(t, "queue", fileStatuses["aliases.sh"])

	// Pack should have mixed status (queue)
	assert.Equal(t, "queue", pack.Status)
}

// calculateChecksum calculates SHA256 checksum for test data
func calculateChecksum(data []byte) string {
	hash := sha256.Sum256(data)
	return hex.EncodeToString(hash[:])
}

func TestStatusPacksOptions(t *testing.T) {
	// Test that StatusPacksOptions properly initializes defaults
	result, err := StatusPacks(StatusPacksOptions{
		DotfilesRoot: "/non/existent/path",
		PackNames:    []string{"test"},
	})

	// Should get an error about non-existent path
	assert.Error(t, err)
	assert.Nil(t, result)
	assert.Contains(t, err.Error(), "dotfiles root does not exist")
}

func TestStatusPacksEmptyDir(t *testing.T) {
	// Create a temporary directory for testing
	tmpDir := t.TempDir()

	result, err := StatusPacks(StatusPacksOptions{
		DotfilesRoot: tmpDir,
		PackNames:    []string{},
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
	result, err := StatusPacks(StatusPacksOptions{
		DotfilesRoot: tmpDir,
		PackNames:    []string{},
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
	})

	require.NoError(t, err)
	require.NotNil(t, result2)
	assert.Len(t, result2.Packs, 1)
	assert.Equal(t, "vim", result2.Packs[0].Name)
}
