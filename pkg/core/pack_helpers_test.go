package core

import (
	"path/filepath"
	"testing"

	"github.com/arthur-debert/dodot/pkg/testutil"
	"github.com/stretchr/testify/assert"
)

func TestDiscoverAndSelectPacks(t *testing.T) {
	// Create test environment
	tempDir := testutil.TempDir(t, "discover-packs")
	dotfilesDir := testutil.CreateDir(t, tempDir, "dotfiles")

	// Create test packs
	testutil.CreateDir(t, dotfilesDir, "vim")
	testutil.CreateDir(t, dotfilesDir, "zsh")
	testutil.CreateDir(t, dotfilesDir, "git")
	testutil.CreateFile(t, dotfilesDir, "vim/vimrc", "vim config")
	testutil.CreateFile(t, dotfilesDir, "zsh/zshrc", "zsh config")

	tests := []struct {
		name      string
		packNames []string
		wantPacks []string
		wantErr   bool
	}{
		{
			name:      "discover all packs",
			packNames: nil,
			wantPacks: []string{"git", "vim", "zsh"}, // alphabetical order
			wantErr:   false,
		},
		{
			name:      "select specific packs",
			packNames: []string{"vim", "git"},
			wantPacks: []string{"git", "vim"}, // alphabetical order
			wantErr:   false,
		},
		{
			name:      "select non-existent pack",
			packNames: []string{"nonexistent"},
			wantPacks: nil,
			wantErr:   true,
		},
		{
			name:      "mix of existing and non-existent",
			packNames: []string{"vim", "nonexistent"},
			wantPacks: nil,
			wantErr:   true,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			packs, err := DiscoverAndSelectPacks(dotfilesDir, tt.packNames)

			if tt.wantErr {
				assert.Error(t, err)
			} else {
				assert.NoError(t, err)
				assert.Len(t, packs, len(tt.wantPacks))

				// Check pack names
				for i, pack := range packs {
					assert.Equal(t, tt.wantPacks[i], pack.Name)
				}
			}
		})
	}
}

func TestFindPack(t *testing.T) {
	// Create test environment
	tempDir := testutil.TempDir(t, "find-pack")
	dotfilesDir := testutil.CreateDir(t, tempDir, "dotfiles")

	// Create test packs
	testutil.CreateDir(t, dotfilesDir, "vim")
	testutil.CreateFile(t, dotfilesDir, "vim/vimrc", "vim config")

	tests := []struct {
		name     string
		packName string
		wantName string
		wantErr  bool
	}{
		{
			name:     "find existing pack",
			packName: "vim",
			wantName: "vim",
			wantErr:  false,
		},
		{
			name:     "find non-existent pack",
			packName: "nonexistent",
			wantName: "",
			wantErr:  true,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			pack, err := FindPack(dotfilesDir, tt.packName)

			if tt.wantErr {
				assert.Error(t, err)
				assert.Nil(t, pack)
			} else {
				assert.NoError(t, err)
				assert.NotNil(t, pack)
				assert.Equal(t, tt.wantName, pack.Name)
			}
		})
	}
}

func TestValidateDotfilesRoot(t *testing.T) {
	// Create test environment
	tempDir := testutil.TempDir(t, "validate-root")
	dotfilesDir := testutil.CreateDir(t, tempDir, "dotfiles")
	filePath := testutil.CreateFile(t, tempDir, "file.txt", "not a directory")

	tests := []struct {
		name    string
		root    string
		wantErr bool
	}{
		{
			name:    "valid directory",
			root:    dotfilesDir,
			wantErr: false,
		},
		{
			name:    "empty root",
			root:    "",
			wantErr: true,
		},
		{
			name:    "non-existent directory",
			root:    filepath.Join(tempDir, "nonexistent"),
			wantErr: true,
		},
		{
			name:    "file instead of directory",
			root:    filePath,
			wantErr: true,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			err := ValidateDotfilesRoot(tt.root)

			if tt.wantErr {
				assert.Error(t, err)
			} else {
				assert.NoError(t, err)
			}
		})
	}
}
