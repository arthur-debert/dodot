// Test Type: Unit Test
// Description: Tests for the packs package - pack selection functions

package packs_test

import (
	"testing"

	"github.com/arthur-debert/dodot/pkg/errors"
	"github.com/arthur-debert/dodot/pkg/packs"
	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/stretchr/testify/assert"
)

func TestSelectPacks(t *testing.T) {
	// Create test packs
	allPacks := []types.Pack{
		{Name: "vim", Path: "/dotfiles/vim"},
		{Name: "bash", Path: "/dotfiles/bash"},
		{Name: "git", Path: "/dotfiles/git"},
		{Name: "zsh", Path: "/dotfiles/zsh"},
	}

	tests := []struct {
		name          string
		allPacks      []types.Pack
		selectedNames []string
		expected      []types.Pack
		expectError   bool
		errorCode     errors.ErrorCode
	}{
		{
			name:          "empty_selection_returns_all",
			allPacks:      allPacks,
			selectedNames: []string{},
			expected:      allPacks,
		},
		{
			name:          "select_single_pack",
			allPacks:      allPacks,
			selectedNames: []string{"vim"},
			expected:      []types.Pack{{Name: "vim", Path: "/dotfiles/vim"}},
		},
		{
			name:          "select_multiple_packs",
			allPacks:      allPacks,
			selectedNames: []string{"vim", "bash"},
			expected: []types.Pack{
				{Name: "bash", Path: "/dotfiles/bash"},
				{Name: "vim", Path: "/dotfiles/vim"},
			},
		},
		{
			name:          "select_all_packs_explicitly",
			allPacks:      allPacks,
			selectedNames: []string{"vim", "bash", "git", "zsh"},
			expected: []types.Pack{
				{Name: "bash", Path: "/dotfiles/bash"},
				{Name: "git", Path: "/dotfiles/git"},
				{Name: "vim", Path: "/dotfiles/vim"},
				{Name: "zsh", Path: "/dotfiles/zsh"},
			},
		},
		{
			name:          "pack_not_found",
			allPacks:      allPacks,
			selectedNames: []string{"nonexistent"},
			expectError:   true,
			errorCode:     errors.ErrPackNotFound,
		},
		{
			name:          "mix_of_found_and_not_found",
			allPacks:      allPacks,
			selectedNames: []string{"vim", "nonexistent", "bash"},
			expectError:   true,
			errorCode:     errors.ErrPackNotFound,
		},
		{
			name:          "empty_allPacks_with_selection",
			allPacks:      []types.Pack{},
			selectedNames: []string{"vim"},
			expectError:   true,
			errorCode:     errors.ErrPackNotFound,
		},
		{
			name:          "empty_allPacks_empty_selection",
			allPacks:      []types.Pack{},
			selectedNames: []string{},
			expected:      []types.Pack{},
		},
		{
			name:          "duplicate_selection_returns_duplicates",
			allPacks:      allPacks,
			selectedNames: []string{"vim", "vim", "bash"},
			expected: []types.Pack{
				{Name: "bash", Path: "/dotfiles/bash"},
				{Name: "vim", Path: "/dotfiles/vim"},
				{Name: "vim", Path: "/dotfiles/vim"},
			},
		},
		{
			name:          "result_is_sorted_by_name",
			allPacks:      allPacks,
			selectedNames: []string{"zsh", "vim", "bash"},
			expected: []types.Pack{
				{Name: "bash", Path: "/dotfiles/bash"},
				{Name: "vim", Path: "/dotfiles/vim"},
				{Name: "zsh", Path: "/dotfiles/zsh"},
			},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result, err := packs.SelectPacks(tt.allPacks, tt.selectedNames)

			if tt.expectError {
				assert.Error(t, err)
				if terr, ok := err.(*errors.DodotError); ok {
					assert.Equal(t, tt.errorCode, terr.Code)
				} else {
					t.Errorf("expected DodotError, got %T", err)
				}
			} else {
				assert.NoError(t, err)
				assert.Equal(t, tt.expected, result)
			}
		})
	}
}

func TestGetPackNames(t *testing.T) {
	tests := []struct {
		name     string
		packs    []types.Pack
		expected []string
	}{
		{
			name:     "empty_packs",
			packs:    []types.Pack{},
			expected: []string{},
		},
		{
			name: "single_pack",
			packs: []types.Pack{
				{Name: "vim", Path: "/dotfiles/vim"},
			},
			expected: []string{"vim"},
		},
		{
			name: "multiple_packs",
			packs: []types.Pack{
				{Name: "vim", Path: "/dotfiles/vim"},
				{Name: "bash", Path: "/dotfiles/bash"},
				{Name: "git", Path: "/dotfiles/git"},
			},
			expected: []string{"vim", "bash", "git"},
		},
		{
			name: "preserves_order",
			packs: []types.Pack{
				{Name: "zsh", Path: "/dotfiles/zsh"},
				{Name: "bash", Path: "/dotfiles/bash"},
				{Name: "vim", Path: "/dotfiles/vim"},
			},
			expected: []string{"zsh", "bash", "vim"},
		},
		{
			name: "handles_packs_with_metadata",
			packs: []types.Pack{
				{
					Name:     "vim",
					Path:     "/dotfiles/vim",
					Metadata: map[string]interface{}{"version": "1.0"},
				},
				{
					Name:     "bash",
					Path:     "/dotfiles/bash",
					Metadata: map[string]interface{}{"author": "test"},
				},
			},
			expected: []string{"vim", "bash"},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result := packs.GetPackNames(tt.packs)
			assert.Equal(t, tt.expected, result)
		})
	}
}
