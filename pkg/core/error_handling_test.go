package core_test

import (
	"errors"
	"fmt"
	"os"
	"testing"

	"github.com/arthur-debert/dodot/pkg/core"
	dodoterrors "github.com/arthur-debert/dodot/pkg/errors"
	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

// Tests for Error Handling in Core Package

func TestValidateDotfilesRoot_ErrorHandling(t *testing.T) {
	t.Run("returns error for empty root", func(t *testing.T) {
		// Execute
		err := core.ValidateDotfilesRoot("")

		// Verify
		assert.Error(t, err)
		assert.Contains(t, err.Error(), "dotfiles root cannot be empty")

		// Check error code
		var dodotErr *dodoterrors.DodotError
		if errors.As(err, &dodotErr) {
			assert.Equal(t, dodoterrors.ErrInvalidInput, dodotErr.Code)
		}
	})

	t.Run("returns error for non-existent directory", func(t *testing.T) {
		// Execute
		err := core.ValidateDotfilesRoot("/non/existent/directory")

		// Verify
		assert.Error(t, err)
	})

	t.Run("returns error for file instead of directory", func(t *testing.T) {
		// Setup - create a file
		tempFile := t.TempDir() + "/file.txt"
		require.NoError(t, os.WriteFile(tempFile, []byte("content"), 0644))

		// Execute
		err := core.ValidateDotfilesRoot(tempFile)

		// Verify
		assert.Error(t, err)
		assert.Contains(t, err.Error(), "not a directory")
	})
}

func TestFindPack_ErrorHandling(t *testing.T) {
	t.Run("returns pack not found error", func(t *testing.T) {
		// Setup
		tempDir := t.TempDir()

		// Execute
		pack, err := core.FindPack(tempDir, "nonexistent")

		// Verify
		assert.Nil(t, pack)
		assert.Error(t, err)
		assert.Contains(t, err.Error(), "pack(s) not found")

		// Check error code
		var dodotErr *dodoterrors.DodotError
		if errors.As(err, &dodotErr) {
			assert.Equal(t, dodoterrors.ErrPackNotFound, dodotErr.Code)
		}
	})
}

func TestGetActionsWithConfirmations_ErrorHandling(t *testing.T) {
	t.Run("handles handler creation failure gracefully", func(t *testing.T) {
		// Setup - matches with non-existent handler
		matches := []types.RuleMatch{
			{HandlerName: "nonexistent", Pack: "test", Path: "file"},
		}

		// Execute
		result, err := core.GetActionsWithConfirmations(matches)

		// Verify - should not error, just skip the handler
		assert.NoError(t, err)
		assert.Empty(t, result.Actions)
		assert.Empty(t, result.Confirmations)
	})

	t.Run("processes valid handlers successfully", func(t *testing.T) {
		// Setup - matches with valid handler
		matches := []types.RuleMatch{
			{
				HandlerName:  "symlink",
				Pack:         "test",
				Path:         ".vimrc",
				AbsolutePath: "/test/.vimrc",
			},
		}

		// Execute
		result, err := core.GetActionsWithConfirmations(matches)

		// Verify - should process successfully
		assert.NoError(t, err)
		assert.NotEmpty(t, result.Actions)
	})
}

func TestFilterProvisioningActions_ErrorHandling(t *testing.T) {
	t.Run("propagates datastore errors", func(t *testing.T) {
		// Setup
		actions := []types.Action{
			&types.RunScriptAction{
				PackName:     "test",
				ScriptPath:   "script.sh",
				SentinelName: "test-sentinel",
				Checksum:     "abc123",
			},
		}

		dataStore := &mockDataStore{
			err: fmt.Errorf("datastore error"),
		}

		// Execute
		filtered, err := core.FilterProvisioningActions(actions, false, dataStore)

		// Verify
		assert.Error(t, err)
		assert.Contains(t, err.Error(), "failed to check provisioning status")
		assert.Contains(t, err.Error(), "datastore error")
		assert.Nil(t, filtered)
	})

	t.Run("handles BrewAction datastore errors", func(t *testing.T) {
		// Setup
		actions := []types.Action{
			&types.BrewAction{
				PackName:     "tools",
				BrewfilePath: "Brewfile",
				Checksum:     "def456",
			},
		}

		dataStore := &mockDataStore{
			err: fmt.Errorf("brew check failed"),
		}

		// Execute
		filtered, err := core.FilterProvisioningActions(actions, false, dataStore)

		// Verify
		assert.Error(t, err)
		assert.Contains(t, err.Error(), "failed to check provisioning status")
		assert.Contains(t, err.Error(), "brew check failed")
		assert.Nil(t, filtered)
	})
}

// Test ConfirmationCollector error handling
func TestConfirmationCollector_ErrorHandling(t *testing.T) {
	t.Run("returns error for duplicate confirmation ID", func(t *testing.T) {
		// Setup
		collector := core.NewConfirmationCollector()

		confirmation1 := types.ConfirmationRequest{
			ID:          "duplicate-id",
			Pack:        "test",
			Handler:     "install",
			Operation:   "provision",
			Title:       "First confirmation",
			Description: "First description",
		}
		confirmation2 := types.ConfirmationRequest{
			ID:          "duplicate-id",
			Pack:        "test",
			Handler:     "install",
			Operation:   "provision",
			Title:       "Second confirmation",
			Description: "Second description",
		}

		// Execute
		err1 := collector.Add(confirmation1)
		err2 := collector.Add(confirmation2)

		// Verify
		assert.NoError(t, err1)
		assert.Error(t, err2)
		assert.Contains(t, err2.Error(), "duplicate confirmation ID: duplicate-id")
	})

	t.Run("AddMultiple fails on first duplicate", func(t *testing.T) {
		// Setup
		collector := core.NewConfirmationCollector()

		confirmations := []types.ConfirmationRequest{
			{ID: "id1", Pack: "test", Handler: "h1", Title: "First"},
			{ID: "id2", Pack: "test", Handler: "h2", Title: "Second"},
			{ID: "id1", Pack: "test", Handler: "h3", Title: "Duplicate"},
			{ID: "id3", Pack: "test", Handler: "h4", Title: "Third"},
		}

		// Execute
		err := collector.AddMultiple(confirmations)

		// Verify
		assert.Error(t, err)
		assert.Contains(t, err.Error(), "duplicate confirmation ID: id1")

		// Check that it stopped at the duplicate
		assert.Equal(t, 2, collector.Count())
	})
}

// Mock dialog that returns an error
type errorDialog struct{}

func (d *errorDialog) PresentConfirmations(confirmations []types.ConfirmationRequest) ([]types.ConfirmationResponse, error) {
	return nil, fmt.Errorf("dialog presentation failed")
}

func TestCollectAndProcessConfirmations_ErrorHandling(t *testing.T) {
	t.Run("propagates dialog errors", func(t *testing.T) {
		// Setup
		confirmations := []types.ConfirmationRequest{
			{ID: "test", Pack: "test", Title: "Test confirmation"},
		}

		dialog := &errorDialog{}

		// Execute
		context, err := core.CollectAndProcessConfirmations(confirmations, dialog)

		// Verify
		assert.Error(t, err)
		assert.Contains(t, err.Error(), "failed to collect confirmation responses")
		assert.Contains(t, err.Error(), "dialog presentation failed")
		assert.Nil(t, context)
	})

	t.Run("handles empty confirmations", func(t *testing.T) {
		// Setup
		confirmations := []types.ConfirmationRequest{}

		// Mock dialog that would error if called
		dialog := &errorDialog{}

		// Execute
		context, err := core.CollectAndProcessConfirmations(confirmations, dialog)

		// Verify - should succeed with nil context (no confirmations needed)
		assert.NoError(t, err)
		assert.Nil(t, context)
	})
}

// Test edge cases and boundary conditions
func TestEdgeCases(t *testing.T) {
	t.Run("GetMatchesFS with nil filesystem", func(t *testing.T) {
		// Setup
		packs := []types.Pack{
			{Name: "test", Path: "/test"},
		}

		// Execute
		_, err := core.GetMatchesFS(packs, nil)

		// Verify - should handle nil filesystem
		// The actual behavior depends on the rules system
		assert.Error(t, err) // Expecting error because pack doesn't exist
	})

	t.Run("FilterMatchesByHandlerCategory with nil matches", func(t *testing.T) {
		// Execute
		filtered := core.FilterMatchesByHandlerCategory(nil, true, true)

		// Verify
		assert.Empty(t, filtered)
	})

	t.Run("GroupMatchesByHandler with nil matches", func(t *testing.T) {
		// Execute
		result := core.GroupMatchesByHandler(nil)

		// Verify
		assert.NotNil(t, result)
		assert.Empty(t, result)
	})
}

// Test panic recovery in MustInitialize
func TestMustInitialize_PanicHandling(t *testing.T) {
	t.Run("panics on initialization error", func(t *testing.T) {
		// Since Initialize() currently always returns nil,
		// we can't test the panic behavior without modifying the code
		// This is a placeholder for when Initialize() can actually fail
		t.Skip("Initialize() currently always returns nil")
	})
}

// Test initialization success
func TestInitialize_Success(t *testing.T) {
	t.Run("initializes successfully", func(t *testing.T) {
		// Execute
		err := core.Initialize()

		// Verify
		assert.NoError(t, err)
	})

	t.Run("MustInitialize does not panic on success", func(t *testing.T) {
		// Execute - should not panic
		assert.NotPanics(t, func() {
			core.MustInitialize()
		})
	})
}
