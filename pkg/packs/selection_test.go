package packs

import (
	"testing"

	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestGetPackNames(t *testing.T) {
	tests := []struct {
		name      string
		packs     []types.Pack
		wantNames []string
	}{
		{
			name:      "empty pack list",
			packs:     []types.Pack{},
			wantNames: []string{},
		},
		{
			name: "single pack",
			packs: []types.Pack{
				{Name: "vim", Path: "/home/user/.dotfiles/vim"},
			},
			wantNames: []string{"vim"},
		},
		{
			name: "multiple packs",
			packs: []types.Pack{
				{Name: "vim", Path: "/home/user/.dotfiles/vim"},
				{Name: "zsh", Path: "/home/user/.dotfiles/zsh"},
				{Name: "git", Path: "/home/user/.dotfiles/git"},
			},
			wantNames: []string{"vim", "zsh", "git"},
		},
		{
			name: "packs with metadata",
			packs: []types.Pack{
				{
					Name: "vim",
					Path: "/home/user/.dotfiles/vim",
					Config: types.PackConfig{
						Ignore: []types.IgnoreRule{{Path: "*.tmp"}},
					},
					Metadata: map[string]interface{}{"version": "1.0"},
				},
				{
					Name: "zsh",
					Path: "/home/user/.dotfiles/zsh",
					Config: types.PackConfig{
						Override: []types.OverrideRule{{Path: ".zshrc", Powerup: "symlink"}},
					},
				},
			},
			wantNames: []string{"vim", "zsh"},
		},
		{
			name:      "nil pack list returns empty slice",
			packs:     nil,
			wantNames: []string{},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got := GetPackNames(tt.packs)
			assert.Equal(t, tt.wantNames, got)
		})
	}
}

func TestSelectPacks(t *testing.T) {
	// Test packs
	vimPack := types.Pack{Name: "vim", Path: "/home/user/.dotfiles/vim"}
	zshPack := types.Pack{Name: "zsh", Path: "/home/user/.dotfiles/zsh"}
	gitPack := types.Pack{Name: "git", Path: "/home/user/.dotfiles/git"}
	tmuxPack := types.Pack{Name: "tmux", Path: "/home/user/.dotfiles/tmux"}

	allPacks := []types.Pack{vimPack, zshPack, gitPack, tmuxPack}

	tests := []struct {
		name          string
		allPacks      []types.Pack
		selectedNames []string
		wantPacks     []types.Pack
		wantErr       bool
		errContains   string
	}{
		{
			name:          "no selection returns all packs",
			allPacks:      allPacks,
			selectedNames: []string{},
			wantPacks:     allPacks,
			wantErr:       false,
		},
		{
			name:          "select single pack",
			allPacks:      allPacks,
			selectedNames: []string{"vim"},
			wantPacks:     []types.Pack{vimPack},
			wantErr:       false,
		},
		{
			name:          "select multiple packs",
			allPacks:      allPacks,
			selectedNames: []string{"vim", "git"},
			wantPacks:     []types.Pack{gitPack, vimPack}, // Should be sorted
			wantErr:       false,
		},
		{
			name:          "select all packs explicitly",
			allPacks:      allPacks,
			selectedNames: []string{"vim", "zsh", "git", "tmux"},
			wantPacks:     []types.Pack{gitPack, tmuxPack, vimPack, zshPack}, // Sorted
			wantErr:       false,
		},
		{
			name:          "select non-existent pack",
			allPacks:      allPacks,
			selectedNames: []string{"nonexistent"},
			wantErr:       true,
			errContains:   "pack(s) not found",
		},
		{
			name:          "select mix of existing and non-existent",
			allPacks:      allPacks,
			selectedNames: []string{"vim", "nonexistent"},
			wantErr:       true,
			errContains:   "pack(s) not found",
		},
		{
			name:          "multiple non-existent packs",
			allPacks:      allPacks,
			selectedNames: []string{"foo", "bar"},
			wantErr:       true,
			errContains:   "pack(s) not found",
		},
		{
			name:          "empty pack list with selection",
			allPacks:      []types.Pack{},
			selectedNames: []string{"vim"},
			wantErr:       true,
			errContains:   "pack(s) not found",
		},
		{
			name:          "duplicate selection returns duplicates",
			allPacks:      allPacks,
			selectedNames: []string{"vim", "vim", "git"},
			wantPacks:     []types.Pack{gitPack, vimPack, vimPack}, // Sorted but includes duplicates
			wantErr:       false,
		},
		{
			name: "selection preserves pack details",
			allPacks: []types.Pack{
				{
					Name: "vim",
					Path: "/home/user/.dotfiles/vim",
					Config: types.PackConfig{
						Ignore: []types.IgnoreRule{{Path: "*.tmp"}},
					},
					Metadata: map[string]interface{}{"test": true},
				},
			},
			selectedNames: []string{"vim"},
			wantPacks: []types.Pack{
				{
					Name: "vim",
					Path: "/home/user/.dotfiles/vim",
					Config: types.PackConfig{
						Ignore: []types.IgnoreRule{{Path: "*.tmp"}},
					},
					Metadata: map[string]interface{}{"test": true},
				},
			},
			wantErr: false,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got, err := SelectPacks(tt.allPacks, tt.selectedNames)

			if tt.wantErr {
				require.Error(t, err)
				if tt.errContains != "" {
					assert.Contains(t, err.Error(), tt.errContains)
				}

				// Check error details if it's a pack not found error
				if tt.errContains == "pack(s) not found" {
					// The error should contain details about what wasn't found
					assert.Contains(t, err.Error(), "[PACK_NOT_FOUND]")
				}
			} else {
				require.NoError(t, err)
				assert.Equal(t, tt.wantPacks, got)
			}
		})
	}
}

func TestSelectPacks_Sorting(t *testing.T) {
	// Test that packs are always sorted by name
	packs := []types.Pack{
		{Name: "zsh", Path: "/path/zsh"},
		{Name: "vim", Path: "/path/vim"},
		{Name: "git", Path: "/path/git"},
		{Name: "tmux", Path: "/path/tmux"},
		{Name: "bash", Path: "/path/bash"},
	}

	selected, err := SelectPacks(packs, []string{"zsh", "bash", "git"})
	require.NoError(t, err)

	// Verify sorted order
	assert.Len(t, selected, 3)
	assert.Equal(t, "bash", selected[0].Name)
	assert.Equal(t, "git", selected[1].Name)
	assert.Equal(t, "zsh", selected[2].Name)
}

func TestSelectPacks_ErrorMessage(t *testing.T) {
	allPacks := []types.Pack{
		{Name: "vim", Path: "/path/vim"},
		{Name: "zsh", Path: "/path/zsh"},
	}

	_, err := SelectPacks(allPacks, []string{"foo", "bar"})
	require.Error(t, err)

	// Verify the error message
	assert.Contains(t, err.Error(), "pack(s) not found")
	assert.Contains(t, err.Error(), "[PACK_NOT_FOUND]")
}
