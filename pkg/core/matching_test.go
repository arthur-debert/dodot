package core_test

import (
	"testing"

	"github.com/arthur-debert/dodot/pkg/core"
	"github.com/arthur-debert/dodot/pkg/handlerpipeline"
	"github.com/arthur-debert/dodot/pkg/testutil"
	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

// Tests for Matching Orchestration

func TestGetMatchesFS(t *testing.T) {
	t.Run("collects matches from multiple packs", func(t *testing.T) {
		// Setup
		env := testutil.NewTestEnvironment(t, testutil.EnvMemoryOnly)

		// Create test packs with rules
		env.SetupPack("vim", testutil.PackConfig{
			Files: map[string]string{
				".vimrc":    "\" vim config",
				".vim/init": "init",
				".dodot.toml": `[[rule]]
match = ".vimrc"
handler = "symlink"

[[rule]]
match = ".vim"
handler = "symlink"`,
			},
		})

		env.SetupPack("bash", testutil.PackConfig{
			Files: map[string]string{
				".bashrc": "# bash config",
				".dodot.toml": `[[rule]]
match = ".bashrc"
handler = "symlink"`,
			},
		})

		// Get packs
		packs, err := core.DiscoverAndSelectPacksFS(env.DotfilesRoot, nil, env.FS)
		require.NoError(t, err)

		// Execute - GetMatchesFS now properly uses the filesystem parameter
		matches, err := core.GetMatchesFS(packs, env.FS)

		// Verify - Now that GetMatchesFS properly uses the filesystem,
		// it should work correctly with the memory filesystem
		require.NoError(t, err)
		assert.NotEmpty(t, matches)

		// Should have matches from both packs
		packNames := make(map[string]bool)
		for _, match := range matches {
			packNames[match.Pack] = true
		}
		assert.Len(t, packNames, 2, "should have matches from both packs")
	})

	t.Run("handles packs without rules", func(t *testing.T) {
		// Setup
		env := testutil.NewTestEnvironment(t, testutil.EnvMemoryOnly)

		// Create pack without rules file
		env.SetupPack("norules", testutil.PackConfig{
			Files: map[string]string{
				"config.txt": "some config",
			},
		})

		// Get packs
		packs, err := core.DiscoverAndSelectPacksFS(env.DotfilesRoot, []string{"norules"}, env.FS)
		require.NoError(t, err)

		// Execute - GetMatchesFS now properly uses the filesystem parameter
		matches, err := core.GetMatchesFS(packs, env.FS)

		// Verify - it should work even without pack-specific rules (uses global rules)
		require.NoError(t, err)
		assert.NotEmpty(t, matches, "should have matches using global rules")
	})

	t.Run("handles empty pack list", func(t *testing.T) {
		// Execute
		matches, err := core.GetMatchesFS([]types.Pack{}, nil)

		// Verify
		require.NoError(t, err)
		assert.Empty(t, matches)
	})
}

func TestFilterMatchesByHandlerCategory(t *testing.T) {
	// Create test matches
	createMatch := func(pack, handler string) types.RuleMatch {
		return types.RuleMatch{
			Pack:        pack,
			HandlerName: handler,
			Path:        "test",
		}
	}

	allMatches := []types.RuleMatch{
		createMatch("vim", "symlink"),    // configuration
		createMatch("vim", "shell"),      // configuration
		createMatch("vim", "path"),       // configuration
		createMatch("tools", "homebrew"), // code execution
		createMatch("tools", "install"),  // code execution
	}

	t.Run("filter only configuration handlers", func(t *testing.T) {
		// Execute
		filtered := handlerpipeline.FilterMatchesByHandlerCategory(allMatches, true, false)

		// Verify
		assert.Len(t, filtered, 3)
		for _, match := range filtered {
			assert.Contains(t, []string{"symlink", "shell", "path"}, match.HandlerName)
		}
	})

	t.Run("filter only code execution handlers", func(t *testing.T) {
		// Execute
		filtered := handlerpipeline.FilterMatchesByHandlerCategory(allMatches, false, true)

		// Verify
		assert.Len(t, filtered, 2)
		for _, match := range filtered {
			assert.Contains(t, []string{"homebrew", "install"}, match.HandlerName)
		}
	})

	t.Run("allow both categories", func(t *testing.T) {
		// Execute
		filtered := handlerpipeline.FilterMatchesByHandlerCategory(allMatches, true, true)

		// Verify
		assert.Equal(t, allMatches, filtered)
	})

	t.Run("filter none when both false", func(t *testing.T) {
		// Execute
		filtered := handlerpipeline.FilterMatchesByHandlerCategory(allMatches, false, false)

		// Verify
		assert.Empty(t, filtered)
	})

	t.Run("handles unknown handler types", func(t *testing.T) {
		// Create matches with unknown handler
		matchesWithUnknown := append(allMatches, createMatch("test", "unknown"))

		// Execute - unknown handlers should be filtered out
		filtered := handlerpipeline.FilterMatchesByHandlerCategory(matchesWithUnknown, true, true)

		// Verify - should only include known handlers
		assert.Len(t, filtered, len(allMatches))
	})
}
