package unlink

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/arthur-debert/dodot/pkg/filesystem"
	"github.com/arthur-debert/dodot/pkg/testutil"
	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestUnlinkPacks(t *testing.T) {
	tests := []struct {
		name             string
		setupPacks       func(t *testing.T, dotfilesRoot, dataDir, homeDir string)
		packNames        []string
		dryRun           bool
		expectRemoved    int
		expectExistAfter []string
		expectNotExist   []string
	}{
		{
			name: "removes symlink deployments",
			setupPacks: func(t *testing.T, dotfilesRoot, dataDir, homeDir string) {
				// Create vim pack with deployed symlink
				testutil.CreateFile(t, dotfilesRoot, "vim/vimrc", "vim config")

				// Create intermediate and deployed symlinks
				intermediatePath := filepath.Join(dataDir, "deployed", "symlink", ".vimrc")
				testutil.CreateDir(t, dataDir, "deployed/symlink")
				testutil.CreateSymlink(t, filepath.Join(dotfilesRoot, "vim", "vimrc"), intermediatePath)
				testutil.CreateSymlink(t, intermediatePath, filepath.Join(homeDir, ".vimrc"))
			},
			packNames:     []string{"vim"},
			expectRemoved: 2, // deployed symlink + intermediate symlink
			expectNotExist: []string{
				"~/.vimrc",
				"deployed/symlink/.vimrc",
			},
			expectExistAfter: []string{
				"vim/vimrc", // source file should remain
			},
		},
		{
			name: "removes PATH deployments",
			setupPacks: func(t *testing.T, dotfilesRoot, dataDir, homeDir string) {
				// Create tools pack with bin directory
				testutil.CreateDir(t, dotfilesRoot, "tools/bin")
				testutil.CreateFile(t, dotfilesRoot, "tools/bin/mytool", "#!/bin/bash\necho tool")

				// Create PATH deployment - note: dodot names it with just "bin", not "tools-bin"
				deployedPath := filepath.Join(dataDir, "deployed", "path", "bin")
				testutil.CreateDir(t, dataDir, "deployed/path")
				testutil.CreateSymlink(t, filepath.Join(dotfilesRoot, "tools", "bin"), deployedPath)
			},
			packNames:     []string{"tools"},
			expectRemoved: 1, // path symlink
			expectNotExist: []string{
				"deployed/path/bin",
			},
			expectExistAfter: []string{
				"tools/bin/mytool", // source files should remain
			},
		},
		{
			name: "removes shell profile deployments",
			setupPacks: func(t *testing.T, dotfilesRoot, dataDir, homeDir string) {
				// Create bash pack with aliases
				testutil.CreateFile(t, dotfilesRoot, "bash/aliases.sh", "alias ll='ls -la'")

				// Create shell profile deployment
				deployedPath := filepath.Join(dataDir, "deployed", "shell_profile", "aliases.sh")
				testutil.CreateDir(t, dataDir, "deployed/shell_profile")
				testutil.CreateSymlink(t, filepath.Join(dotfilesRoot, "bash", "aliases.sh"), deployedPath)
			},
			packNames:     []string{"bash"},
			expectRemoved: 1, // shell profile symlink
			expectNotExist: []string{
				"deployed/shell_profile/aliases.sh",
			},
			expectExistAfter: []string{
				"bash/aliases.sh", // source file should remain
			},
		},
		{
			name: "removes state files",
			setupPacks: func(t *testing.T, dotfilesRoot, dataDir, homeDir string) {
				// Create pack with install script
				testutil.CreateFile(t, dotfilesRoot, "mypack/install.sh", "#!/bin/bash\necho installed")

				// Create provision sentinel
				testutil.CreateDir(t, dataDir, "provision/sentinels")
				testutil.CreateFile(t, dataDir, "provision/sentinels/mypack", "checksum123")

				// Create homebrew sentinel
				testutil.CreateDir(t, dataDir, "homebrew")
				testutil.CreateFile(t, dataDir, "homebrew/mypack", "brewchecksum456")
			},
			packNames:     []string{"mypack"},
			expectRemoved: 2, // provision sentinel + brew sentinel
			expectNotExist: []string{
				"provision/sentinels/mypack",
				"homebrew/mypack",
			},
			expectExistAfter: []string{
				"mypack/install.sh", // source file should remain
			},
		},
		{
			name: "dry run doesn't remove anything",
			setupPacks: func(t *testing.T, dotfilesRoot, dataDir, homeDir string) {
				// Create vim pack with deployed symlink
				testutil.CreateFile(t, dotfilesRoot, "vim/vimrc", "vim config")

				// Create intermediate and deployed symlinks
				intermediatePath := filepath.Join(dataDir, "deployed", "symlink", ".vimrc")
				testutil.CreateDir(t, dataDir, "deployed/symlink")
				testutil.CreateSymlink(t, filepath.Join(dotfilesRoot, "vim", "vimrc"), intermediatePath)
				testutil.CreateSymlink(t, intermediatePath, filepath.Join(homeDir, ".vimrc"))
			},
			packNames:     []string{"vim"},
			dryRun:        true,
			expectRemoved: 2, // would remove 2 items
			expectExistAfter: []string{
				"~/.vimrc",                // should still exist
				"deployed/symlink/.vimrc", // should still exist
				"vim/vimrc",               // source file
			},
		},
		{
			name: "handles multiple packs",
			setupPacks: func(t *testing.T, dotfilesRoot, dataDir, homeDir string) {
				// Create vim pack
				testutil.CreateFile(t, dotfilesRoot, "vim/vimrc", "vim config")
				intermediatePath := filepath.Join(dataDir, "deployed", "symlink", ".vimrc")
				testutil.CreateDir(t, dataDir, "deployed/symlink")
				testutil.CreateSymlink(t, filepath.Join(dotfilesRoot, "vim", "vimrc"), intermediatePath)
				testutil.CreateSymlink(t, intermediatePath, filepath.Join(homeDir, ".vimrc"))

				// Create git pack
				testutil.CreateFile(t, dotfilesRoot, "git/gitconfig", "git config")
				gitIntermediate := filepath.Join(dataDir, "deployed", "symlink", ".gitconfig")
				testutil.CreateSymlink(t, filepath.Join(dotfilesRoot, "git", "gitconfig"), gitIntermediate)
				testutil.CreateSymlink(t, gitIntermediate, filepath.Join(homeDir, ".gitconfig"))
			},
			packNames:     []string{"vim", "git"},
			expectRemoved: 4, // 2 symlinks per pack
			expectNotExist: []string{
				"~/.vimrc",
				"~/.gitconfig",
				"deployed/symlink/.vimrc",
				"deployed/symlink/.gitconfig",
			},
			expectExistAfter: []string{
				"vim/vimrc",
				"git/gitconfig",
			},
		},
		{
			name: "handles empty pack list (all packs)",
			setupPacks: func(t *testing.T, dotfilesRoot, dataDir, homeDir string) {
				// Create vim pack
				testutil.CreateFile(t, dotfilesRoot, "vim/vimrc", "vim config")
				intermediatePath := filepath.Join(dataDir, "deployed", "symlink", ".vimrc")
				testutil.CreateDir(t, dataDir, "deployed/symlink")
				testutil.CreateSymlink(t, filepath.Join(dotfilesRoot, "vim", "vimrc"), intermediatePath)
				testutil.CreateSymlink(t, intermediatePath, filepath.Join(homeDir, ".vimrc"))
			},
			packNames:     []string{}, // empty = all packs
			expectRemoved: 2,
			expectNotExist: []string{
				"~/.vimrc",
				"deployed/symlink/.vimrc",
			},
		},
		{
			name: "handles non-existent deployments gracefully",
			setupPacks: func(t *testing.T, dotfilesRoot, dataDir, homeDir string) {
				// Create pack but don't deploy anything
				testutil.CreateFile(t, dotfilesRoot, "vim/vimrc", "vim config")
			},
			packNames:        []string{"vim"},
			expectRemoved:    0, // nothing to remove
			expectExistAfter: []string{"vim/vimrc"},
		},
		{
			name: "doesn't remove user-created symlinks",
			setupPacks: func(t *testing.T, dotfilesRoot, dataDir, homeDir string) {
				// Create vim pack
				testutil.CreateFile(t, dotfilesRoot, "vim/vimrc", "vim config")

				// Create a user's own symlink that doesn't point to our intermediate
				userTarget := filepath.Join(homeDir, "my-own-vimrc")
				testutil.CreateFile(t, homeDir, "my-own-vimrc", "user's vimrc")
				testutil.CreateSymlink(t, userTarget, filepath.Join(homeDir, ".vimrc"))
			},
			packNames:     []string{"vim"},
			expectRemoved: 0, // should not remove user's symlink
			expectExistAfter: []string{
				"~/.vimrc",       // user's symlink should remain
				"~/my-own-vimrc", // user's file should remain
				"vim/vimrc",      // source file should remain
			},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			// Setup test environment
			tempDir := testutil.TempDir(t, "off-command-test")
			dotfilesRoot := filepath.Join(tempDir, "dotfiles")
			homeDir := filepath.Join(tempDir, "home")
			dataDir := filepath.Join(homeDir, ".local", "share", "dodot")

			testutil.CreateDir(t, tempDir, "dotfiles")
			testutil.CreateDir(t, tempDir, "home")
			testutil.CreateDir(t, homeDir, ".local/share/dodot")

			t.Setenv("HOME", homeDir)
			t.Setenv("DOTFILES_ROOT", dotfilesRoot)
			t.Setenv("DODOT_DATA_DIR", dataDir)

			// Setup test packs
			tt.setupPacks(t, dotfilesRoot, dataDir, homeDir)

			// Run off command
			result, err := UnlinkPacks(UnlinkPacksOptions{
				DotfilesRoot: dotfilesRoot,
				DataDir:      dataDir,
				PackNames:    tt.packNames,
				DryRun:       tt.dryRun,
			})

			require.NoError(t, err)
			assert.NotNil(t, result)
			assert.Equal(t, tt.dryRun, result.DryRun)
			assert.Equal(t, tt.expectRemoved, result.TotalRemoved)

			// Check filesystem state
			fs := filesystem.NewOS()

			// Check files that should exist
			for _, path := range tt.expectExistAfter {
				checkPath := path
				if checkPath[0] == '~' {
					checkPath = filepath.Join(homeDir, checkPath[2:])
				} else if !filepath.IsAbs(checkPath) {
					// Check relative to dotfiles or data dir
					if _, err := fs.Stat(filepath.Join(dotfilesRoot, checkPath)); err == nil {
						continue
					}
					checkPath = filepath.Join(dataDir, checkPath)
				}

				_, err := fs.Stat(checkPath)
				assert.NoError(t, err, "Expected %s to exist", path)
			}

			// Check files that should not exist (unless dry run)
			if !tt.dryRun {
				for _, path := range tt.expectNotExist {
					checkPath := path
					if checkPath[0] == '~' {
						checkPath = filepath.Join(homeDir, checkPath[2:])
					} else if !filepath.IsAbs(checkPath) {
						checkPath = filepath.Join(dataDir, checkPath)
					}

					_, err := fs.Stat(checkPath)
					assert.True(t, os.IsNotExist(err), "Expected %s to not exist", path)
				}
			}
		})
	}
}

func TestUnlinkPacks_Errors(t *testing.T) {
	tests := []struct {
		name        string
		packNames   []string
		expectError string
	}{
		{
			name:        "returns error for non-existent pack",
			packNames:   []string{"nonexistent"},
			expectError: "pack(s) not found",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			// Setup minimal test environment
			tempDir := testutil.TempDir(t, "off-error-test")
			dotfilesRoot := filepath.Join(tempDir, "dotfiles")
			dataDir := filepath.Join(tempDir, "data")

			testutil.CreateDir(t, tempDir, "dotfiles")
			testutil.CreateDir(t, tempDir, "data")

			// Run off command
			_, err := UnlinkPacks(UnlinkPacksOptions{
				DotfilesRoot: dotfilesRoot,
				DataDir:      dataDir,
				PackNames:    tt.packNames,
			})

			require.Error(t, err)
			assert.Contains(t, err.Error(), tt.expectError)
		})
	}
}

func TestRemoveIfExists(t *testing.T) {
	tests := []struct {
		name          string
		setupFile     func(t *testing.T, fs types.FS, path string)
		itemType      string
		dryRun        bool
		expectSuccess bool
		expectRemoved bool
	}{
		{
			name: "removes existing file",
			setupFile: func(t *testing.T, fs types.FS, path string) {
				require.NoError(t, fs.WriteFile(path, []byte("content"), 0644))
			},
			itemType:      "file",
			expectSuccess: true,
			expectRemoved: true,
		},
		{
			name: "removes existing symlink",
			setupFile: func(t *testing.T, fs types.FS, path string) {
				target := path + ".target"
				require.NoError(t, fs.WriteFile(target, []byte("content"), 0644))
				require.NoError(t, fs.Symlink(target, path))
			},
			itemType:      "symlink",
			expectSuccess: true,
			expectRemoved: true,
		},
		{
			name:          "handles non-existent file",
			setupFile:     func(t *testing.T, fs types.FS, path string) {},
			itemType:      "file",
			expectSuccess: true, // no error, just nothing to remove
			expectRemoved: false,
		},
		{
			name: "dry run doesn't remove",
			setupFile: func(t *testing.T, fs types.FS, path string) {
				require.NoError(t, fs.WriteFile(path, []byte("content"), 0644))
			},
			itemType:      "file",
			dryRun:        true,
			expectSuccess: true,
			expectRemoved: false, // file should still exist
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			// Setup test filesystem
			tempDir := testutil.TempDir(t, "remove-test")
			testPath := filepath.Join(tempDir, "test-item")

			fs := filesystem.NewOS()

			// Setup the file/symlink
			tt.setupFile(t, fs, testPath)

			// Create options
			opts := UnlinkPacksOptions{
				DryRun: tt.dryRun,
			}

			// Test removeIfExists
			item := removeIfExists(testPath, tt.itemType, fs, opts)

			if !tt.expectSuccess {
				if item != nil {
					assert.False(t, item.Success)
				}
				return
			}

			// Check the result
			if tt.expectRemoved {
				assert.NotNil(t, item)
				assert.True(t, item.Success)
				assert.Equal(t, tt.itemType, item.Type)
				assert.Equal(t, testPath, item.Path)
			}

			// Check filesystem state
			_, err := fs.Stat(testPath)
			if tt.expectRemoved && !tt.dryRun {
				assert.True(t, os.IsNotExist(err), "File should not exist after removal")
			} else if tt.dryRun && item != nil {
				// Dry run - file should still exist
				assert.NoError(t, err, "File should still exist after dry run")
			}
		})
	}
}
