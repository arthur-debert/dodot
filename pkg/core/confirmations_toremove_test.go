package core

import (
	"testing"

	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestNewConfirmationCollector(t *testing.T) {
	collector := NewConfirmationCollector()

	assert.NotNil(t, collector)
	assert.False(t, collector.HasConfirmations())
	assert.Equal(t, 0, collector.Count())
	assert.Empty(t, collector.GetAll())
}

func TestConfirmationCollector_Add(t *testing.T) {
	collector := NewConfirmationCollector()

	confirmation := types.ConfirmationRequest{
		ID:          "test-id",
		Pack:        "test-pack",
		Handler:     "homebrew",
		Operation:   "clear",
		Title:       "Test Confirmation",
		Description: "Test description",
		Items:       []string{"item1", "item2"},
		Default:     false,
	}

	err := collector.Add(confirmation)
	assert.NoError(t, err)

	assert.True(t, collector.HasConfirmations())
	assert.Equal(t, 1, collector.Count())

	all := collector.GetAll()
	assert.Len(t, all, 1)
	assert.Equal(t, confirmation, all[0])
}

func TestConfirmationCollector_Add_DuplicateID(t *testing.T) {
	collector := NewConfirmationCollector()

	confirmation1 := types.ConfirmationRequest{ID: "duplicate-id", Pack: "pack1"}
	confirmation2 := types.ConfirmationRequest{ID: "duplicate-id", Pack: "pack2"}

	err := collector.Add(confirmation1)
	assert.NoError(t, err)

	err = collector.Add(confirmation2)
	assert.Error(t, err)
	assert.Contains(t, err.Error(), "duplicate confirmation ID")

	// Should still have only the first confirmation
	assert.Equal(t, 1, collector.Count())
}

func TestConfirmationCollector_AddMultiple(t *testing.T) {
	collector := NewConfirmationCollector()

	confirmations := []types.ConfirmationRequest{
		{ID: "id1", Pack: "pack1", Handler: "handler1"},
		{ID: "id2", Pack: "pack2", Handler: "handler2"},
		{ID: "id3", Pack: "pack1", Handler: "handler2"},
	}

	err := collector.AddMultiple(confirmations)
	assert.NoError(t, err)

	assert.Equal(t, 3, collector.Count())
	assert.True(t, collector.HasConfirmations())
}

func TestConfirmationCollector_AddMultiple_WithDuplicate(t *testing.T) {
	collector := NewConfirmationCollector()

	confirmations := []types.ConfirmationRequest{
		{ID: "id1", Pack: "pack1"},
		{ID: "id2", Pack: "pack2"},
		{ID: "id1", Pack: "pack3"}, // Duplicate ID
	}

	err := collector.AddMultiple(confirmations)
	assert.Error(t, err)
	assert.Contains(t, err.Error(), "duplicate confirmation ID")

	// Should have stopped at the duplicate, so only 2 confirmations
	assert.Equal(t, 2, collector.Count())
}

func TestConfirmationCollector_GetAll_Sorting(t *testing.T) {
	collector := NewConfirmationCollector()

	// Add confirmations in mixed order
	confirmations := []types.ConfirmationRequest{
		{ID: "id1", Pack: "pack-z", Handler: "handler-b", Operation: "provision"},
		{ID: "id2", Pack: "pack-a", Handler: "handler-z", Operation: "clear"},
		{ID: "id3", Pack: "pack-a", Handler: "handler-a", Operation: "provision"},
		{ID: "id4", Pack: "pack-a", Handler: "handler-a", Operation: "clear"},
		{ID: "id5", Pack: "pack-z", Handler: "handler-a", Operation: "clear"},
	}

	for _, conf := range confirmations {
		require.NoError(t, collector.Add(conf))
	}

	sorted := collector.GetAll()
	require.Len(t, sorted, 5)

	// Should be sorted by pack, then handler, then operation
	expected := []types.ConfirmationRequest{
		{ID: "id4", Pack: "pack-a", Handler: "handler-a", Operation: "clear"},
		{ID: "id3", Pack: "pack-a", Handler: "handler-a", Operation: "provision"},
		{ID: "id2", Pack: "pack-a", Handler: "handler-z", Operation: "clear"},
		{ID: "id5", Pack: "pack-z", Handler: "handler-a", Operation: "clear"},
		{ID: "id1", Pack: "pack-z", Handler: "handler-b", Operation: "provision"},
	}

	for i, expectedConf := range expected {
		assert.Equal(t, expectedConf.ID, sorted[i].ID, "Mismatch at index %d", i)
		assert.Equal(t, expectedConf.Pack, sorted[i].Pack, "Pack mismatch at index %d", i)
		assert.Equal(t, expectedConf.Handler, sorted[i].Handler, "Handler mismatch at index %d", i)
		assert.Equal(t, expectedConf.Operation, sorted[i].Operation, "Operation mismatch at index %d", i)
	}
}

func TestNewConsoleConfirmationDialog(t *testing.T) {
	dialog := NewConsoleConfirmationDialog()
	assert.NotNil(t, dialog)
}

func TestGetHandlerEmoji(t *testing.T) {
	tests := []struct {
		handler  string
		expected string
	}{
		{"homebrew", "🍺"},
		{"symlink", "🔗"},
		{"provision", "🔧"},
		{"shell", "🐚"},
		{"path", "📁"},
		{"unknown", "⚙️"},
		{"", "⚙️"},
		{"HOMEBREW", "🍺"}, // Case insensitive
	}

	for _, tt := range tests {
		t.Run(tt.handler, func(t *testing.T) {
			result := getHandlerEmoji(tt.handler)
			assert.Equal(t, tt.expected, result)
		})
	}
}

func TestCollectAndProcessConfirmations_NoConfirmations(t *testing.T) {
	dialog := NewConsoleConfirmationDialog()

	ctx, err := CollectAndProcessConfirmations([]types.ConfirmationRequest{}, dialog)
	assert.NoError(t, err)
	assert.Nil(t, ctx) // No confirmations means nil context
}

// Mock dialog for testing without user interaction
type mockConfirmationDialog struct {
	responses []types.ConfirmationResponse
	err       error
}

func (m *mockConfirmationDialog) PresentConfirmations(confirmations []types.ConfirmationRequest) ([]types.ConfirmationResponse, error) {
	if m.err != nil {
		return nil, m.err
	}
	return m.responses, nil
}

func TestCollectAndProcessConfirmations_WithMockDialog(t *testing.T) {
	confirmations := []types.ConfirmationRequest{
		{ID: "conf1", Pack: "pack1"},
		{ID: "conf2", Pack: "pack2"},
	}

	responses := []types.ConfirmationResponse{
		{ID: "conf1", Approved: true},
		{ID: "conf2", Approved: false},
	}

	mockDialog := &mockConfirmationDialog{responses: responses}

	ctx, err := CollectAndProcessConfirmations(confirmations, mockDialog)
	assert.NoError(t, err)
	assert.NotNil(t, ctx)

	assert.True(t, ctx.IsApproved("conf1"))
	assert.False(t, ctx.IsApproved("conf2"))
}

func TestCollectAndProcessConfirmations_DialogError(t *testing.T) {
	confirmations := []types.ConfirmationRequest{
		{ID: "conf1", Pack: "pack1"},
	}

	mockDialog := &mockConfirmationDialog{err: assert.AnError}

	ctx, err := CollectAndProcessConfirmations(confirmations, mockDialog)
	assert.Error(t, err)
	assert.Nil(t, ctx)
	assert.Contains(t, err.Error(), "failed to collect confirmation responses")
}
