package datastore

import (
	"path/filepath"
	"testing"

	"github.com/arthur-debert/dodot/pkg/paths"
	"github.com/arthur-debert/dodot/pkg/types"
)

func TestHasHandlerState(t *testing.T) {
	tests := []struct {
		name        string
		pack        string
		handler     string
		setupFunc   func(fs types.FS, paths paths.Paths)
		expected    bool
		expectError bool
	}{
		{
			name:    "handler with state returns true",
			pack:    "vim",
			handler: "symlink",
			setupFunc: func(fs types.FS, paths paths.Paths) {
				dir := paths.PackHandlerDir("vim", "symlink")
				_ = fs.MkdirAll(dir, 0755)
				_ = fs.WriteFile(filepath.Join(dir, "vimrc"), []byte("link"), 0644)
			},
			expected: true,
		},
		{
			name:    "handler with empty directory returns false",
			pack:    "vim",
			handler: "symlink",
			setupFunc: func(fs types.FS, paths paths.Paths) {
				dir := paths.PackHandlerDir("vim", "symlink")
				_ = fs.MkdirAll(dir, 0755)
			},
			expected: false,
		},
		{
			name:        "non-existent handler returns false",
			pack:        "vim",
			handler:     "symlink",
			setupFunc:   func(fs types.FS, paths paths.Paths) {},
			expected:    false,
			expectError: false,
		},
		{
			name:    "handler with multiple files returns true",
			pack:    "tmux",
			handler: "homebrew",
			setupFunc: func(fs types.FS, paths paths.Paths) {
				dir := paths.PackHandlerDir("tmux", "homebrew")
				_ = fs.MkdirAll(dir, 0755)
				_ = fs.WriteFile(filepath.Join(dir, "Brewfile-abc123"), []byte("timestamp"), 0644)
				_ = fs.WriteFile(filepath.Join(dir, "Brewfile-def456"), []byte("timestamp"), 0644)
			},
			expected: true,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			// Setup
			fs := NewMemoryFS()
			p, _ := paths.New("/home/user/.dotfiles")
			ds := New(fs, p)

			// Execute setup
			if tt.setupFunc != nil {
				tt.setupFunc(fs, p)
			}

			// Test
			result, err := ds.HasHandlerState(tt.pack, tt.handler)

			// Verify
			if tt.expectError && err == nil {
				t.Error("expected error but got none")
			}
			if !tt.expectError && err != nil {
				t.Errorf("unexpected error: %v", err)
			}
			if result != tt.expected {
				t.Errorf("expected %v but got %v", tt.expected, result)
			}
		})
	}
}

func TestListPackHandlers(t *testing.T) {
	tests := []struct {
		name        string
		pack        string
		setupFunc   func(fs types.FS, paths paths.Paths)
		expected    []string
		expectError bool
	}{
		{
			name: "pack with multiple handlers",
			pack: "vim",
			setupFunc: func(fs types.FS, paths paths.Paths) {
				packDir := filepath.Join(paths.DataDir(), "packs", "vim")
				_ = fs.MkdirAll(filepath.Join(packDir, "symlink"), 0755)
				_ = fs.MkdirAll(filepath.Join(packDir, "homebrew"), 0755)
				_ = fs.MkdirAll(filepath.Join(packDir, "shell"), 0755)
			},
			expected: []string{"homebrew", "shell", "symlink"}, // alphabetical order
		},
		{
			name:      "non-existent pack returns empty list",
			pack:      "nonexistent",
			setupFunc: func(fs types.FS, paths paths.Paths) {},
			expected:  []string{},
		},
		{
			name: "pack with files in directory (should be ignored)",
			pack: "tmux",
			setupFunc: func(fs types.FS, paths paths.Paths) {
				packDir := filepath.Join(paths.DataDir(), "packs", "tmux")
				_ = fs.MkdirAll(packDir, 0755)
				_ = fs.WriteFile(filepath.Join(packDir, "not-a-handler.txt"), []byte("data"), 0644)
				_ = fs.MkdirAll(filepath.Join(packDir, "symlink"), 0755)
			},
			expected: []string{"symlink"},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			// Setup
			fs := NewMemoryFS()
			p, _ := paths.New("/home/user/.dotfiles")
			ds := New(fs, p)

			// Execute setup
			if tt.setupFunc != nil {
				tt.setupFunc(fs, p)
			}

			// Test
			result, err := ds.ListPackHandlers(tt.pack)

			// Verify
			if tt.expectError && err == nil {
				t.Error("expected error but got none")
			}
			if !tt.expectError && err != nil {
				t.Errorf("unexpected error: %v", err)
			}

			// Sort results for consistent comparison
			if len(result) != len(tt.expected) {
				t.Errorf("expected %d handlers but got %d: %v", len(tt.expected), len(result), result)
				return
			}

			// Create a map for easier comparison
			resultMap := make(map[string]bool)
			for _, h := range result {
				resultMap[h] = true
			}

			for _, expected := range tt.expected {
				if !resultMap[expected] {
					t.Errorf("expected handler %s not found in result %v", expected, result)
				}
			}
		})
	}
}

func TestListHandlerSentinels(t *testing.T) {
	tests := []struct {
		name        string
		pack        string
		handler     string
		setupFunc   func(fs types.FS, paths paths.Paths)
		expected    []string
		expectError bool
	}{
		{
			name:    "handler with sentinels",
			pack:    "vim",
			handler: "homebrew",
			setupFunc: func(fs types.FS, paths paths.Paths) {
				dir := paths.PackHandlerDir("vim", "homebrew")
				_ = fs.MkdirAll(dir, 0755)
				_ = fs.WriteFile(filepath.Join(dir, "Brewfile-abc123"), []byte("2024-01-01"), 0644)
				_ = fs.WriteFile(filepath.Join(dir, "Brewfile-def456"), []byte("2024-01-02"), 0644)
			},
			expected: []string{"Brewfile-abc123", "Brewfile-def456"},
		},
		{
			name:    "handler with subdirectories (should be ignored)",
			pack:    "tmux",
			handler: "symlink",
			setupFunc: func(fs types.FS, paths paths.Paths) {
				dir := paths.PackHandlerDir("tmux", "symlink")
				_ = fs.MkdirAll(dir, 0755)
				_ = fs.WriteFile(filepath.Join(dir, "tmux.conf"), []byte("link"), 0644)
				_ = fs.MkdirAll(filepath.Join(dir, "subdir"), 0755)
			},
			expected: []string{"tmux.conf"},
		},
		{
			name:        "non-existent handler returns empty list",
			pack:        "vim",
			handler:     "nonexistent",
			setupFunc:   func(fs types.FS, paths paths.Paths) {},
			expected:    []string{},
			expectError: false,
		},
		{
			name:    "empty handler directory returns empty list",
			pack:    "vim",
			handler: "install",
			setupFunc: func(fs types.FS, paths paths.Paths) {
				dir := paths.PackHandlerDir("vim", "install")
				_ = fs.MkdirAll(dir, 0755)
			},
			expected: []string{},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			// Setup
			fs := NewMemoryFS()
			p, _ := paths.New("/home/user/.dotfiles")
			ds := New(fs, p)

			// Execute setup
			if tt.setupFunc != nil {
				tt.setupFunc(fs, p)
			}

			// Test
			result, err := ds.ListHandlerSentinels(tt.pack, tt.handler)

			// Verify
			if tt.expectError && err == nil {
				t.Error("expected error but got none")
			}
			if !tt.expectError && err != nil {
				t.Errorf("unexpected error: %v", err)
			}

			// Sort results for consistent comparison
			if len(result) != len(tt.expected) {
				t.Errorf("expected %d sentinels but got %d: %v", len(tt.expected), len(result), result)
				return
			}

			// Create a map for easier comparison
			resultMap := make(map[string]bool)
			for _, s := range result {
				resultMap[s] = true
			}

			for _, expected := range tt.expected {
				if !resultMap[expected] {
					t.Errorf("expected sentinel %s not found in result %v", expected, result)
				}
			}
		})
	}
}
