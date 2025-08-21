package homebrew

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/arthur-debert/dodot/pkg/testutil"
	"github.com/arthur-debert/dodot/pkg/types"
)

func TestHomebrewHandler_Basic(t *testing.T) {
	handler := NewHomebrewHandler()

	testutil.AssertEqual(t, HomebrewHandlerName, handler.Name())
	testutil.AssertEqual(t, "Processes Brewfiles to install Homebrew packages", handler.Description())
	testutil.AssertEqual(t, types.RunModeOnce, handler.RunMode())
}

func TestHomebrewHandler_Process(t *testing.T) {
	// Create test files
	tmpDir := testutil.TempDir(t, "homebrew-test")

	// Create a test Brewfile
	brewfilePath := filepath.Join(tmpDir, "Brewfile")
	brewfileContent := `brew "git"
brew "node"
cask "visual-studio-code"`
	err := os.WriteFile(brewfilePath, []byte(brewfileContent), 0644)
	testutil.AssertNoError(t, err)

	handler := NewHomebrewHandler()

	matches := []types.TriggerMatch{
		{
			Path:         "Brewfile",
			AbsolutePath: brewfilePath,
			Pack:         "tools",
			Priority:     100,
		},
	}

	actions, err := handler.Process(matches)
	testutil.AssertNoError(t, err)
	testutil.AssertEqual(t, 1, len(actions)) // Only brew action now

	// Single action should be brew
	brewAction := actions[0]
	testutil.AssertEqual(t, types.ActionTypeBrew, brewAction.Type)
	testutil.AssertEqual(t, brewfilePath, brewAction.Source)
	testutil.AssertEqual(t, "tools", brewAction.Pack)
	testutil.AssertEqual(t, HomebrewHandlerName, brewAction.HandlerName)
	testutil.AssertEqual(t, 100, brewAction.Priority)
	testutil.AssertContains(t, brewAction.Description, "Install packages from")

	// Check metadata
	testutil.AssertNotNil(t, brewAction.Metadata)
	testutil.AssertEqual(t, "tools", brewAction.Metadata["pack"])
	// Checksum should NOT be in metadata anymore
	_, hasChecksum := brewAction.Metadata["checksum"]
	testutil.AssertFalse(t, hasChecksum, "Checksum should not be in metadata")
}

func TestHomebrewHandler_Process_MultipleMatches(t *testing.T) {
	tmpDir := testutil.TempDir(t, "homebrew-test")

	// Create multiple Brewfiles
	brewfile1 := filepath.Join(tmpDir, "Brewfile1")
	brewfile2 := filepath.Join(tmpDir, "Brewfile2")

	err := os.WriteFile(brewfile1, []byte("brew \"git\""), 0644)
	testutil.AssertNoError(t, err)
	err = os.WriteFile(brewfile2, []byte("brew \"node\""), 0644)
	testutil.AssertNoError(t, err)

	handler := NewHomebrewHandler()

	matches := []types.TriggerMatch{
		{
			Path:         "Brewfile1",
			AbsolutePath: brewfile1,
			Pack:         "pack1",
			Priority:     100,
		},
		{
			Path:         "Brewfile2",
			AbsolutePath: brewfile2,
			Pack:         "pack2",
			Priority:     200,
		},
	}

	actions, err := handler.Process(matches)
	testutil.AssertNoError(t, err)
	testutil.AssertEqual(t, 2, len(actions)) // 2 brew actions only

	// Verify actions are in correct order
	// First brew for pack1
	testutil.AssertEqual(t, types.ActionTypeBrew, actions[0].Type)
	testutil.AssertEqual(t, "pack1", actions[0].Pack)
	testutil.AssertEqual(t, 100, actions[0].Priority)

	// Then brew for pack2
	testutil.AssertEqual(t, types.ActionTypeBrew, actions[1].Type)
	testutil.AssertEqual(t, "pack2", actions[1].Pack)
	testutil.AssertEqual(t, 200, actions[1].Priority)
}

func TestHomebrewHandler_Process_ChecksumError(t *testing.T) {
	handler := NewHomebrewHandler()

	matches := []types.TriggerMatch{
		{
			Path:         "Brewfile",
			AbsolutePath: "/non/existent/file",
			Pack:         "tools",
			Priority:     100,
		},
	}

	// Handler should create action even with non-existent file
	actions, err := handler.Process(matches)
	testutil.AssertNoError(t, err)
	testutil.AssertEqual(t, 1, len(actions))

	// Single action should be brew
	testutil.AssertEqual(t, types.ActionTypeBrew, actions[0].Type)
	testutil.AssertEqual(t, "/non/existent/file", actions[0].Source)
}

func TestHomebrewHandler_ValidateOptions(t *testing.T) {
	handler := NewHomebrewHandler()

	// Brewfile power-up doesn't have options, so any options should be accepted
	err := handler.ValidateOptions(nil)
	testutil.AssertNoError(t, err)

	err = handler.ValidateOptions(map[string]interface{}{})
	testutil.AssertNoError(t, err)

	err = handler.ValidateOptions(map[string]interface{}{
		"some": "option",
	})
	testutil.AssertNoError(t, err)
}
