package list

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/arthur-debert/dodot/pkg/testutil"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	// Import to register triggers and handlers
	_ "github.com/arthur-debert/dodot/pkg/handlers"
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
				pack := testutil.SetupTestPack(t, "vim")
				pack.AddFile(t, ".vimrc", "\" vim config")
				return pack.Root
			},
			wantPacks: []string{"vim"},
			wantErr:   false,
		},
		{
			name: "list with multiple packs",
			setup: func(t *testing.T) string {
				packs := testutil.SetupMultiplePacks(t, "zsh", "vim", "git", "tmux", "bash")
				for name, pack := range packs {
					// Add a file to make it a valid pack
					pack.AddFile(t, "config", "# "+name+" config")
				}
				return packs["vim"].Root
			},
			wantPacks: []string{"bash", "git", "tmux", "vim", "zsh"},
			wantErr:   false,
		},
		{
			name: "list ignores hidden directories",
			setup: func(t *testing.T) string {
				pack := testutil.SetupTestPack(t, "vim")
				pack.AddFile(t, ".vimrc", "\" vim config")

				// Create hidden directory (should be ignored)
				hiddenDir := filepath.Join(pack.Root, ".hidden")
				require.NoError(t, os.MkdirAll(hiddenDir, 0755))
				require.NoError(t, os.WriteFile(
					filepath.Join(hiddenDir, "config"),
					[]byte("# hidden config"),
					0644,
				))

				// Create .git directory (should be ignored)
				gitDir := filepath.Join(pack.Root, ".git")
				require.NoError(t, os.MkdirAll(gitDir, 0755))

				return pack.Root
			},
			wantPacks: []string{"vim"},
			wantErr:   false,
		},
		{
			name: "list respects .dodotignore",
			setup: func(t *testing.T) string {
				packs := testutil.SetupMultiplePacks(t, "vim", "ignored")

				// Add content to vim pack
				packs["vim"].AddFile(t, ".vimrc", "\" vim config")

				// Create ignored pack with .dodotignore
				packs["ignored"].AddFile(t, ".dodotignore", "")
				packs["ignored"].AddFile(t, "config", "# ignored config")

				return packs["vim"].Root
			},
			wantPacks: []string{"vim"},
			wantErr:   false,
		},
		// These test cases validate specific error messages for the list command
		// While pack discovery is tested in pipeline, list-specific error handling is tested here
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
