package list

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	// Import to register triggers and powerups
	_ "github.com/arthur-debert/dodot/pkg/powerups"
	_ "github.com/arthur-debert/dodot/pkg/triggers"
)

func TestListPacks(t *testing.T) {
	tests := []struct {
		name        string
		setup       func(t *testing.T) string // returns dotfilesRoot
		wantPacks   []string
		wantErr     bool
		expectedErr string
	}{
		{
			name: "list with no packs",
			setup: func(t *testing.T) string {
				tmpDir := t.TempDir()
				dotfilesRoot := filepath.Join(tmpDir, "dotfiles")
				require.NoError(t, os.MkdirAll(dotfilesRoot, 0755))
				return dotfilesRoot
			},
			wantPacks: []string{},
			wantErr:   false,
		},
		{
			name: "list with single pack",
			setup: func(t *testing.T) string {
				tmpDir := t.TempDir()
				dotfilesRoot := filepath.Join(tmpDir, "dotfiles")
				require.NoError(t, os.MkdirAll(dotfilesRoot, 0755))

				// Create vim pack
				vimDir := filepath.Join(dotfilesRoot, "vim")
				require.NoError(t, os.MkdirAll(vimDir, 0755))
				require.NoError(t, os.WriteFile(
					filepath.Join(vimDir, ".vimrc"),
					[]byte("\" vim config"),
					0644,
				))

				return dotfilesRoot
			},
			wantPacks: []string{"vim"},
			wantErr:   false,
		},
		{
			name: "list with multiple packs",
			setup: func(t *testing.T) string {
				tmpDir := t.TempDir()
				dotfilesRoot := filepath.Join(tmpDir, "dotfiles")
				require.NoError(t, os.MkdirAll(dotfilesRoot, 0755))

				// Create packs
				packs := []string{"zsh", "vim", "git", "tmux", "bash"}
				for _, pack := range packs {
					packDir := filepath.Join(dotfilesRoot, pack)
					require.NoError(t, os.MkdirAll(packDir, 0755))
					// Add a file to make it a valid pack
					require.NoError(t, os.WriteFile(
						filepath.Join(packDir, "config"),
						[]byte("# "+pack+" config"),
						0644,
					))
				}

				return dotfilesRoot
			},
			wantPacks: []string{"bash", "git", "tmux", "vim", "zsh"},
			wantErr:   false,
		},
		{
			name: "list ignores hidden directories",
			setup: func(t *testing.T) string {
				tmpDir := t.TempDir()
				dotfilesRoot := filepath.Join(tmpDir, "dotfiles")
				require.NoError(t, os.MkdirAll(dotfilesRoot, 0755))

				// Create regular packs
				vimDir := filepath.Join(dotfilesRoot, "vim")
				require.NoError(t, os.MkdirAll(vimDir, 0755))
				require.NoError(t, os.WriteFile(
					filepath.Join(vimDir, ".vimrc"),
					[]byte("\" vim config"),
					0644,
				))

				// Create hidden directory (should be ignored)
				hiddenDir := filepath.Join(dotfilesRoot, ".hidden")
				require.NoError(t, os.MkdirAll(hiddenDir, 0755))
				require.NoError(t, os.WriteFile(
					filepath.Join(hiddenDir, "config"),
					[]byte("# hidden config"),
					0644,
				))

				// Create .git directory (should be ignored)
				gitDir := filepath.Join(dotfilesRoot, ".git")
				require.NoError(t, os.MkdirAll(gitDir, 0755))

				return dotfilesRoot
			},
			wantPacks: []string{"vim"},
			wantErr:   false,
		},
		{
			name: "list respects .dodotignore",
			setup: func(t *testing.T) string {
				tmpDir := t.TempDir()
				dotfilesRoot := filepath.Join(tmpDir, "dotfiles")
				require.NoError(t, os.MkdirAll(dotfilesRoot, 0755))

				// Create normal pack
				vimDir := filepath.Join(dotfilesRoot, "vim")
				require.NoError(t, os.MkdirAll(vimDir, 0755))
				require.NoError(t, os.WriteFile(
					filepath.Join(vimDir, ".vimrc"),
					[]byte("\" vim config"),
					0644,
				))

				// Create ignored pack
				ignoredDir := filepath.Join(dotfilesRoot, "ignored")
				require.NoError(t, os.MkdirAll(ignoredDir, 0755))
				require.NoError(t, os.WriteFile(
					filepath.Join(ignoredDir, ".dodotignore"),
					[]byte(""),
					0644,
				))
				require.NoError(t, os.WriteFile(
					filepath.Join(ignoredDir, "config"),
					[]byte("# ignored config"),
					0644,
				))

				return dotfilesRoot
			},
			wantPacks: []string{"vim"},
			wantErr:   false,
		},
		{
			name: "list with invalid dotfiles root",
			setup: func(t *testing.T) string {
				return "/nonexistent/path"
			},
			wantErr:     true,
			expectedErr: "dotfiles root does not exist",
		},
		{
			name: "list with file as dotfiles root",
			setup: func(t *testing.T) string {
				tmpDir := t.TempDir()
				filePath := filepath.Join(tmpDir, "file")
				require.NoError(t, os.WriteFile(filePath, []byte("content"), 0644))
				return filePath
			},
			wantErr:     true,
			expectedErr: "dotfiles root is not a directory",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			dotfilesRoot := tt.setup(t)

			// Call ListPacks directly
			result, err := ListPacks(ListPacksOptions{
				DotfilesRoot: dotfilesRoot,
			})

			if tt.wantErr {
				assert.Error(t, err)
				if tt.expectedErr != "" {
					assert.Contains(t, err.Error(), tt.expectedErr)
				}
				return
			}

			require.NoError(t, err)
			require.NotNil(t, result)

			// Check pack names
			packNames := make([]string, len(result.Packs))
			for i, packInfo := range result.Packs {
				packNames[i] = packInfo.Name
			}
			assert.Equal(t, tt.wantPacks, packNames)
		})
	}
}
