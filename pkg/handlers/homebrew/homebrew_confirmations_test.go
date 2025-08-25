package homebrew

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/arthur-debert/dodot/pkg/testutil"
	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestHomebrewHandler_ProcessProvisioningWithConfirmations(t *testing.T) {
	h := NewHomebrewHandler()

	// Create test filesystem with a Brewfile - use real filesystem for hashutil
	tmpDir := t.TempDir()
	brewfilePath := filepath.Join(tmpDir, "Brewfile")
	brewfileContent := `brew "git"
brew "vim"
cask "visual-studio-code"`

	require.NoError(t, os.WriteFile(brewfilePath, []byte(brewfileContent), 0644))

	matches := []types.TriggerMatch{
		{
			Pack:         "test-pack",
			Path:         "Brewfile",
			AbsolutePath: brewfilePath,
		},
	}

	result, err := h.ProcessProvisioningWithConfirmations(matches)
	require.NoError(t, err)

	// Should have actions but no confirmations (provisioning doesn't need confirmation)
	assert.Len(t, result.Actions, 1)
	assert.Len(t, result.Confirmations, 0)
	assert.False(t, result.HasConfirmations())

	// Action should be BrewAction
	brewAction, ok := result.Actions[0].(*types.BrewAction)
	require.True(t, ok)
	assert.Equal(t, "test-pack", brewAction.PackName)
	assert.Equal(t, brewfilePath, brewAction.BrewfilePath)
	assert.NotEmpty(t, brewAction.Checksum)
}

func TestHomebrewHandler_ProcessProvisioning_BackwardCompatibility(t *testing.T) {
	h := NewHomebrewHandler()

	// Create test filesystem with a Brewfile - use real filesystem for hashutil
	tmpDir := t.TempDir()
	brewfilePath := filepath.Join(tmpDir, "Brewfile")
	brewfileContent := `brew "git"`

	require.NoError(t, os.WriteFile(brewfilePath, []byte(brewfileContent), 0644))

	matches := []types.TriggerMatch{
		{
			Pack:         "test-pack",
			Path:         "Brewfile",
			AbsolutePath: brewfilePath,
		},
	}

	actions, err := h.ProcessProvisioning(matches)
	require.NoError(t, err)

	// Should still work the old way
	assert.Len(t, actions, 1)

	brewAction, ok := actions[0].(*types.BrewAction)
	require.True(t, ok)
	assert.Equal(t, "test-pack", brewAction.PackName)
}

func TestHomebrewHandler_GetClearConfirmations_Disabled(t *testing.T) {
	// Ensure env var is not set
	_ = os.Unsetenv("DODOT_HOMEBREW_UNINSTALL")

	h := NewHomebrewHandler()
	fs := testutil.NewTestFS()

	ctx := types.ClearContext{
		Pack:  types.Pack{Name: "test-pack", Path: "/test/pack"},
		FS:    fs,
		Paths: &mockClearPaths{dataDir: "/test/data"},
	}

	confirmations, err := h.GetClearConfirmations(ctx)
	require.NoError(t, err)

	// Should return empty when uninstall is not enabled
	assert.Empty(t, confirmations)
}

func TestHomebrewHandler_GetClearConfirmations_NoState(t *testing.T) {
	// Set env var to enable uninstall
	require.NoError(t, os.Setenv("DODOT_HOMEBREW_UNINSTALL", "true"))
	defer func() { _ = os.Unsetenv("DODOT_HOMEBREW_UNINSTALL") }()

	h := NewHomebrewHandler()
	fs := testutil.NewTestFS()

	ctx := types.ClearContext{
		Pack:  types.Pack{Name: "test-pack", Path: "test/pack"},
		FS:    fs,
		Paths: &mockClearPaths{dataDir: "test/data"},
	}

	confirmations, err := h.GetClearConfirmations(ctx)
	require.NoError(t, err)

	// Should return empty when no state exists
	assert.Empty(t, confirmations)
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
	require.NoError(t, os.Setenv("DODOT_HOMEBREW_UNINSTALL", "true"))
	defer func() { _ = os.Unsetenv("DODOT_HOMEBREW_UNINSTALL") }()

	h := NewHomebrewHandler()
	fs := testutil.NewTestFS()

	// Create test pack structure
	dataDir := "test/data"
	stateDir := filepath.Join(dataDir, "packs", "test-pack", "homebrew")

	// Create Brewfile with real filesystem (needed for parseBrewfile)
	tmpDir := t.TempDir()
	brewfileContent := `brew "git"
brew "vim"
cask "visual-studio-code"`
	require.NoError(t, os.WriteFile(filepath.Join(tmpDir, "Brewfile"), []byte(brewfileContent), 0644))

	// Create state directory and sentinel file
	require.NoError(t, fs.MkdirAll(stateDir, 0755))
	sentinelPath := filepath.Join(stateDir, "test-pack_Brewfile.sentinel")
	require.NoError(t, fs.WriteFile(sentinelPath, []byte("checksum|2024-01-01"), 0644))

	ctx := types.ClearContext{
		Pack: types.Pack{
			Name: "test-pack",
			Path: tmpDir, // Point to real directory with Brewfile
		},
		FS:    fs,
		Paths: &mockClearPaths{dataDir: dataDir},
	}

	confirmations, err := h.GetClearConfirmations(ctx)
	require.NoError(t, err)

	// Should have one confirmation
	require.Len(t, confirmations, 1)

	confirmation := confirmations[0]
	assert.Equal(t, "homebrew-clear-test-pack", confirmation.ID)
	assert.Equal(t, "test-pack", confirmation.Pack)
	assert.Equal(t, "homebrew", confirmation.Handler)
	assert.Equal(t, "clear", confirmation.Operation)
	assert.Equal(t, "Uninstall Homebrew packages", confirmation.Title)
	assert.Contains(t, confirmation.Description, "Uninstall Homebrew packages")
	assert.False(t, confirmation.Default) // Should default to No

	// Should have items describing packages
	assert.NotEmpty(t, confirmation.Items)
}

func TestHomebrewHandler_ClearWithConfirmations_NoUninstall(t *testing.T) {
	// Ensure env var is not set
	_ = os.Unsetenv("DODOT_HOMEBREW_UNINSTALL")

	h := NewHomebrewHandler()
	fs := testutil.NewTestFS()

	ctx := types.ClearContext{
		Pack:  types.Pack{Name: "test-pack", Path: "test/pack"},
		FS:    fs,
		Paths: &mockClearPaths{dataDir: "test/data"},
	}

	// Should fall back to basic clear when uninstall is not enabled
	items, err := h.ClearWithConfirmations(ctx, nil)
	require.NoError(t, err)

	// Basic clear with no state should return empty
	assert.Empty(t, items)
}

func TestHomebrewHandler_ClearWithConfirmations_UserDeclined(t *testing.T) {
	// Set env var to enable uninstall
	require.NoError(t, os.Setenv("DODOT_HOMEBREW_UNINSTALL", "true"))
	defer func() { _ = os.Unsetenv("DODOT_HOMEBREW_UNINSTALL") }()

	h := NewHomebrewHandler()
	fs := testutil.NewTestFS()

	ctx := types.ClearContext{
		Pack:  types.Pack{Name: "test-pack", Path: "test/pack"},
		FS:    fs,
		Paths: &mockClearPaths{dataDir: "test/data"},
	}

	// Create confirmation context where user declined
	responses := []types.ConfirmationResponse{
		{ID: "homebrew-clear-test-pack", Approved: false},
	}
	confirmationCtx := types.NewConfirmationContext(responses)

	// Should fall back to basic clear when user declined
	items, err := h.ClearWithConfirmations(ctx, confirmationCtx)
	require.NoError(t, err)

	// Basic clear with no state should return empty
	assert.Empty(t, items)
}

func TestHomebrewHandler_InterfaceCompliance(t *testing.T) {
	h := NewHomebrewHandler()

	// Should implement all expected interfaces
	var _ types.ProvisioningHandler = h
	var _ types.ProvisioningHandlerWithConfirmations = h
	var _ types.Clearable = h
	var _ types.ClearableWithConfirmations = h

	// Should be detectable by helper functions
	assert.True(t, types.IsProvisioningHandlerWithConfirmations(h))
	assert.True(t, types.IsClearableWithConfirmations(h))
}

func TestHomebrewHandler_ProcessProvisioningWithConfirmations_MultipleMatches(t *testing.T) {
	h := NewHomebrewHandler()

	// Create test filesystem with multiple Brewfiles - use real filesystem for hashutil
	tmpDir := t.TempDir()

	brewfile1Path := filepath.Join(tmpDir, "pack1", "Brewfile")
	require.NoError(t, os.MkdirAll(filepath.Dir(brewfile1Path), 0755))
	require.NoError(t, os.WriteFile(brewfile1Path, []byte(`brew "git"`), 0644))

	brewfile2Path := filepath.Join(tmpDir, "pack2", "Brewfile.dev")
	require.NoError(t, os.MkdirAll(filepath.Dir(brewfile2Path), 0755))
	require.NoError(t, os.WriteFile(brewfile2Path, []byte(`cask "visual-studio-code"`), 0644))

	matches := []types.TriggerMatch{
		{Pack: "pack1", Path: "Brewfile", AbsolutePath: brewfile1Path},
		{Pack: "pack2", Path: "Brewfile.dev", AbsolutePath: brewfile2Path},
	}

	result, err := h.ProcessProvisioningWithConfirmations(matches)
	require.NoError(t, err)

	// Should have 2 actions, no confirmations
	assert.Len(t, result.Actions, 2)
	assert.Len(t, result.Confirmations, 0)

	// Both actions should be BrewActions
	for i, action := range result.Actions {
		brewAction, ok := action.(*types.BrewAction)
		require.True(t, ok, "Action %d should be BrewAction", i)
		assert.NotEmpty(t, brewAction.PackName)
		assert.NotEmpty(t, brewAction.BrewfilePath)
		assert.NotEmpty(t, brewAction.Checksum)
	}
}
