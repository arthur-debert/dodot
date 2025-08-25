package types

import (
	"testing"

	"github.com/stretchr/testify/assert"
)

func TestNewProcessingResult(t *testing.T) {
	actions := []Action{
		&LinkAction{PackName: "test", SourceFile: "file1"},
		&LinkAction{PackName: "test", SourceFile: "file2"},
	}

	result := NewProcessingResult(actions)

	assert.Len(t, result.Actions, 2)
	assert.Len(t, result.Confirmations, 0)
	assert.False(t, result.HasConfirmations())
}

func TestNewProcessingResultWithConfirmations(t *testing.T) {
	actions := []Action{
		&LinkAction{PackName: "test", SourceFile: "file1"},
	}

	confirmations := []ConfirmationRequest{
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
	}

	result := NewProcessingResultWithConfirmations(actions, confirmations)

	assert.Len(t, result.Actions, 1)
	assert.Len(t, result.Confirmations, 1)
	assert.True(t, result.HasConfirmations())

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

func TestProcessingResult_HasConfirmations(t *testing.T) {
	// No confirmations
	result := ProcessingResult{
		Actions:       []Action{},
		Confirmations: []ConfirmationRequest{},
	}
	assert.False(t, result.HasConfirmations())

	// With confirmations
	result = ProcessingResult{
		Actions: []Action{},
		Confirmations: []ConfirmationRequest{
			{ID: "test"},
		},
	}
	assert.True(t, result.HasConfirmations())
}

func TestNewConfirmationContext(t *testing.T) {
	responses := []ConfirmationResponse{
		{ID: "confirm1", Approved: true},
		{ID: "confirm2", Approved: false},
		{ID: "confirm3", Approved: true},
	}

	ctx := NewConfirmationContext(responses)

	assert.NotNil(t, ctx)
	assert.Len(t, ctx.Responses, 3)
	assert.True(t, ctx.Responses["confirm1"])
	assert.False(t, ctx.Responses["confirm2"])
	assert.True(t, ctx.Responses["confirm3"])
}

func TestConfirmationContext_IsApproved(t *testing.T) {
	responses := []ConfirmationResponse{
		{ID: "approved", Approved: true},
		{ID: "denied", Approved: false},
	}

	ctx := NewConfirmationContext(responses)

	assert.True(t, ctx.IsApproved("approved"))
	assert.False(t, ctx.IsApproved("denied"))
	assert.False(t, ctx.IsApproved("nonexistent"))

	// Test nil context
	var nilCtx *ConfirmationContext
	assert.False(t, nilCtx.IsApproved("any"))
}

func TestConfirmationContext_AllApproved(t *testing.T) {
	responses := []ConfirmationResponse{
		{ID: "confirm1", Approved: true},
		{ID: "confirm2", Approved: true},
		{ID: "confirm3", Approved: false},
	}

	ctx := NewConfirmationContext(responses)

	// All approved
	assert.True(t, ctx.AllApproved([]string{"confirm1", "confirm2"}))

	// Some denied
	assert.False(t, ctx.AllApproved([]string{"confirm1", "confirm2", "confirm3"}))

	// Empty list
	assert.True(t, ctx.AllApproved([]string{}))

	// Nonexistent confirmation
	assert.False(t, ctx.AllApproved([]string{"nonexistent"}))

	// Test nil context
	var nilCtx *ConfirmationContext
	assert.False(t, nilCtx.AllApproved([]string{"any"}))
	assert.True(t, nilCtx.AllApproved([]string{})) // Empty list should return true even for nil context
}

func TestConfirmationRequest_Fields(t *testing.T) {
	req := ConfirmationRequest{
		ID:          "test-id",
		Pack:        "test-pack",
		Handler:     "test-handler",
		Operation:   "provision",
		Title:       "Test Confirmation",
		Description: "This is a test confirmation request",
		Items:       []string{"item1", "item2", "item3"},
		Default:     true,
	}

	assert.Equal(t, "test-id", req.ID)
	assert.Equal(t, "test-pack", req.Pack)
	assert.Equal(t, "test-handler", req.Handler)
	assert.Equal(t, "provision", req.Operation)
	assert.Equal(t, "Test Confirmation", req.Title)
	assert.Equal(t, "This is a test confirmation request", req.Description)
	assert.Equal(t, []string{"item1", "item2", "item3"}, req.Items)
	assert.True(t, req.Default)
}

func TestConfirmationResponse_Fields(t *testing.T) {
	resp := ConfirmationResponse{
		ID:       "response-id",
		Approved: true,
	}

	assert.Equal(t, "response-id", resp.ID)
	assert.True(t, resp.Approved)

	resp2 := ConfirmationResponse{
		ID:       "response-id-2",
		Approved: false,
	}

	assert.Equal(t, "response-id-2", resp2.ID)
	assert.False(t, resp2.Approved)
}
