package homebrew

import (
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/arthur-debert/dodot/pkg/handlers"
	"github.com/arthur-debert/dodot/pkg/testutil"
	"github.com/arthur-debert/dodot/pkg/types"
)

func TestHomebrewHandler_ProcessProvisioningWithConfirmations(t *testing.T) {
	h := NewHomebrewHandler()

	// Create test filesystem with a Brewfile - use real filesystem for hashutil
	tmpDir := t.TempDir()
	brewfilePath := filepath.Join(tmpDir, "Brewfile")
	brewfileContent := `brew "git"
brew "vim"
cask "visual-studio-code"`

	if err := os.WriteFile(brewfilePath, []byte(brewfileContent), 0644); err != nil {
		t.Fatalf("failed to write Brewfile: %v", err)
	}

	matches := []types.RuleMatch{
		{
			Pack:         "test-pack",
			Path:         "Brewfile",
			AbsolutePath: brewfilePath,
		},
	}

	result, err := h.ProcessProvisioningWithConfirmations(matches)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	// Should have actions but no confirmations (provisioning doesn't need confirmation)
	if len(result.Actions) != 1 {
		t.Errorf("got %d actions, want 1", len(result.Actions))
	}
	if len(result.Confirmations) != 0 {
		t.Errorf("got %d confirmations, want 0", len(result.Confirmations))
	}
	if result.HasConfirmations() {
		t.Error("HasConfirmations() = true, want false")
	}

	// Action should be BrewAction
	brewAction, ok := result.Actions[0].(*types.BrewAction)
	if !ok {
		t.Fatalf("action should be BrewAction, got %T", result.Actions[0])
	}
	if brewAction.PackName != "test-pack" {
		t.Errorf("PackName = %q, want %q", brewAction.PackName, "test-pack")
	}
	if brewAction.BrewfilePath != brewfilePath {
		t.Errorf("BrewfilePath = %q, want %q", brewAction.BrewfilePath, brewfilePath)
	}
	if brewAction.Checksum == "" {
		t.Error("Checksum should not be empty")
	}
}

func TestHomebrewHandler_ProcessProvisioning_BackwardCompatibility(t *testing.T) {
	h := NewHomebrewHandler()

	// Create test filesystem with a Brewfile - use real filesystem for hashutil
	tmpDir := t.TempDir()
	brewfilePath := filepath.Join(tmpDir, "Brewfile")
	brewfileContent := `brew "git"`

	if err := os.WriteFile(brewfilePath, []byte(brewfileContent), 0644); err != nil {
		t.Fatalf("failed to write Brewfile: %v", err)
	}

	matches := []types.RuleMatch{
		{
			Pack:         "test-pack",
			Path:         "Brewfile",
			AbsolutePath: brewfilePath,
		},
	}

	actions, err := h.ProcessProvisioning(matches)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	// Should still work the old way
	if len(actions) != 1 {
		t.Errorf("got %d actions, want 1", len(actions))
	}

	brewAction, ok := actions[0].(*types.BrewAction)
	if !ok {
		t.Fatalf("action should be BrewAction, got %T", actions[0])
	}
	if brewAction.PackName != "test-pack" {
		t.Errorf("PackName = %q, want %q", brewAction.PackName, "test-pack")
	}
}

func TestHomebrewHandler_GetClearConfirmations_Disabled(t *testing.T) {
	// Ensure env var is not set
	_ = os.Unsetenv("DODOT_HOMEBREW_UNINSTALL")

	h := NewHomebrewHandler()
	fs := testutil.NewMemoryFS()
	paths := testutil.NewMockPathResolver("/home/test", "/home/test/.config", "/test/data")

	ctx := types.ClearContext{
		Pack:  types.Pack{Name: "test-pack", Path: "/test/pack"},
		FS:    fs,
		Paths: paths,
	}

	confirmations, err := h.GetClearConfirmations(ctx)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	// Should return empty when uninstall is not enabled
	if len(confirmations) != 0 {
		t.Errorf("got %d confirmations, want 0", len(confirmations))
	}
}

func TestHomebrewHandler_GetClearConfirmations_NoState(t *testing.T) {
	// Set env var to enable uninstall
	if err := os.Setenv("DODOT_HOMEBREW_UNINSTALL", "true"); err != nil {
		t.Fatalf("failed to set env var: %v", err)
	}
	defer func() { _ = os.Unsetenv("DODOT_HOMEBREW_UNINSTALL") }()

	h := NewHomebrewHandler()
	fs := testutil.NewMemoryFS()
	paths := testutil.NewMockPathResolver("/home/test", "/home/test/.config", "/test/data")

	ctx := types.ClearContext{
		Pack:  types.Pack{Name: "test-pack", Path: "test/pack"},
		FS:    fs,
		Paths: paths,
	}

	confirmations, err := h.GetClearConfirmations(ctx)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	// Should return empty when no state exists
	if len(confirmations) != 0 {
		t.Errorf("got %d confirmations, want 0", len(confirmations))
	}
}

func TestHomebrewHandler_GetClearConfirmations_WithPackages(t *testing.T) {
	if testing.Short() {
		t.Skip("Skipping test that requires brew in short mode")
	}

	// Check if brew is available (needed for parseBrewfile)
	if _, err := os.Stat("/usr/local/bin/brew"); err != nil {
		if _, err := os.Stat("/opt/homebrew/bin/brew"); err != nil {
			t.Skip("Homebrew not installed, skipping test")
		}
	}

	// Set env var to enable uninstall
	if err := os.Setenv("DODOT_HOMEBREW_UNINSTALL", "true"); err != nil {
		t.Fatalf("failed to set env var: %v", err)
	}
	defer func() { _ = os.Unsetenv("DODOT_HOMEBREW_UNINSTALL") }()

	h := NewHomebrewHandler()
	fs := testutil.NewMemoryFS()
	paths := testutil.NewMockPathResolver("/home/test", "/home/test/.config", "/test/data")

	// Create test pack structure with real filesystem (needed for parseBrewfile)
	tmpDir := t.TempDir()
	brewfileContent := `brew "git"
brew "vim"
cask "visual-studio-code"`
	if err := os.WriteFile(filepath.Join(tmpDir, "Brewfile"), []byte(brewfileContent), 0644); err != nil {
		t.Fatalf("failed to write Brewfile: %v", err)
	}

	// Create state directory and sentinel file in memory FS
	stateDir := "/test/data/dodot/packs/test-pack/homebrew"
	if err := fs.MkdirAll(stateDir, 0755); err != nil {
		t.Fatalf("failed to create state dir: %v", err)
	}
	sentinelPath := filepath.Join(stateDir, "test-pack_Brewfile.sentinel")
	if err := fs.WriteFile(sentinelPath, []byte("checksum|2024-01-01"), 0644); err != nil {
		t.Fatalf("failed to write sentinel: %v", err)
	}

	ctx := types.ClearContext{
		Pack: types.Pack{
			Name: "test-pack",
			Path: tmpDir, // Point to real directory with Brewfile
		},
		FS:    fs,
		Paths: paths,
	}

	confirmations, err := h.GetClearConfirmations(ctx)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	// Should have one confirmation
	if len(confirmations) != 1 {
		t.Fatalf("got %d confirmations, want 1", len(confirmations))
	}

	confirmation := confirmations[0]
	if confirmation.ID != "homebrew-clear-test-pack" {
		t.Errorf("ID = %q, want %q", confirmation.ID, "homebrew-clear-test-pack")
	}
	if confirmation.Pack != "test-pack" {
		t.Errorf("Pack = %q, want %q", confirmation.Pack, "test-pack")
	}
	if confirmation.Handler != "homebrew" {
		t.Errorf("Handler = %q, want %q", confirmation.Handler, "homebrew")
	}
	if confirmation.Operation != "clear" {
		t.Errorf("Operation = %q, want %q", confirmation.Operation, "clear")
	}
	if confirmation.Title != "Uninstall Homebrew packages" {
		t.Errorf("Title = %q, want %q", confirmation.Title, "Uninstall Homebrew packages")
	}
	if !strings.Contains(confirmation.Description, "Uninstall Homebrew packages") {
		t.Errorf("Description should contain 'Uninstall Homebrew packages', got %q", confirmation.Description)
	}
	if confirmation.Default {
		t.Error("Default = true, want false")
	}

	// Should have items describing packages
	if len(confirmation.Items) == 0 {
		t.Error("Items should not be empty")
	}
}

func TestHomebrewHandler_ClearWithConfirmations_NoUninstall(t *testing.T) {
	// Ensure env var is not set
	_ = os.Unsetenv("DODOT_HOMEBREW_UNINSTALL")

	h := NewHomebrewHandler()
	fs := testutil.NewMemoryFS()
	paths := testutil.NewMockPathResolver("/home/test", "/home/test/.config", "/test/data")

	ctx := types.ClearContext{
		Pack:  types.Pack{Name: "test-pack", Path: "test/pack"},
		FS:    fs,
		Paths: paths,
	}

	// Should fall back to basic clear when uninstall is not enabled
	items, err := h.ClearWithConfirmations(ctx, nil)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	// Basic clear with no state should return empty
	if len(items) != 0 {
		t.Errorf("got %d items, want 0", len(items))
	}
}

func TestHomebrewHandler_ClearWithConfirmations_UserDeclined(t *testing.T) {
	// Set env var to enable uninstall
	if err := os.Setenv("DODOT_HOMEBREW_UNINSTALL", "true"); err != nil {
		t.Fatalf("failed to set env var: %v", err)
	}
	defer func() { _ = os.Unsetenv("DODOT_HOMEBREW_UNINSTALL") }()

	h := NewHomebrewHandler()
	fs := testutil.NewMemoryFS()
	paths := testutil.NewMockPathResolver("/home/test", "/home/test/.config", "/test/data")

	ctx := types.ClearContext{
		Pack:  types.Pack{Name: "test-pack", Path: "test/pack"},
		FS:    fs,
		Paths: paths,
	}

	// Create confirmation context where user declined
	responses := []types.ConfirmationResponse{
		{ID: "homebrew-clear-test-pack", Approved: false},
	}
	confirmationCtx := types.NewConfirmationContext(responses)

	// Should fall back to basic clear when user declined
	items, err := h.ClearWithConfirmations(ctx, confirmationCtx)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	// Basic clear with no state should return empty
	if len(items) != 0 {
		t.Errorf("got %d items, want 0", len(items))
	}
}

func TestHomebrewHandler_InterfaceCompliance(t *testing.T) {
	h := NewHomebrewHandler()

	// Should implement all expected interfaces
	var _ handlers.ProvisioningHandler = h
	var _ handlers.ProvisioningHandlerWithConfirmations = h
	var _ handlers.Clearable = h
	var _ handlers.ClearableWithConfirmations = h

	// Verify through type assertion
	if _, ok := interface{}(h).(handlers.ProvisioningHandlerWithConfirmations); !ok {
		t.Error("Should implement ProvisioningHandlerWithConfirmations")
	}

	if _, ok := interface{}(h).(handlers.ClearableWithConfirmations); !ok {
		t.Error("Should implement ClearableWithConfirmations")
	}
}

func TestHomebrewHandler_ProcessProvisioningWithConfirmations_MultipleMatches(t *testing.T) {
	h := NewHomebrewHandler()

	// Create test filesystem with multiple Brewfiles - use real filesystem for hashutil
	tmpDir := t.TempDir()

	brewfile1Path := filepath.Join(tmpDir, "pack1", "Brewfile")
	if err := os.MkdirAll(filepath.Dir(brewfile1Path), 0755); err != nil {
		t.Fatalf("failed to create dir: %v", err)
	}
	if err := os.WriteFile(brewfile1Path, []byte(`brew "git"`), 0644); err != nil {
		t.Fatalf("failed to write Brewfile: %v", err)
	}

	brewfile2Path := filepath.Join(tmpDir, "pack2", "Brewfile.dev")
	if err := os.MkdirAll(filepath.Dir(brewfile2Path), 0755); err != nil {
		t.Fatalf("failed to create dir: %v", err)
	}
	if err := os.WriteFile(brewfile2Path, []byte(`cask "visual-studio-code"`), 0644); err != nil {
		t.Fatalf("failed to write Brewfile: %v", err)
	}

	matches := []types.RuleMatch{
		{Pack: "pack1", Path: "Brewfile", AbsolutePath: brewfile1Path},
		{Pack: "pack2", Path: "Brewfile.dev", AbsolutePath: brewfile2Path},
	}

	result, err := h.ProcessProvisioningWithConfirmations(matches)
	if err != nil {
		t.Fatalf("unexpected error: %v", err)
	}

	// Should have 2 actions, no confirmations
	if len(result.Actions) != 2 {
		t.Errorf("got %d actions, want 2", len(result.Actions))
	}
	if len(result.Confirmations) != 0 {
		t.Errorf("got %d confirmations, want 0", len(result.Confirmations))
	}

	// Both actions should be BrewActions
	for i, action := range result.Actions {
		brewAction, ok := action.(*types.BrewAction)
		if !ok {
			t.Errorf("Action %d should be BrewAction, got %T", i, action)
			continue
		}
		if brewAction.PackName == "" {
			t.Errorf("Action %d: PackName should not be empty", i)
		}
		if brewAction.BrewfilePath == "" {
			t.Errorf("Action %d: BrewfilePath should not be empty", i)
		}
		if brewAction.Checksum == "" {
			t.Errorf("Action %d: Checksum should not be empty", i)
		}
	}
}
