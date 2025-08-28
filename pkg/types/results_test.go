// pkg/types/results_test.go
// TEST TYPE: Unit Tests
// DEPENDENCIES: None
// PURPOSE: Test result types and display structures

package types_test

import (
	"testing"
	"time"

	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/stretchr/testify/assert"
)

func TestDisplayPack_GetPackStatus(t *testing.T) {
	tests := []struct {
		name       string
		pack       types.DisplayPack
		wantStatus string
	}{
		{
			name: "empty_pack_returns_queue",
			pack: types.DisplayPack{
				Name:  "empty-pack",
				Files: []types.DisplayFile{},
			},
			wantStatus: "queue",
		},
		{
			name: "all_success_returns_success",
			pack: types.DisplayPack{
				Name: "success-pack",
				Files: []types.DisplayFile{
					{Path: "file1", Status: "success"},
					{Path: "file2", Status: "success"},
					{Path: "file3", Status: "success"},
				},
			},
			wantStatus: "success",
		},
		{
			name: "any_error_returns_alert",
			pack: types.DisplayPack{
				Name: "error-pack",
				Files: []types.DisplayFile{
					{Path: "file1", Status: "success"},
					{Path: "file2", Status: "error"},
					{Path: "file3", Status: "success"},
				},
			},
			wantStatus: "alert",
		},
		{
			name: "all_errors_returns_alert",
			pack: types.DisplayPack{
				Name: "all-errors-pack",
				Files: []types.DisplayFile{
					{Path: "file1", Status: "error"},
					{Path: "file2", Status: "error"},
				},
			},
			wantStatus: "alert",
		},
		{
			name: "mixed_statuses_returns_queue",
			pack: types.DisplayPack{
				Name: "mixed-pack",
				Files: []types.DisplayFile{
					{Path: "file1", Status: "success"},
					{Path: "file2", Status: "queue"},
					{Path: "file3", Status: "success"},
				},
			},
			wantStatus: "queue",
		},
		{
			name: "config_files_are_ignored",
			pack: types.DisplayPack{
				Name: "config-pack",
				Files: []types.DisplayFile{
					{Path: "file1", Status: "success"},
					{Path: ".dodot.toml", Status: "config"},
					{Path: "file2", Status: "success"},
				},
			},
			wantStatus: "success",
		},
		{
			name: "only_config_files_returns_success",
			pack: types.DisplayPack{
				Name: "only-config-pack",
				Files: []types.DisplayFile{
					{Path: ".dodot.toml", Status: "config"},
					{Path: ".dodotignore", Status: "config"},
				},
			},
			wantStatus: "success",
		},
		{
			name: "error_takes_precedence_over_queue",
			pack: types.DisplayPack{
				Name: "error-precedence-pack",
				Files: []types.DisplayFile{
					{Path: "file1", Status: "queue"},
					{Path: "file2", Status: "error"},
					{Path: "file3", Status: "queue"},
				},
			},
			wantStatus: "alert",
		},
		{
			name: "unknown_status_treated_as_not_success",
			pack: types.DisplayPack{
				Name: "unknown-pack",
				Files: []types.DisplayFile{
					{Path: "file1", Status: "success"},
					{Path: "file2", Status: "unknown"},
					{Path: "file3", Status: "success"},
				},
			},
			wantStatus: "queue",
		},
		{
			name: "ignored_status_files",
			pack: types.DisplayPack{
				Name: "ignored-pack",
				Files: []types.DisplayFile{
					{Path: "file1", Status: "success"},
					{Path: "file2", Status: "ignored"},
					{Path: "file3", Status: "success"},
				},
			},
			wantStatus: "queue",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got := tt.pack.GetPackStatus()
			assert.Equal(t, tt.wantStatus, got)
		})
	}
}

func TestCommandResult_FormatCommandMessage(t *testing.T) {
	tests := []struct {
		name        string
		verb        string
		packNames   []string
		expectedMsg string
	}{
		{
			name:        "single_pack",
			verb:        "linked",
			packNames:   []string{"vim"},
			expectedMsg: "The pack vim has been linked.",
		},
		{
			name:        "two_packs",
			verb:        "unlinked",
			packNames:   []string{"vim", "git"},
			expectedMsg: "The packs vim and git have been unlinked.",
		},
		{
			name:        "three_packs",
			verb:        "provisioned",
			packNames:   []string{"vim", "git", "tools"},
			expectedMsg: "The packs vim, git, and tools have been provisioned.",
		},
		{
			name:        "no_packs",
			verb:        "linked",
			packNames:   []string{},
			expectedMsg: "",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got := types.FormatCommandMessage(tt.verb, tt.packNames)
			assert.Equal(t, tt.expectedMsg, got)
		})
	}
}

func TestDisplayResult_Structure(t *testing.T) {
	now := time.Now()
	result := types.DisplayResult{
		Command: "status",
		Packs: []types.DisplayPack{
			{
				Name:   "vim",
				Status: "success",
				Files: []types.DisplayFile{
					{
						Handler: "symlink",
						Path:    ".vimrc",
						Status:  "success",
						Message: "linked",
					},
				},
			},
		},
		DryRun:    false,
		Timestamp: now,
	}

	assert.Equal(t, "status", result.Command)
	assert.False(t, result.DryRun)
	assert.Equal(t, now, result.Timestamp)
	assert.Len(t, result.Packs, 1)
	assert.Equal(t, "vim", result.Packs[0].Name)
}
