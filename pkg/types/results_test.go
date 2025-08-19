package types

import (
	"testing"
	"time"

	"github.com/stretchr/testify/assert"
)

func TestDisplayPack_GetPackStatus(t *testing.T) {
	tests := []struct {
		name       string
		pack       DisplayPack
		wantStatus string
	}{
		{
			name: "empty pack returns queue",
			pack: DisplayPack{
				Name:  "empty-pack",
				Files: []DisplayFile{},
			},
			wantStatus: "queue",
		},
		{
			name: "all success returns success",
			pack: DisplayPack{
				Name: "success-pack",
				Files: []DisplayFile{
					{Path: "file1", Status: "success"},
					{Path: "file2", Status: "success"},
					{Path: "file3", Status: "success"},
				},
			},
			wantStatus: "success",
		},
		{
			name: "any error returns alert",
			pack: DisplayPack{
				Name: "error-pack",
				Files: []DisplayFile{
					{Path: "file1", Status: "success"},
					{Path: "file2", Status: "error"},
					{Path: "file3", Status: "success"},
				},
			},
			wantStatus: "alert",
		},
		{
			name: "all errors returns alert",
			pack: DisplayPack{
				Name: "all-errors-pack",
				Files: []DisplayFile{
					{Path: "file1", Status: "error"},
					{Path: "file2", Status: "error"},
				},
			},
			wantStatus: "alert",
		},
		{
			name: "mixed statuses returns queue",
			pack: DisplayPack{
				Name: "mixed-pack",
				Files: []DisplayFile{
					{Path: "file1", Status: "success"},
					{Path: "file2", Status: "queue"},
					{Path: "file3", Status: "success"},
				},
			},
			wantStatus: "queue",
		},
		{
			name: "config files are ignored",
			pack: DisplayPack{
				Name: "config-pack",
				Files: []DisplayFile{
					{Path: "file1", Status: "success"},
					{Path: ".dodot.toml", Status: "config"},
					{Path: "file2", Status: "success"},
				},
			},
			wantStatus: "success",
		},
		{
			name: "only config files returns success",
			pack: DisplayPack{
				Name: "only-config-pack",
				Files: []DisplayFile{
					{Path: ".dodot.toml", Status: "config"},
					{Path: ".dodotignore", Status: "config"},
				},
			},
			wantStatus: "success",
		},
		{
			name: "error takes precedence over queue",
			pack: DisplayPack{
				Name: "error-precedence-pack",
				Files: []DisplayFile{
					{Path: "file1", Status: "queue"},
					{Path: "file2", Status: "error"},
					{Path: "file3", Status: "queue"},
				},
			},
			wantStatus: "alert",
		},
		{
			name: "unknown status treated as not success",
			pack: DisplayPack{
				Name: "unknown-pack",
				Files: []DisplayFile{
					{Path: "file1", Status: "success"},
					{Path: "file2", Status: "unknown"},
					{Path: "file3", Status: "success"},
				},
			},
			wantStatus: "queue",
		},
		{
			name: "ignored status files",
			pack: DisplayPack{
				Name: "ignored-pack",
				Files: []DisplayFile{
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

func TestActionResult_Duration(t *testing.T) {
	tests := []struct {
		name         string
		startTime    time.Time
		endTime      time.Time
		wantDuration time.Duration
	}{
		{
			name:         "normal duration",
			startTime:    time.Date(2023, 1, 1, 10, 0, 0, 0, time.UTC),
			endTime:      time.Date(2023, 1, 1, 10, 0, 5, 0, time.UTC),
			wantDuration: 5 * time.Second,
		},
		{
			name:         "zero duration",
			startTime:    time.Date(2023, 1, 1, 10, 0, 0, 0, time.UTC),
			endTime:      time.Date(2023, 1, 1, 10, 0, 0, 0, time.UTC),
			wantDuration: 0,
		},
		{
			name:         "millisecond precision",
			startTime:    time.Date(2023, 1, 1, 10, 0, 0, 0, time.UTC),
			endTime:      time.Date(2023, 1, 1, 10, 0, 0, 500000000, time.UTC),
			wantDuration: 500 * time.Millisecond,
		},
		{
			name:         "long duration",
			startTime:    time.Date(2023, 1, 1, 10, 0, 0, 0, time.UTC),
			endTime:      time.Date(2023, 1, 1, 11, 30, 45, 0, time.UTC),
			wantDuration: time.Hour + 30*time.Minute + 45*time.Second,
		},
		{
			name:         "negative duration (end before start)",
			startTime:    time.Date(2023, 1, 1, 10, 0, 5, 0, time.UTC),
			endTime:      time.Date(2023, 1, 1, 10, 0, 0, 0, time.UTC),
			wantDuration: -5 * time.Second,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			ar := &ActionResult{
				StartTime: tt.startTime,
				EndTime:   tt.endTime,
			}
			got := ar.Duration()
			assert.Equal(t, tt.wantDuration, got)
		})
	}
}

func TestDisplayPack_ComplexScenarios(t *testing.T) {
	t.Run("large pack with mixed statuses", func(t *testing.T) {
		pack := DisplayPack{
			Name: "large-pack",
			Files: []DisplayFile{
				{Path: "file1", Status: "success"},
				{Path: "file2", Status: "success"},
				{Path: "file3", Status: "config"},
				{Path: "file4", Status: "success"},
				{Path: "file5", Status: "queue"},
				{Path: "file6", Status: "success"},
				{Path: "file7", Status: "config"},
				{Path: "file8", Status: "success"},
			},
		}
		// Has non-success (queue) so should return queue
		assert.Equal(t, "queue", pack.GetPackStatus())
	})

	t.Run("pack with override files", func(t *testing.T) {
		pack := DisplayPack{
			Name: "override-pack",
			Files: []DisplayFile{
				{Path: "file1", Status: "success", IsOverride: true},
				{Path: "file2", Status: "error", IsOverride: true},
				{Path: "file3", Status: "success", IsOverride: false},
			},
		}
		// Override doesn't affect status calculation
		assert.Equal(t, "alert", pack.GetPackStatus())
	})

	t.Run("pack with message and powerup details", func(t *testing.T) {
		lastExec := time.Now()
		pack := DisplayPack{
			Name:      "detailed-pack",
			HasConfig: true,
			IsIgnored: false,
			Files: []DisplayFile{
				{
					PowerUp:      "symlink",
					Path:         "file1",
					Status:       "success",
					Message:      "Linked successfully",
					LastExecuted: &lastExec,
				},
				{
					PowerUp:      "homebrew",
					Path:         "Brewfile",
					Status:       "error",
					Message:      "Failed to install",
					LastExecuted: nil,
				},
			},
		}
		// Error takes precedence
		assert.Equal(t, "alert", pack.GetPackStatus())
	})
}

func TestActionResult_WithCompleteAction(t *testing.T) {
	// Test ActionResult with a complete Action
	start := time.Date(2023, 1, 1, 10, 0, 0, 0, time.UTC)
	end := time.Date(2023, 1, 1, 10, 0, 30, 0, time.UTC)

	ar := &ActionResult{
		Action: Action{
			Type:        ActionTypeLink,
			Description: "Create symlink",
			Source:      "/source/file",
			Target:      "/target/file",
			Priority:    10,
			Pack:        "test-pack",
			PowerUpName: "symlink",
			Metadata:    map[string]interface{}{"test": true},
		},
		Status:              StatusReady,
		Error:               nil,
		StartTime:           start,
		EndTime:             end,
		Message:             "Completed successfully",
		SynthfsOperationIDs: []string{"op1", "op2"},
	}

	// Verify duration calculation
	assert.Equal(t, 30*time.Second, ar.Duration())

	// Verify all fields are accessible
	assert.Equal(t, ActionTypeLink, ar.Action.Type)
	assert.Equal(t, StatusReady, ar.Status)
	assert.Nil(t, ar.Error)
	assert.Equal(t, "Completed successfully", ar.Message)
	assert.Len(t, ar.SynthfsOperationIDs, 2)
}
