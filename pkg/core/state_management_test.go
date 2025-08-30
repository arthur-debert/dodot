package core_test

import (
	"testing"

	"github.com/arthur-debert/dodot/pkg/core"
	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

// Tests for State Management in Core Package

func TestConfirmationCollector_StateManagement(t *testing.T) {
	t.Run("initializes with empty state", func(t *testing.T) {
		// Execute
		collector := core.NewConfirmationCollector()

		// Verify
		assert.Equal(t, 0, collector.Count())
		assert.False(t, collector.HasConfirmations())
		assert.Empty(t, collector.GetAll())
	})

	t.Run("maintains state across additions", func(t *testing.T) {
		// Setup
		collector := core.NewConfirmationCollector()

		// Execute - add confirmations one by one
		conf1 := types.ConfirmationRequest{
			ID:    "conf1",
			Pack:  "vim",
			Title: "First",
		}
		conf2 := types.ConfirmationRequest{
			ID:    "conf2",
			Pack:  "bash",
			Title: "Second",
		}

		err1 := collector.Add(conf1)
		assert.NoError(t, err1)
		assert.Equal(t, 1, collector.Count())
		assert.True(t, collector.HasConfirmations())

		err2 := collector.Add(conf2)
		assert.NoError(t, err2)
		assert.Equal(t, 2, collector.Count())
		assert.True(t, collector.HasConfirmations())

		// Verify - check collected confirmations
		all := collector.GetAll()
		assert.Len(t, all, 2)
	})

	t.Run("prevents duplicate IDs maintaining state integrity", func(t *testing.T) {
		// Setup
		collector := core.NewConfirmationCollector()

		// Execute
		conf1 := types.ConfirmationRequest{ID: "dup", Pack: "p1"}
		conf2 := types.ConfirmationRequest{ID: "dup", Pack: "p2"}
		conf3 := types.ConfirmationRequest{ID: "unique", Pack: "p3"}

		assert.NoError(t, collector.Add(conf1))
		assert.Error(t, collector.Add(conf2))
		assert.NoError(t, collector.Add(conf3))

		// Verify - state should have only 2 confirmations
		assert.Equal(t, 2, collector.Count())
		all := collector.GetAll()
		assert.Len(t, all, 2)
		assert.Equal(t, "dup", all[0].ID)
		assert.Equal(t, "unique", all[1].ID)
	})

	t.Run("GetAll returns sorted confirmations", func(t *testing.T) {
		// Setup
		collector := core.NewConfirmationCollector()

		// Add confirmations in random order
		confirmations := []types.ConfirmationRequest{
			{ID: "1", Pack: "zsh", Handler: "install", Operation: "provision"},
			{ID: "2", Pack: "vim", Handler: "symlink", Operation: "clear"},
			{ID: "3", Pack: "vim", Handler: "symlink", Operation: "provision"},
			{ID: "4", Pack: "vim", Handler: "install", Operation: "provision"},
			{ID: "5", Pack: "bash", Handler: "symlink", Operation: "provision"},
		}

		require.NoError(t, collector.AddMultiple(confirmations))

		// Execute
		sorted := collector.GetAll()

		// Verify - should be sorted by pack, then handler, then operation
		assert.Len(t, sorted, 5)
		assert.Equal(t, "bash", sorted[0].Pack)
		assert.Equal(t, "vim", sorted[1].Pack)
		assert.Equal(t, "install", sorted[1].Handler)
		assert.Equal(t, "vim", sorted[2].Pack)
		assert.Equal(t, "symlink", sorted[2].Handler)
		assert.Equal(t, "clear", sorted[2].Operation)
		assert.Equal(t, "vim", sorted[3].Pack)
		assert.Equal(t, "symlink", sorted[3].Handler)
		assert.Equal(t, "provision", sorted[3].Operation)
		assert.Equal(t, "zsh", sorted[4].Pack)
	})

	t.Run("AddMultiple maintains transactional integrity", func(t *testing.T) {
		// Setup
		collector := core.NewConfirmationCollector()

		// Add initial confirmation
		assert.NoError(t, collector.Add(types.ConfirmationRequest{
			ID:   "existing",
			Pack: "test",
		}))

		// Try to add multiple with duplicate
		confirmations := []types.ConfirmationRequest{
			{ID: "new1", Pack: "test"},
			{ID: "new2", Pack: "test"},
			{ID: "existing", Pack: "test"}, // duplicate
			{ID: "new3", Pack: "test"},
		}

		// Execute
		err := collector.AddMultiple(confirmations)

		// Verify - should fail at duplicate, but first two should be added
		assert.Error(t, err)
		assert.Contains(t, err.Error(), "duplicate confirmation ID: existing")
		assert.Equal(t, 3, collector.Count()) // existing + new1 + new2

		// Check that new3 was not added (failed before it)
		all := collector.GetAll()
		for _, conf := range all {
			assert.NotEqual(t, "new3", conf.ID)
		}
	})
}

func TestActionGenerationResult_StateManagement(t *testing.T) {
	t.Run("HasConfirmations reflects confirmation state", func(t *testing.T) {
		// Test empty state
		result1 := core.ActionGenerationResult{}
		assert.False(t, result1.HasConfirmations())

		// Test with confirmations
		result2 := core.ActionGenerationResult{
			Confirmations: []types.ConfirmationRequest{
				{ID: "test"},
			},
		}
		assert.True(t, result2.HasConfirmations())

		// Test with empty slice
		result3 := core.ActionGenerationResult{
			Confirmations: []types.ConfirmationRequest{},
		}
		assert.False(t, result3.HasConfirmations())
	})

	t.Run("maintains separate action and confirmation state", func(t *testing.T) {
		// Setup
		result := core.ActionGenerationResult{
			Actions: []types.Action{
				&types.LinkAction{PackName: "vim"},
				&types.RunScriptAction{PackName: "tools"},
			},
			Confirmations: []types.ConfirmationRequest{
				{ID: "conf1", Pack: "vim"},
			},
		}

		// Verify
		assert.Len(t, result.Actions, 2)
		assert.Len(t, result.Confirmations, 1)
		assert.True(t, result.HasConfirmations())
	})
}

func TestGetActionsWithConfirmations_StateAccumulation(t *testing.T) {
	t.Run("accumulates actions from multiple handlers", func(t *testing.T) {
		// Setup - matches for multiple handlers (avoiding handlers that require files)
		matches := []types.RuleMatch{
			{HandlerName: "symlink", Pack: "vim", Path: ".vimrc", AbsolutePath: "/test/.vimrc"},
			{HandlerName: "symlink", Pack: "bash", Path: ".bashrc", AbsolutePath: "/test/.bashrc"},
			{HandlerName: "path", Pack: "tools", Path: "bin", AbsolutePath: "/test/bin"},
			// Unknown handler to test skipping
			{HandlerName: "unknown", Pack: "test", Path: "file"},
		}

		// Execute
		result, err := core.GetActionsWithConfirmations(matches)

		// Verify
		assert.NoError(t, err)
		assert.NotEmpty(t, result.Actions)
		// Should have actions from symlink and path handlers (unknown skipped)
		assert.GreaterOrEqual(t, len(result.Actions), 2)
	})

	t.Run("groups matches by handler before processing", func(t *testing.T) {
		// Setup - multiple matches for same handler
		matches := []types.RuleMatch{
			{HandlerName: "symlink", Pack: "vim", Path: "file1", AbsolutePath: "/test/file1"},
			{HandlerName: "symlink", Pack: "vim", Path: "file2", AbsolutePath: "/test/file2"},
			{HandlerName: "symlink", Pack: "vim", Path: "file3", AbsolutePath: "/test/file3"},
		}

		// Execute
		result, err := core.GetActionsWithConfirmations(matches)

		// Verify
		assert.NoError(t, err)
		assert.NotEmpty(t, result.Actions)
		// All matches should be processed together by the symlink handler
	})
}

func TestStateConsistency_MultipleOperations(t *testing.T) {
	t.Run("maintains consistency across filter operations", func(t *testing.T) {
		// Setup - mixed action types
		originalActions := []types.Action{
			&types.LinkAction{PackName: "vim"},
			&types.RunScriptAction{PackName: "tools"},
			&types.BrewAction{PackName: "homebrew"},
			&types.AddToPathAction{PackName: "bash"},
			&types.RecordProvisioningAction{PackName: "test"},
		}

		// Execute multiple filter operations
		configActions := core.FilterConfigurationActions(originalActions)
		codeActions := core.FilterCodeExecutionActions(originalActions)

		// Verify - original should be unchanged
		assert.Len(t, originalActions, 5)

		// Verify filtered results
		assert.Len(t, configActions, 2) // LinkAction, AddToPathAction
		assert.Len(t, codeActions, 3)   // RunScriptAction, BrewAction, RecordProvisioningAction

		// Verify no overlap between filters
		for _, configAction := range configActions {
			for _, codeAction := range codeActions {
				assert.NotEqual(t, configAction, codeAction)
			}
		}
	})
}

func TestConfirmationCollector_ThreadSafety(t *testing.T) {
	t.Run("sequential operations maintain state integrity", func(t *testing.T) {
		// Note: ConfirmationCollector is not thread-safe by design
		// This test ensures sequential operations work correctly

		collector := core.NewConfirmationCollector()

		// Rapid sequential additions
		for i := 0; i < 100; i++ {
			conf := types.ConfirmationRequest{
				ID:   string(rune('a'+i%26)) + string(rune('0'+i/26)),
				Pack: "test",
			}
			err := collector.Add(conf)
			assert.NoError(t, err)
		}

		// Verify
		assert.Equal(t, 100, collector.Count())
		assert.True(t, collector.HasConfirmations())

		// Verify all confirmations are present
		all := collector.GetAll()
		assert.Len(t, all, 100)

		// Verify uniqueness
		seen := make(map[string]bool)
		for _, conf := range all {
			assert.False(t, seen[conf.ID], "duplicate ID found: %s", conf.ID)
			seen[conf.ID] = true
		}
	})
}

// Test helper functions that maintain state
func TestGroupMatchesByHandler_StateGrouping(t *testing.T) {
	t.Run("correctly groups matches by handler", func(t *testing.T) {
		// Setup
		matches := []types.RuleMatch{
			{HandlerName: "symlink", Pack: "p1", Path: "f1"},
			{HandlerName: "homebrew", Pack: "p2", Path: "f2"},
			{HandlerName: "symlink", Pack: "p3", Path: "f3"},
			{HandlerName: "install", Pack: "p4", Path: "f4"},
			{HandlerName: "homebrew", Pack: "p5", Path: "f5"},
			{HandlerName: "", Pack: "p6", Path: "f6"}, // empty handler
		}

		// Execute
		grouped := core.GroupMatchesByHandler(matches)

		// Verify
		assert.Len(t, grouped, 3) // empty handler is ignored
		assert.Len(t, grouped["symlink"], 2)
		assert.Len(t, grouped["homebrew"], 2)
		assert.Len(t, grouped["install"], 1)

		// Verify specific groupings
		assert.Equal(t, "p1", grouped["symlink"][0].Pack)
		assert.Equal(t, "p3", grouped["symlink"][1].Pack)
		assert.Equal(t, "p2", grouped["homebrew"][0].Pack)
		assert.Equal(t, "p5", grouped["homebrew"][1].Pack)
	})

	t.Run("preserves order within groups", func(t *testing.T) {
		// Setup
		matches := []types.RuleMatch{
			{HandlerName: "h", Pack: "p1", Path: "f1"},
			{HandlerName: "h", Pack: "p2", Path: "f2"},
			{HandlerName: "h", Pack: "p3", Path: "f3"},
		}

		// Execute
		grouped := core.GroupMatchesByHandler(matches)

		// Verify order is preserved
		assert.Len(t, grouped["h"], 3)
		assert.Equal(t, "p1", grouped["h"][0].Pack)
		assert.Equal(t, "p2", grouped["h"][1].Pack)
		assert.Equal(t, "p3", grouped["h"][2].Pack)
	})
}
