package display

import (
	"bytes"
	"strings"
	"testing"
	"time"

	"github.com/arthur-debert/dodot/pkg/testutil"
	"github.com/arthur-debert/dodot/pkg/types"
)

func TestDisplayIntegration_FullWorkflow(t *testing.T) {
	// Create a complete execution context as would be created by a command
	ctx := types.NewExecutionContext("deploy", false)

	// Add first pack
	pack1 := &types.Pack{
		Name: "vim",
		Path: "/dotfiles/vim",
	}
	packResult1 := types.NewPackExecutionResult(pack1)

	// Add successful symlink operations
	powerUpResult1 := &types.HandlerResult{
		HandlerName: "symlink",
		Files:       []string{"vimrc", "vim/colors/monokai.vim"},
		Status:      types.StatusReady,
		Message:     "linked to .vimrc",
		Pack:        "vim",
		StartTime:   time.Now(),
		EndTime:     time.Now(),
	}
	packResult1.AddHandlerResult(powerUpResult1)
	packResult1.Complete()

	// Add second pack with mixed results
	pack2 := &types.Pack{
		Name: "shell",
		Path: "/dotfiles/shell",
	}
	packResult2 := types.NewPackExecutionResult(pack2)

	// Add successful operation
	powerUpResult2 := &types.HandlerResult{
		HandlerName: "symlink",
		Files:       []string{"bashrc"},
		Status:      types.StatusReady,
		Message:     "linked to .bashrc",
		Pack:        "shell",
		StartTime:   time.Now(),
		EndTime:     time.Now(),
	}
	packResult2.AddHandlerResult(powerUpResult2)

	// Add failed operation
	powerUpResult3 := &types.HandlerResult{
		HandlerName: "install_script",
		Files:       []string{"install.sh"},
		Status:      types.StatusError,
		Message:     "install script failed: exit status 1",
		Pack:        "shell",
		StartTime:   time.Now(),
		EndTime:     time.Now(),
	}
	packResult2.AddHandlerResult(powerUpResult3)
	packResult2.Complete()

	// Add pack results to context
	ctx.AddPackResult("vim", packResult1)
	ctx.AddPackResult("shell", packResult2)
	ctx.Complete()

	// Render the result
	var buf bytes.Buffer
	renderer := NewTextRenderer(&buf)

	err := renderer.RenderExecutionContext(ctx)
	testutil.AssertNoError(t, err)

	output := buf.String()

	// Verify output contains expected elements
	expectedStrings := []string{
		"deploy",
		"vim [status=",                    // Pack name with status
		"shell [status=",                  // Pack name with status
		"symlink",                         // Handler name
		"linked to $HOME/vimrc [status=",  // Handler-aware message with status
		"linked to $HOME/bashrc [status=", // Handler-aware message with status
		"install_script",                  // Handler name
		"installation failed",             // Handler-aware error message
		"[status=error]",                  // Error status indicator
	}

	for _, expected := range expectedStrings {
		testutil.AssertTrue(t, strings.Contains(output, expected),
			"Expected output to contain '%s', got:\n%s", expected, output)
	}

	// Debug output
	t.Logf("Full output:\n%s", output)

	// Verify pack order (shell should come before vim due to alphabetical sorting)
	shellIndex := strings.Index(output, "shell [status=")
	vimIndex := strings.Index(output, "vim [status=")
	testutil.AssertTrue(t, shellIndex < vimIndex, "Packs should be sorted alphabetically (shell before vim)")
}
