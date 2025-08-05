package style

import (
	"strings"
	"testing"
)

func TestRenderFileStatus(t *testing.T) {
	tests := []struct {
		name     string
		status   FileStatus
		contains []string
	}{
		{
			name: "symlink success",
			status: FileStatus{
				PowerUp:  "symlink",
				FilePath: ".vimrc",
				Status:   StatusSuccess,
				Target:   "$HOME/.vimrc",
			},
			contains: []string{"symlink", ".vimrc", "linked to $HOME/.vimrc"},
		},
		{
			name: "symlink queue",
			status: FileStatus{
				PowerUp:  "symlink",
				FilePath: ".vimrc",
				Status:   StatusQueue,
				Target:   "$HOME/.vimrc",
			},
			contains: []string{"symlink", ".vimrc", "will be linked to $HOME/.vimrc"},
		},
		{
			name: "profile included",
			status: FileStatus{
				PowerUp:  "profile",
				FilePath: "alias.sh",
				Status:   StatusSuccess,
				Target:   "zsh",
			},
			contains: []string{"profile", "alias.sh", "included in zsh"},
		},
		{
			name: "homebrew with date",
			status: FileStatus{
				PowerUp:  "homebrew",
				FilePath: "Brewfile",
				Status:   StatusSuccess,
				Target:   "homebrew",
				Date:     "2024-01-15",
			},
			contains: []string{"homebrew", "Brewfile", "executed on homebrew", "2024-01-15"},
		},
		{
			name: "install script override",
			status: FileStatus{
				PowerUp:    "install",
				FilePath:   "runme.sh",
				Status:     StatusQueue,
				Target:     "installation",
				IsOverride: true,
			},
			contains: []string{"install", "*runme.sh", "to be executed"},
		},
		{
			name: "config file",
			status: FileStatus{
				PowerUp:  "config",
				FilePath: ".dodot.toml",
				Status:   StatusConfig,
			},
			contains: []string{"config", ".dodot.toml", "dodot config file found"},
		},
		{
			name: "error status",
			status: FileStatus{
				PowerUp:  "symlink",
				FilePath: ".vimrc",
				Status:   StatusError,
				Target:   "$HOME/.vimrc",
			},
			contains: []string{"symlink", ".vimrc", "failed to symlink"},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result := RenderFileStatus(tt.status)
			for _, expected := range tt.contains {
				if !strings.Contains(result, expected) {
					t.Errorf("Expected output to contain %q, got %q", expected, result)
				}
			}
		})
	}
}

func TestRenderPackStatus(t *testing.T) {
	tests := []struct {
		name     string
		pack     PackStatus
		contains []string
	}{
		{
			name: "pack with config and files",
			pack: PackStatus{
				Name:      "neovim",
				Status:    StatusSuccess,
				HasConfig: true,
				Files: []FileStatus{
					{
						PowerUp:  "symlink",
						FilePath: ".vimrc",
						Status:   StatusSuccess,
						Target:   "$HOME/.vimrc",
					},
					{
						PowerUp:  "profile",
						FilePath: "alias.sh",
						Status:   StatusSuccess,
						Target:   "zsh",
					},
				},
			},
			contains: []string{"neovim:", "config", ".dodot.toml", "symlink", ".vimrc", "profile", "alias.sh"},
		},
		{
			name: "ignored pack",
			pack: PackStatus{
				Name:      "temp",
				Status:    StatusIgnored,
				IsIgnored: true,
			},
			contains: []string{"temp:", ".dodotignore", "dodot is ignoring this dir"},
		},
		{
			name: "pack with errors",
			pack: PackStatus{
				Name:   "broken",
				Status: StatusAlert,
				Files: []FileStatus{
					{
						PowerUp:  "symlink",
						FilePath: "config",
						Status:   StatusError,
						Target:   "$HOME/.config",
					},
				},
			},
			contains: []string{"broken:", "symlink", "failed to"},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result := RenderPackStatus(tt.pack)
			for _, expected := range tt.contains {
				if !strings.Contains(result, expected) {
					t.Errorf("Expected output to contain %q, got:\n%s", expected, result)
				}
			}
		})
	}
}

func TestAggregatePackStatus(t *testing.T) {
	tests := []struct {
		name     string
		files    []FileStatus
		expected Status
	}{
		{
			name: "all success",
			files: []FileStatus{
				{Status: StatusSuccess},
				{Status: StatusSuccess},
			},
			expected: StatusSuccess,
		},
		{
			name: "all queue",
			files: []FileStatus{
				{Status: StatusQueue},
				{Status: StatusQueue},
			},
			expected: StatusQueue,
		},
		{
			name: "has error",
			files: []FileStatus{
				{Status: StatusSuccess},
				{Status: StatusError},
				{Status: StatusQueue},
			},
			expected: StatusAlert,
		},
		{
			name: "mixed success and queue",
			files: []FileStatus{
				{Status: StatusSuccess},
				{Status: StatusQueue},
			},
			expected: StatusQueue,
		},
		{
			name:     "empty files",
			files:    []FileStatus{},
			expected: StatusQueue,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result := AggregatePackStatus(tt.files)
			if result != tt.expected {
				t.Errorf("Expected %s, got %s", tt.expected, result)
			}
		})
	}
}
