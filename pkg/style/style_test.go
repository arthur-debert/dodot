package style

import (
	"strings"
	"testing"

	"github.com/arthur-debert/dodot/pkg/types"
)

func TestMarkupParser(t *testing.T) {
	parser := NewMarkupParser()

	tests := []struct {
		name     string
		input    string
		contains []string
	}{
		{
			name:     "simple tags",
			input:    "[title]Hello World[/title]",
			contains: []string{"Hello World"},
		},
		{
			name:     "nested tags",
			input:    "[title]Hello [bold]World[/bold][/title]",
			contains: []string{"Hello", "World"},
		},
		{
			name:     "multiple tags",
			input:    "[success]OK[/success] [error]Failed[/error]",
			contains: []string{"OK", "Failed"},
		},
		{
			name:     "powerup tags",
			input:    "[symlink]Creating symlink[/symlink]",
			contains: []string{"Creating symlink"},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result := parser.Render(tt.input)
			for _, expected := range tt.contains {
				if !strings.Contains(result, expected) {
					t.Errorf("Expected output to contain %q, got %q", expected, result)
				}
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
