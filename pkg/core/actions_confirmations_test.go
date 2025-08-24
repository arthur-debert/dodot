package core

import (
	"testing"

	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/stretchr/testify/assert"
)

func TestActionGenerationResult_HasConfirmations(t *testing.T) {
	// Test with no confirmations
	result := ActionGenerationResult{
		Actions:       []types.Action{},
		Confirmations: []types.ConfirmationRequest{},
	}
	assert.False(t, result.HasConfirmations())

	// Test with confirmations
	result = ActionGenerationResult{
		Actions: []types.Action{},
		Confirmations: []types.ConfirmationRequest{
			{ID: "test-confirmation"},
		},
	}
	assert.True(t, result.HasConfirmations())
}

func TestGetActions_BackwardCompatibility(t *testing.T) {
	// Test that the old GetActions function still works even with no matches
	matches := []types.TriggerMatch{}

	// Test the function
	actions, err := GetActions(matches)
	assert.NoError(t, err)
	assert.Empty(t, actions)
}

func TestGetActionsWithConfirmations_EmptyMatches(t *testing.T) {
	// Test with empty matches
	matches := []types.TriggerMatch{}

	result, err := GetActionsWithConfirmations(matches)
	assert.NoError(t, err)
	assert.Empty(t, result.Actions)
	assert.Empty(t, result.Confirmations)
	assert.False(t, result.HasConfirmations())
}
