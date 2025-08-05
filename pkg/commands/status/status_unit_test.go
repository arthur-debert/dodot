package status

import (
	"os"
	"path/filepath"
	"testing"
	"time"

	"github.com/arthur-debert/dodot/pkg/testutil"
	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	// Import to register triggers and powerups
	_ "github.com/arthur-debert/dodot/pkg/powerups"
	_ "github.com/arthur-debert/dodot/pkg/triggers"
)

func TestStatusPacks(t *testing.T) {
	tests := []struct {
		name          string
		setup         func(t *testing.T) (string, string) // returns dotfilesRoot, homeDir
		packNames     []string
		wantErr       bool
		expectedPacks []string
		checkStatus   func(t *testing.T, packStatuses []types.PackStatus)
	}{
		{
			name: "status with no packs",
			setup: func(t *testing.T) (string, string) {
				tmpDir := t.TempDir()
				dotfilesRoot := filepath.Join(tmpDir, "dotfiles")
				homeDir := filepath.Join(tmpDir, "home")
				require.NoError(t, os.MkdirAll(dotfilesRoot, 0755))
				require.NoError(t, os.MkdirAll(homeDir, 0755))
				return dotfilesRoot, homeDir
			},
			packNames:     []string{},
			expectedPacks: []string{},
			wantErr:       false,
		},
		{
			name: "status of single pack with install script",
			setup: func(t *testing.T) (string, string) {
				tmpDir := t.TempDir()
				dotfilesRoot := filepath.Join(tmpDir, "dotfiles")
				homeDir := filepath.Join(tmpDir, "home")

				// Create vim pack with install script
				vimDir := filepath.Join(dotfilesRoot, "vim")
				require.NoError(t, os.MkdirAll(vimDir, 0755))

				installScript := `#!/bin/bash
echo "Installing vim plugins..."
mkdir -p ~/.vim/bundle`
				require.NoError(t, os.WriteFile(
					filepath.Join(vimDir, "install.sh"),
					[]byte(installScript),
					0755,
				))

				return dotfilesRoot, homeDir
			},
			packNames:     []string{"vim"},
			expectedPacks: []string{"vim"},
			checkStatus: func(t *testing.T, packStatuses []types.PackStatus) {
				require.Len(t, packStatuses, 1)
				vimStatus := packStatuses[0]
				require.Equal(t, "vim", vimStatus.Name)

				// Find install status
				var installStatus *types.PowerUpStatus
				for _, ps := range vimStatus.PowerUpState {
					if ps.Name == "install" {
						installStatus = &ps
						break
					}
				}

				require.NotNil(t, installStatus, "Expected install status for vim")
				assert.Equal(t, "Not Installed", installStatus.State)
			},
		},
		{
			name: "status of pack with executed install script",
			setup: func(t *testing.T) (string, string) {
				tmpDir := t.TempDir()
				dotfilesRoot := filepath.Join(tmpDir, "dotfiles")
				homeDir := filepath.Join(tmpDir, "home")

				// Create vim pack with install script
				vimDir := filepath.Join(dotfilesRoot, "vim")
				require.NoError(t, os.MkdirAll(vimDir, 0755))

				installScript := `#!/bin/bash
echo "Installing vim plugins..."`
				scriptPath := filepath.Join(vimDir, "install.sh")
				require.NoError(t, os.WriteFile(scriptPath, []byte(installScript), 0755))

				// Calculate checksum and create sentinel file
				checksum, err := testutil.CalculateFileChecksum(scriptPath)
				require.NoError(t, err)

				// Create sentinel file in the test-specific data directory
				testDataDir := filepath.Join(homeDir, ".local", "share", "dodot")
				sentinelPath := filepath.Join(testDataDir, "install", "vim")
				require.NoError(t, os.MkdirAll(filepath.Dir(sentinelPath), 0755))
				require.NoError(t, os.WriteFile(sentinelPath, []byte(checksum), 0644))

				return dotfilesRoot, homeDir
			},
			packNames:     []string{"vim"},
			expectedPacks: []string{"vim"},
			checkStatus: func(t *testing.T, packStatuses []types.PackStatus) {
				require.Len(t, packStatuses, 1)
				vimStatus := packStatuses[0]
				require.Equal(t, "vim", vimStatus.Name)

				// Find install status
				var installStatus *types.PowerUpStatus
				for _, ps := range vimStatus.PowerUpState {
					if ps.Name == "install" {
						installStatus = &ps
						break
					}
				}

				require.NotNil(t, installStatus, "Expected install status for vim")
				assert.Equal(t, "Installed", installStatus.State)
			},
		},
		{
			name: "status of all packs when no args provided",
			setup: func(t *testing.T) (string, string) {
				tmpDir := t.TempDir()
				dotfilesRoot := filepath.Join(tmpDir, "dotfiles")
				homeDir := filepath.Join(tmpDir, "home")

				// Create multiple packs
				vimDir := filepath.Join(dotfilesRoot, "vim")
				zshDir := filepath.Join(dotfilesRoot, "zsh")
				gitDir := filepath.Join(dotfilesRoot, "git")

				require.NoError(t, os.MkdirAll(vimDir, 0755))
				require.NoError(t, os.MkdirAll(zshDir, 0755))
				require.NoError(t, os.MkdirAll(gitDir, 0755))

				// Add some files to make them valid packs
				require.NoError(t, os.WriteFile(
					filepath.Join(vimDir, ".vimrc"),
					[]byte("\" vim config"),
					0644,
				))
				require.NoError(t, os.WriteFile(
					filepath.Join(zshDir, ".zshrc"),
					[]byte("# zsh config"),
					0644,
				))
				require.NoError(t, os.WriteFile(
					filepath.Join(gitDir, ".gitconfig"),
					[]byte("[user]\n  name = Test"),
					0644,
				))

				return dotfilesRoot, homeDir
			},
			packNames:     []string{}, // No args = all packs
			expectedPacks: []string{"git", "vim", "zsh"},
			wantErr:       false,
		},
		{
			name: "status of non-existent pack",
			setup: func(t *testing.T) (string, string) {
				tmpDir := t.TempDir()
				dotfilesRoot := filepath.Join(tmpDir, "dotfiles")
				homeDir := filepath.Join(tmpDir, "home")

				// Create one valid pack
				vimDir := filepath.Join(dotfilesRoot, "vim")
				require.NoError(t, os.MkdirAll(vimDir, 0755))
				require.NoError(t, os.WriteFile(
					filepath.Join(vimDir, ".vimrc"),
					[]byte("\" vim config"),
					0644,
				))

				return dotfilesRoot, homeDir
			},
			packNames: []string{"nonexistent"},
			wantErr:   true,
		},
		{
			name: "status of pack with changed install script",
			setup: func(t *testing.T) (string, string) {
				tmpDir := t.TempDir()
				dotfilesRoot := filepath.Join(tmpDir, "dotfiles")
				homeDir := filepath.Join(tmpDir, "home")

				// Create vim pack with install script
				vimDir := filepath.Join(dotfilesRoot, "vim")
				require.NoError(t, os.MkdirAll(vimDir, 0755))

				// First create sentinel with old checksum
				testDataDir := filepath.Join(homeDir, ".local", "share", "dodot")
				sentinelPath := filepath.Join(testDataDir, "install", "vim")
				require.NoError(t, os.MkdirAll(filepath.Dir(sentinelPath), 0755))
				require.NoError(t, os.WriteFile(sentinelPath, []byte("old-checksum"), 0644))

				// Now create install script with different content
				installScript := `#!/bin/bash
echo "New install script content"`
				require.NoError(t, os.WriteFile(
					filepath.Join(vimDir, "install.sh"),
					[]byte(installScript),
					0755,
				))

				// Touch the sentinel file to give it a valid timestamp
				require.NoError(t, os.Chtimes(sentinelPath, time.Now(), time.Now()))

				return dotfilesRoot, homeDir
			},
			packNames:     []string{"vim"},
			expectedPacks: []string{"vim"},
			checkStatus: func(t *testing.T, packStatuses []types.PackStatus) {
				require.Len(t, packStatuses, 1)
				vimStatus := packStatuses[0]
				require.Equal(t, "vim", vimStatus.Name)

				// Find install status
				var installStatus *types.PowerUpStatus
				for _, ps := range vimStatus.PowerUpState {
					if ps.Name == "install" {
						installStatus = &ps
						break
					}
				}

				require.NotNil(t, installStatus, "Expected install status for vim")
				assert.Equal(t, "Changed", installStatus.State)
			},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			dotfilesRoot, homeDir := tt.setup(t)

			// Set environment variables
			t.Setenv("HOME", homeDir)
			t.Setenv("DODOT_TEST_MODE", "true")
			t.Setenv("DODOT_DATA_DIR", filepath.Join(homeDir, ".local", "share", "dodot"))
			t.Setenv("DODOT_CONFIG_DIR", filepath.Join(homeDir, ".config", "dodot"))
			t.Setenv("DODOT_CACHE_DIR", filepath.Join(homeDir, ".cache", "dodot"))

			// Call StatusPacks directly
			result, err := StatusPacks(StatusPacksOptions{
				DotfilesRoot: dotfilesRoot,
				PackNames:    tt.packNames,
			})

			if tt.wantErr {
				assert.Error(t, err)
				return
			}

			require.NoError(t, err)
			require.NotNil(t, result)

			// Check pack names
			packNames := make([]string, len(result.Packs))
			for i, pack := range result.Packs {
				packNames[i] = pack.Name
			}
			assert.ElementsMatch(t, tt.expectedPacks, packNames)

			// Run additional status checks if provided
			if tt.checkStatus != nil {
				tt.checkStatus(t, result.Packs)
			}
		})
	}
}
