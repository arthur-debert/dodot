// pkg/ui/display/types_test.go
// TEST TYPE: Unit Tests
// DEPENDENCIES: None
// PURPOSE: Test result types and display structures

package display_test

import (
	"testing"
	"time"

	"github.com/arthur-debert/dodot/pkg/ui/display"
	"github.com/stretchr/testify/assert"
)

func TestDisplayPack_GetPackStatus(t *testing.T) {
	tests := []struct {
		name       string
		pack       display.DisplayPack
		wantStatus string
	}{
		{
			name: "empty_pack_returns_queue",
			pack: display.DisplayPack{
				Name:  "empty-pack",
				Files: []display.DisplayFile{},
			},
			wantStatus: "queue",
		},
		{
			name: "all_success_returns_success",
			pack: display.DisplayPack{
				Name: "success-pack",
				Files: []display.DisplayFile{
					{Status: "success"},
					{Status: "success"},
					{Status: "success"},
				},
			},
			wantStatus: "success",
		},
		{
			name: "any_error_returns_alert",
			pack: display.DisplayPack{
				Name: "error-pack",
				Files: []display.DisplayFile{
					{Status: "success"},
					{Status: "error"},
					{Status: "success"},
				},
			},
			wantStatus: "alert",
		},
		{
			name: "warning_without_errors_returns_partial",
			pack: display.DisplayPack{
				Name: "warning-pack",
				Files: []display.DisplayFile{
					{Status: "success"},
					{Status: "warning"},
					{Status: "success"},
				},
			},
			wantStatus: "partial",
		},
		{
			name: "error_takes_precedence_over_warning",
			pack: display.DisplayPack{
				Name: "mixed-pack",
				Files: []display.DisplayFile{
					{Status: "warning"},
					{Status: "error"},
					{Status: "success"},
				},
			},
			wantStatus: "alert",
		},
		{
			name: "mixed_non_success_returns_queue",
			pack: display.DisplayPack{
				Name: "queue-pack",
				Files: []display.DisplayFile{
					{Status: "success"},
					{Status: "queue"},
					{Status: "ignored"},
				},
			},
			wantStatus: "queue",
		},
		{
			name: "config_files_are_skipped",
			pack: display.DisplayPack{
				Name: "config-pack",
				Files: []display.DisplayFile{
					{Status: "success"},
					{Status: "config"}, // This should be ignored
					{Status: "success"},
				},
			},
			wantStatus: "success",
		},
		{
			name: "only_config_files_returns_success",
			pack: display.DisplayPack{
				Name: "only-config-pack",
				Files: []display.DisplayFile{
					{Status: "config"},
					{Status: "config"},
				},
			},
			wantStatus: "success", // All non-config files succeed (i.e., none)
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got := tt.pack.GetPackStatus()
			assert.Equal(t, tt.wantStatus, got)
		})
	}
}

func TestFormatCommandMessage(t *testing.T) {
	tests := []struct {
		name      string
		verb      string
		packNames []string
		want      string
	}{
		{
			name:      "empty_pack_list",
			verb:      "linked",
			packNames: []string{},
			want:      "",
		},
		{
			name:      "single_pack",
			verb:      "linked",
			packNames: []string{"vim"},
			want:      "The pack vim has been linked.",
		},
		{
			name:      "two_packs",
			verb:      "unlinked",
			packNames: []string{"vim", "git"},
			want:      "The packs vim and git have been unlinked.",
		},
		{
			name:      "three_packs",
			verb:      "provisioned",
			packNames: []string{"vim", "git", "tmux"},
			want:      "The packs vim, git, and tmux have been provisioned.",
		},
		{
			name:      "many_packs",
			verb:      "turned on",
			packNames: []string{"vim", "git", "tmux", "zsh", "docker"},
			want:      "The packs vim, git, tmux, zsh, and docker have been turned on.",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got := display.FormatCommandMessage(tt.verb, tt.packNames)
			assert.Equal(t, tt.want, got)
		})
	}
}

func TestTruncateLeft(t *testing.T) {
	tests := []struct {
		name   string
		s      string
		maxLen int
		want   string
	}{
		{
			name:   "shorter_than_max",
			s:      "hello",
			maxLen: 10,
			want:   "hello",
		},
		{
			name:   "exactly_max",
			s:      "hello",
			maxLen: 5,
			want:   "hello",
		},
		{
			name:   "longer_than_max",
			s:      "hello world",
			maxLen: 8,
			want:   "...world",
		},
		{
			name:   "very_long_string",
			s:      "/Users/example/very/long/path/to/file.txt",
			maxLen: 20,
			want:   ".../path/to/file.txt",
		},
		{
			name:   "maxlen_3",
			s:      "hello",
			maxLen: 3,
			want:   "...",
		},
		{
			name:   "maxlen_less_than_3",
			s:      "hello",
			maxLen: 2,
			want:   "...",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got := display.TruncateLeft(tt.s, tt.maxLen)
			assert.Equal(t, tt.want, got)
		})
	}
}

func TestFormatSymlinkForDisplay(t *testing.T) {
	tests := []struct {
		name    string
		path    string
		homeDir string
		maxLen  int
		want    string
	}{
		{
			name:    "home_replacement",
			path:    "/home/user/.config/vim/vimrc",
			homeDir: "/home/user",
			maxLen:  30,
			want:    "~/.config/vim/vimrc",
		},
		{
			name:    "home_replacement_with_truncation",
			path:    "/home/user/.config/very/long/path/to/file.txt",
			homeDir: "/home/user",
			maxLen:  20,
			want:    ".../path/to/file.txt",
		},
		{
			name:    "no_home_dir",
			path:    "/etc/config/file.conf",
			homeDir: "",
			maxLen:  30,
			want:    "/etc/config/file.conf",
		},
		{
			name:    "path_not_in_home",
			path:    "/opt/app/config",
			homeDir: "/home/user",
			maxLen:  30,
			want:    "/opt/app/config",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got := display.FormatSymlinkForDisplay(tt.path, tt.homeDir, tt.maxLen)
			assert.Equal(t, tt.want, got)
		})
	}
}

func TestGetHandlerSymbol(t *testing.T) {
	tests := []struct {
		handler string
		want    string
	}{
		{"symlink", "➞"},
		{"shell", "⚙"},
		{"shell_add_path", "⚙"},
		{"path", "+"},
		{"homebrew", "⚙"},
		{"provision", "×"},
		{"unknown", ""},
		{"", ""},
	}

	for _, tt := range tests {
		t.Run(tt.handler, func(t *testing.T) {
			got := display.GetHandlerSymbol(tt.handler)
			assert.Equal(t, tt.want, got, "handler: %s", tt.handler)
		})
	}
}

func TestGenConfigResult(t *testing.T) {
	// Test that GenConfigResult struct exists and has expected fields
	result := display.GenConfigResult{
		ConfigContent: "test config",
		FilesWritten:  []string{"file1.toml", "file2.toml"},
	}

	assert.Equal(t, "test config", result.ConfigContent)
	assert.Equal(t, []string{"file1.toml", "file2.toml"}, result.FilesWritten)
}

func TestCommandMetadata(t *testing.T) {
	// Test CommandMetadata struct and ensure it properly holds different command data
	t.Run("on_command_metadata", func(t *testing.T) {
		meta := display.CommandMetadata{
			TotalDeployed:  5,
			NoProvision:    true,
			ProvisionRerun: false,
		}

		assert.Equal(t, 5, meta.TotalDeployed)
		assert.True(t, meta.NoProvision)
		assert.False(t, meta.ProvisionRerun)
	})

	t.Run("off_command_metadata", func(t *testing.T) {
		meta := display.CommandMetadata{
			TotalCleared: 3,
			HandlersRun:  []string{"symlink", "shell"},
		}

		assert.Equal(t, 3, meta.TotalCleared)
		assert.Equal(t, []string{"symlink", "shell"}, meta.HandlersRun)
	})

	t.Run("adopt_command_metadata", func(t *testing.T) {
		meta := display.CommandMetadata{
			FilesAdopted: 2,
			AdoptedPaths: []string{"/path/1", "/path/2"},
		}

		assert.Equal(t, 2, meta.FilesAdopted)
		assert.Equal(t, []string{"/path/1", "/path/2"}, meta.AdoptedPaths)
	})
}

func TestPackCommandResult(t *testing.T) {
	now := time.Now()

	result := display.PackCommandResult{
		Command: "up",
		Packs: []display.DisplayPack{
			{
				Name:   "vim",
				Status: "success",
			},
		},
		Message: "The pack vim has been turned on.",
		Metadata: display.CommandMetadata{
			TotalDeployed: 1,
		},
		DryRun:    false,
		Timestamp: now,
	}

	assert.Equal(t, "up", result.Command)
	assert.Len(t, result.Packs, 1)
	assert.Equal(t, "vim", result.Packs[0].Name)
	assert.Equal(t, "The pack vim has been turned on.", result.Message)
	assert.Equal(t, 1, result.Metadata.TotalDeployed)
	assert.False(t, result.DryRun)
	assert.Equal(t, now, result.Timestamp)
}
