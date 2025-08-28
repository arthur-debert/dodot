// pkg/types/confirmation_test.go
// TEST TYPE: Unit Tests
// DEPENDENCIES: None
// PURPOSE: Test confirmation request structures

package types_test

import (
	"testing"

	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/stretchr/testify/assert"
)

func TestNewProcessingResult(t *testing.T) {
	actions := []types.Action{
		&types.LinkAction{PackName: "test", SourceFile: "file1"},
		&types.LinkAction{PackName: "test", SourceFile: "file2"},
	}

	result := types.NewProcessingResult(actions)

	assert.Len(t, result.Actions, 2)
	assert.Len(t, result.Confirmations, 0)
}

func TestProcessingResultWithConfirmations(t *testing.T) {
	actions := []types.Action{
		&types.LinkAction{PackName: "test", SourceFile: "file1"},
	}

	result := types.ProcessingResult{
		Actions: actions,
		Confirmations: []types.ConfirmationRequest{
			{
				ID:          "test-confirmation",
				Pack:        "test-pack",
				Handler:     "homebrew",
				Operation:   "clear",
				Title:       "Uninstall packages",
				Description: "Remove homebrew packages",
				Items:       []string{"git", "vim"},
				Default:     false,
			},
		},
	}

	assert.Len(t, result.Actions, 1)
	assert.Len(t, result.Confirmations, 1)

	confirmation := result.Confirmations[0]
	assert.Equal(t, "test-confirmation", confirmation.ID)
	assert.Equal(t, "test-pack", confirmation.Pack)
	assert.Equal(t, "homebrew", confirmation.Handler)
	assert.Equal(t, "clear", confirmation.Operation)
	assert.Equal(t, "Uninstall packages", confirmation.Title)
	assert.Equal(t, "Remove homebrew packages", confirmation.Description)
	assert.Equal(t, []string{"git", "vim"}, confirmation.Items)
	assert.False(t, confirmation.Default)
}
