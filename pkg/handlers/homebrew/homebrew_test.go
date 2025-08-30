package homebrew_test

import (
	"fmt"
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/arthur-debert/dodot/pkg/handlers/homebrew"
	"github.com/arthur-debert/dodot/pkg/operations"
	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestHandler_ToOperations(t *testing.T) {
	// Create temp directory for test Brewfiles
	tempDir := t.TempDir()

	// Create test Brewfile
	brewfileContent := `# Test Brewfile
brew "git"
brew "jq"
cask "visual-studio-code"
`
	brewfilePath := filepath.Join(tempDir, "Brewfile")
	err := os.WriteFile(brewfilePath, []byte(brewfileContent), 0644)
	require.NoError(t, err)

	handler := homebrew.NewHandler()

	tests := []struct {
		name     string
		matches  []types.RuleMatch
		wantOps  int
		wantErr  bool
		checkOps func(*testing.T, []operations.Operation)
	}{
		{
			name: "single Brewfile creates one RunCommand operation",
			matches: []types.RuleMatch{
				{
					Pack:         "dev-tools",
					Path:         "Brewfile",
					AbsolutePath: brewfilePath,
					HandlerName:  "homebrew",
				},
			},
			wantOps: 1,
			checkOps: func(t *testing.T, ops []operations.Operation) {
				assert.Equal(t, operations.RunCommand, ops[0].Type)
				assert.Equal(t, "dev-tools", ops[0].Pack)
				assert.Equal(t, "homebrew", ops[0].Handler)
				assert.Contains(t, ops[0].Command, "brew bundle")
				assert.Contains(t, ops[0].Command, brewfilePath)
				assert.NotEmpty(t, ops[0].Sentinel)
				assert.Contains(t, ops[0].Sentinel, "dev-tools_Brewfile-")
			},
		},
		{
			name: "multiple Brewfiles create multiple operations",
			matches: []types.RuleMatch{
				{
					Pack:         "dev-tools",
					Path:         "Brewfile",
					AbsolutePath: brewfilePath,
					HandlerName:  "homebrew",
				},
				{
					Pack:         "apps",
					Path:         "apps/Brewfile",
					AbsolutePath: brewfilePath,
					HandlerName:  "homebrew",
				},
			},
			wantOps: 2,
			checkOps: func(t *testing.T, ops []operations.Operation) {
				// Both should be RunCommand operations
				for _, op := range ops {
					assert.Equal(t, operations.RunCommand, op.Type)
					assert.Equal(t, "homebrew", op.Handler)
					assert.Contains(t, op.Command, "brew bundle")
				}

				// Check specific packs
				assert.Equal(t, "dev-tools", ops[0].Pack)
				assert.Contains(t, ops[0].Sentinel, "dev-tools_Brewfile-")

				assert.Equal(t, "apps", ops[1].Pack)
				assert.Contains(t, ops[1].Sentinel, "apps_Brewfile-")
			},
		},
		{
			name:    "empty matches returns empty operations",
			matches: []types.RuleMatch{},
			wantOps: 0,
		},
		{
			name: "non-existent Brewfile path returns error",
			matches: []types.RuleMatch{
				{
					Pack:         "badpack",
					Path:         "missing/Brewfile",
					AbsolutePath: "/non/existent/Brewfile",
					HandlerName:  "homebrew",
				},
			},
			wantErr: true,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			ops, err := handler.ToOperations(tt.matches)

			if tt.wantErr {
				assert.Error(t, err)
				return
			}

			require.NoError(t, err)
			assert.Len(t, ops, tt.wantOps)

			if tt.checkOps != nil {
				tt.checkOps(t, ops)
			}
		})
	}
}

func TestHandler_GetMetadata(t *testing.T) {
	handler := homebrew.NewHandler()
	meta := handler.GetMetadata()

	assert.Equal(t, "Processes Brewfiles to install Homebrew packages", meta.Description)
	assert.False(t, meta.RequiresConfirm)
	assert.False(t, meta.CanRunMultiple) // Only run once per checksum
}

func TestHandler_DeterministicSentinel(t *testing.T) {
	// Create a test Brewfile with known content
	tempDir := t.TempDir()
	brewfileContent := "brew \"git\"\n"
	brewfilePath := filepath.Join(tempDir, "Brewfile")
	err := os.WriteFile(brewfilePath, []byte(brewfileContent), 0644)
	require.NoError(t, err)

	handler := homebrew.NewHandler()

	match := types.RuleMatch{
		Pack:         "test",
		Path:         "Brewfile",
		AbsolutePath: brewfilePath,
		HandlerName:  "homebrew",
	}

	// Generate operations multiple times
	ops1, err := handler.ToOperations([]types.RuleMatch{match})
	require.NoError(t, err)

	ops2, err := handler.ToOperations([]types.RuleMatch{match})
	require.NoError(t, err)

	// Sentinels should be identical for same content
	assert.Equal(t, ops1[0].Sentinel, ops2[0].Sentinel)

	// Modify the Brewfile
	newContent := "brew \"git\"\nbrew \"jq\"\n"
	err = os.WriteFile(brewfilePath, []byte(newContent), 0644)
	require.NoError(t, err)

	ops3, err := handler.ToOperations([]types.RuleMatch{match})
	require.NoError(t, err)

	// Sentinel should be different after modification
	assert.NotEqual(t, ops1[0].Sentinel, ops3[0].Sentinel)
}

func TestHandler_CommandFormat(t *testing.T) {
	tempDir := t.TempDir()

	// Test with path containing spaces
	brewfilePath := filepath.Join(tempDir, "my brew file")
	err := os.WriteFile(brewfilePath, []byte("brew \"git\"\n"), 0644)
	require.NoError(t, err)

	handler := homebrew.NewHandler()

	match := types.RuleMatch{
		Pack:         "test",
		Path:         "my brew file",
		AbsolutePath: brewfilePath,
		HandlerName:  "homebrew",
	}

	ops, err := handler.ToOperations([]types.RuleMatch{match})
	require.NoError(t, err)

	// Command should properly quote the path
	expectedCommand := fmt.Sprintf("brew bundle --file='%s'", brewfilePath)
	assert.Equal(t, expectedCommand, ops[0].Command)
}

func TestHandler_GetTemplateContent(t *testing.T) {
	handler := homebrew.NewHandler()

	// Template should not be empty
	template := handler.GetTemplateContent()
	assert.NotEmpty(t, template)

	// Should contain Brewfile markers
	assert.Contains(t, template, "brew")
}

func TestHandler_GetClearConfirmation(t *testing.T) {
	handler := homebrew.NewHandler()

	ctx := types.ClearContext{
		Pack: types.Pack{
			Name: "test",
		},
		DryRun: false,
	}

	// Without DODOT_HOMEBREW_UNINSTALL, no confirmation needed
	confirmation := handler.GetClearConfirmation(ctx)
	assert.Nil(t, confirmation)

	// With DODOT_HOMEBREW_UNINSTALL=true, confirmation is needed
	_ = os.Setenv("DODOT_HOMEBREW_UNINSTALL", "true")
	defer func() { _ = os.Unsetenv("DODOT_HOMEBREW_UNINSTALL") }()

	confirmation = handler.GetClearConfirmation(ctx)
	assert.NotNil(t, confirmation)
	assert.Equal(t, "homebrew_uninstall_test", confirmation.ID)
	assert.Contains(t, confirmation.Title, "Uninstall Homebrew packages")
	assert.Contains(t, confirmation.Description, "test pack")
}

func TestHandler_FormatClearedItem(t *testing.T) {
	handler := homebrew.NewHandler()

	item := types.ClearedItem{
		Type:        "homebrew_state",
		Path:        "/some/path",
		Description: "Default description",
	}

	// Test dry run without uninstall enabled
	formatted := handler.FormatClearedItem(item, true)
	assert.Contains(t, formatted, "Would remove Homebrew state")
	assert.Contains(t, formatted, "DODOT_HOMEBREW_UNINSTALL=true")

	// Test actual run without uninstall enabled
	formatted = handler.FormatClearedItem(item, false)
	assert.Contains(t, formatted, "Removing Homebrew state")
	assert.Contains(t, formatted, "DODOT_HOMEBREW_UNINSTALL=true")

	// Test with uninstall enabled
	_ = os.Setenv("DODOT_HOMEBREW_UNINSTALL", "true")
	defer func() { _ = os.Unsetenv("DODOT_HOMEBREW_UNINSTALL") }()

	formatted = handler.FormatClearedItem(item, true)
	assert.Contains(t, formatted, "Would uninstall Homebrew packages")

	formatted = handler.FormatClearedItem(item, false)
	assert.Contains(t, formatted, "Uninstalling Homebrew packages")
}

func TestHandler_SentinelFormat(t *testing.T) {
	// This test verifies the sentinel format includes pack name
	tempDir := t.TempDir()
	brewfilePath := filepath.Join(tempDir, "Brewfile")
	err := os.WriteFile(brewfilePath, []byte("brew \"git\"\n"), 0644)
	require.NoError(t, err)

	handler := homebrew.NewHandler()

	match := types.RuleMatch{
		Pack:         "mypack",
		Path:         "tools/Brewfile",
		AbsolutePath: brewfilePath,
		HandlerName:  "homebrew",
	}

	ops, err := handler.ToOperations([]types.RuleMatch{match})
	require.NoError(t, err)

	// Sentinel should include pack name and basename
	assert.True(t, strings.HasPrefix(ops[0].Sentinel, "mypack_Brewfile-"))
}
