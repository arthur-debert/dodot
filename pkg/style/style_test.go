package style

import (
	"strings"
	"testing"

	"github.com/arthur-debert/dodot/pkg/types"
)

func TestPtermStyles(t *testing.T) {
	// Test that our pterm styles are properly initialized
	tests := []struct {
		name     string
		text     string
		style    func(string) string
		contains string
	}{
		{
			name:     "bold text",
			text:     "Hello World",
			style:    Bold,
			contains: "Hello World",
		},
		{
			name:     "italic text",
			text:     "Hello World",
			style:    Italic,
			contains: "Hello World",
		},
		{
			name:     "underline text",
			text:     "Hello World",
			style:    Underline,
			contains: "Hello World",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result := tt.style(tt.text)
			if !strings.Contains(result, tt.contains) {
				t.Errorf("Expected output to contain %q, got %q", tt.contains, result)
			}
		})
	}
}

func TestIndent(t *testing.T) {
	tests := []struct {
		name     string
		text     string
		level    int
		expected string
	}{
		{
			name:     "no indent",
			text:     "Hello",
			level:    0,
			expected: "Hello",
		},
		{
			name:     "single indent",
			text:     "Hello",
			level:    1,
			expected: "  Hello",
		},
		{
			name:     "double indent",
			text:     "Hello",
			level:    2,
			expected: "    Hello",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result := Indent(tt.text, tt.level)
			if result != tt.expected {
				t.Errorf("Expected %q, got %q", tt.expected, result)
			}
		})
	}
}

func TestTerminalRenderer(t *testing.T) {
	renderer := NewTerminalRenderer()

	t.Run("RenderPackList", func(t *testing.T) {
		packs := []types.PackInfo{
			{Name: "vim", Path: "/home/user/dotfiles/vim"},
			{Name: "zsh", Path: "/home/user/dotfiles/zsh"},
		}

		result := renderer.RenderPackList(packs)
		if !strings.Contains(result, "vim") {
			t.Error("Expected output to contain pack name 'vim'")
		}
		if !strings.Contains(result, "zsh") {
			t.Error("Expected output to contain pack name 'zsh'")
		}
		if !strings.Contains(result, "Available Packs") {
			t.Error("Expected output to contain title")
		}
	})

	t.Run("RenderPackList empty", func(t *testing.T) {
		result := renderer.RenderPackList([]types.PackInfo{})
		if !strings.Contains(result, "No packs found") {
			t.Error("Expected 'No packs found' message")
		}
	})

	t.Run("RenderOperations", func(t *testing.T) {
		ops := []types.Operation{
			{
				Type:        types.OperationCreateSymlink,
				Source:      "/home/user/dotfiles/vim/.vimrc",
				Target:      "/home/user/.vimrc",
				Description: "Link vim config",
				Status:      types.StatusReady,
			},
		}

		result := renderer.RenderOperations(ops)
		if !strings.Contains(result, "symlink") {
			t.Error("Expected output to contain 'symlink'")
		}
		if !strings.Contains(result, ".vimrc") {
			t.Error("Expected output to contain '.vimrc'")
		}
	})

	t.Run("RenderOperations empty", func(t *testing.T) {
		result := renderer.RenderOperations([]types.Operation{})
		if !strings.Contains(result, "No operations to perform") {
			t.Error("Expected 'No operations to perform' message")
		}
	})

	t.Run("RenderOperations different statuses", func(t *testing.T) {
		ops := []types.Operation{
			{Type: types.OperationCreateSymlink, Status: types.StatusReady},
			{Type: types.OperationCreateSymlink, Status: types.StatusSkipped},
			{Type: types.OperationCreateSymlink, Status: types.StatusConflict},
			{Type: types.OperationCreateSymlink, Status: types.StatusError},
		}

		result := renderer.RenderOperations(ops)
		// Just ensure it doesn't panic and contains operations
		if !strings.Contains(result, "symlink") {
			t.Error("Expected operations to be rendered")
		}
	})

	t.Run("RenderError", func(t *testing.T) {
		err := &testError{msg: "Something went wrong", code: "E001"}
		result := renderer.RenderError(err)

		if !strings.Contains(result, "E001") {
			t.Error("Expected output to contain error code")
		}
		if !strings.Contains(result, "Something went wrong") {
			t.Error("Expected output to contain error message")
		}
	})

	t.Run("RenderError nil", func(t *testing.T) {
		result := renderer.RenderError(nil)
		if result != "" {
			t.Errorf("Expected empty string for nil error, got %q", result)
		}
	})

	t.Run("RenderProgress", func(t *testing.T) {
		result := renderer.RenderProgress(5, 10, "Processing...")
		if !strings.Contains(result, "5/10") {
			t.Error("Expected progress numbers")
		}
		if !strings.Contains(result, "Processing...") {
			t.Error("Expected message")
		}
		// Check for progress bar characters
		if !strings.Contains(result, "█") && !strings.Contains(result, "░") {
			t.Error("Expected progress bar characters")
		}
	})
}

func TestPlainRenderer(t *testing.T) {
	renderer := NewPlainRenderer()

	t.Run("RenderPackList", func(t *testing.T) {
		packs := []types.PackInfo{
			{Name: "vim", Path: "/home/user/dotfiles/vim"},
			{Name: "zsh", Path: "/home/user/dotfiles/zsh"},
		}

		result := renderer.RenderPackList(packs)
		if !strings.Contains(result, "Available Packs:") {
			t.Error("Expected header 'Available Packs:'")
		}
		if !strings.Contains(result, "- vim") {
			t.Error("Expected '- vim' in output")
		}
		if !strings.Contains(result, "- zsh") {
			t.Error("Expected '- zsh' in output")
		}
	})

	t.Run("RenderPackList empty", func(t *testing.T) {
		result := renderer.RenderPackList([]types.PackInfo{})
		if result != "No packs found" {
			t.Errorf("Expected 'No packs found', got %q", result)
		}
	})

	t.Run("RenderOperations", func(t *testing.T) {
		ops := []types.Operation{
			{
				Type:        types.OperationCreateSymlink,
				Description: "Link vim config",
			},
		}

		result := renderer.RenderOperations(ops)
		if !strings.Contains(result, "create_symlink") {
			t.Error("Expected operation type in output")
		}
		if !strings.Contains(result, "Link vim config") {
			t.Error("Expected description in output")
		}
	})

	t.Run("RenderError", func(t *testing.T) {
		err := &testError{msg: "Something went wrong", code: "E001"}
		result := renderer.RenderError(err)

		if !strings.Contains(result, "Error:") {
			t.Error("Expected 'Error:' prefix")
		}
		if !strings.Contains(result, "Something went wrong") {
			t.Error("Expected error message")
		}
	})

	t.Run("RenderProgress", func(t *testing.T) {
		result := renderer.RenderProgress(5, 10, "Processing...")
		expected := "Progress: 5/10 - Processing..."
		if result != expected {
			t.Errorf("Expected %q, got %q", expected, result)
		}
	})
}

// testError implements an error with code for testing
type testError struct {
	msg  string
	code string
}

func (e *testError) Error() string {
	return e.msg
}

func (e *testError) Code() string {
	return e.code
}

func TestRenderPackStatuses(t *testing.T) {
	renderer := NewTerminalRenderer()

	packs := []PackStatus{
		{
			Name:      "vim",
			Status:    StatusSuccess,
			HasConfig: true,
			Files: []FileStatus{
				{
					PowerUp:  "symlink",
					FilePath: ".vimrc",
					Status:   StatusSuccess,
					Target:   "$HOME/.vimrc",
				},
			},
		},
		{
			Name:      "ignored",
			Status:    StatusIgnored,
			IsIgnored: true,
		},
	}

	result := renderer.RenderPackStatuses(packs)

	// Check for pack names
	if !strings.Contains(result, "vim:") {
		t.Error("Expected vim pack in output")
	}
	if !strings.Contains(result, "ignored:") {
		t.Error("Expected ignored pack in output")
	}

	// Check for spacing between packs
	lines := strings.Split(result, "\n")
	emptyLineCount := 0
	for _, line := range lines {
		if line == "" {
			emptyLineCount++
		}
	}
	if emptyLineCount < 1 {
		t.Error("Expected spacing between packs")
	}
}
