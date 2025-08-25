package types

import (
	"testing"

	"github.com/stretchr/testify/assert"
)

// Mock handlers for testing

type mockLinkingHandler struct{}

func (m *mockLinkingHandler) Name() string                                 { return "mock-linking" }
func (m *mockLinkingHandler) Description() string                          { return "Mock linking handler" }
func (m *mockLinkingHandler) RunMode() RunMode                             { return RunModeLinking }
func (m *mockLinkingHandler) ValidateOptions(map[string]interface{}) error { return nil }
func (m *mockLinkingHandler) GetTemplateContent() string                   { return "" }
func (m *mockLinkingHandler) ProcessLinking([]TriggerMatch) ([]LinkingAction, error) {
	return []LinkingAction{}, nil
}

type mockLinkingHandlerWithConfirmations struct {
	*mockLinkingHandler
}

func (m *mockLinkingHandlerWithConfirmations) ProcessLinkingWithConfirmations([]TriggerMatch) (ProcessingResult, error) {
	return ProcessingResult{}, nil
}

type mockProvisioningHandler struct{}

func (m *mockProvisioningHandler) Name() string                                 { return "mock-provisioning" }
func (m *mockProvisioningHandler) Description() string                          { return "Mock provisioning handler" }
func (m *mockProvisioningHandler) RunMode() RunMode                             { return RunModeProvisioning }
func (m *mockProvisioningHandler) ValidateOptions(map[string]interface{}) error { return nil }
func (m *mockProvisioningHandler) GetTemplateContent() string                   { return "" }
func (m *mockProvisioningHandler) ProcessProvisioning([]TriggerMatch) ([]ProvisioningAction, error) {
	return []ProvisioningAction{}, nil
}

type mockProvisioningHandlerWithConfirmations struct {
	*mockProvisioningHandler
}

func (m *mockProvisioningHandlerWithConfirmations) ProcessProvisioningWithConfirmations([]TriggerMatch) (ProcessingResult, error) {
	return ProcessingResult{}, nil
}

type mockClearableHandler struct{}

func (m *mockClearableHandler) Clear(ClearContext) ([]ClearedItem, error) {
	return []ClearedItem{}, nil
}

type mockClearableHandlerWithConfirmations struct {
	*mockClearableHandler
}

func (m *mockClearableHandlerWithConfirmations) ClearWithConfirmations(ClearContext, *ConfirmationContext) ([]ClearedItem, error) {
	return []ClearedItem{}, nil
}

func (m *mockClearableHandlerWithConfirmations) GetClearConfirmations(ClearContext) ([]ConfirmationRequest, error) {
	return []ConfirmationRequest{}, nil
}

// Tests

func TestNewConfirmationRequest(t *testing.T) {
	req := NewConfirmationRequest(
		"test-id",
		"test-pack",
		"test-handler",
		"test-operation",
		"Test Title",
		"Test Description",
		[]string{"item1", "item2"},
		true,
	)

	assert.Equal(t, "test-id", req.ID)
	assert.Equal(t, "test-pack", req.Pack)
	assert.Equal(t, "test-handler", req.Handler)
	assert.Equal(t, "test-operation", req.Operation)
	assert.Equal(t, "Test Title", req.Title)
	assert.Equal(t, "Test Description", req.Description)
	assert.Equal(t, []string{"item1", "item2"}, req.Items)
	assert.True(t, req.Default)
}

func TestNewClearConfirmationRequest(t *testing.T) {
	req := NewClearConfirmationRequest(
		"clear-id",
		"clear-pack",
		"clear-handler",
		"Clear Title",
		"Clear Description",
		[]string{"file1", "file2"},
		false,
	)

	assert.Equal(t, "clear-id", req.ID)
	assert.Equal(t, "clear-pack", req.Pack)
	assert.Equal(t, "clear-handler", req.Handler)
	assert.Equal(t, "clear", req.Operation) // Should be set to "clear"
	assert.Equal(t, "Clear Title", req.Title)
	assert.Equal(t, "Clear Description", req.Description)
	assert.Equal(t, []string{"file1", "file2"}, req.Items)
	assert.False(t, req.Default)
}

func TestNewProvisionConfirmationRequest(t *testing.T) {
	req := NewProvisionConfirmationRequest(
		"provision-id",
		"provision-pack",
		"provision-handler",
		"Provision Title",
		"Provision Description",
		[]string{"package1", "package2"},
		true,
	)

	assert.Equal(t, "provision-id", req.ID)
	assert.Equal(t, "provision-pack", req.Pack)
	assert.Equal(t, "provision-handler", req.Handler)
	assert.Equal(t, "provision", req.Operation) // Should be set to "provision"
	assert.Equal(t, "Provision Title", req.Title)
	assert.Equal(t, "Provision Description", req.Description)
	assert.Equal(t, []string{"package1", "package2"}, req.Items)
	assert.True(t, req.Default)
}

func TestIsLinkingHandlerWithConfirmations(t *testing.T) {
	regularHandler := &mockLinkingHandler{}
	confirmationHandler := &mockLinkingHandlerWithConfirmations{mockLinkingHandler: regularHandler}

	assert.False(t, IsLinkingHandlerWithConfirmations(regularHandler))
	assert.True(t, IsLinkingHandlerWithConfirmations(confirmationHandler))
	assert.False(t, IsLinkingHandlerWithConfirmations("not a handler"))
	assert.False(t, IsLinkingHandlerWithConfirmations(nil))
}

func TestIsProvisioningHandlerWithConfirmations(t *testing.T) {
	regularHandler := &mockProvisioningHandler{}
	confirmationHandler := &mockProvisioningHandlerWithConfirmations{mockProvisioningHandler: regularHandler}

	assert.False(t, IsProvisioningHandlerWithConfirmations(regularHandler))
	assert.True(t, IsProvisioningHandlerWithConfirmations(confirmationHandler))
	assert.False(t, IsProvisioningHandlerWithConfirmations("not a handler"))
	assert.False(t, IsProvisioningHandlerWithConfirmations(nil))
}

func TestIsClearableWithConfirmations(t *testing.T) {
	regularHandler := &mockClearableHandler{}
	confirmationHandler := &mockClearableHandlerWithConfirmations{mockClearableHandler: regularHandler}

	assert.False(t, IsClearableWithConfirmations(regularHandler))
	assert.True(t, IsClearableWithConfirmations(confirmationHandler))
	assert.False(t, IsClearableWithConfirmations("not a handler"))
	assert.False(t, IsClearableWithConfirmations(nil))
}

func TestHandlerInterfaceComposition(t *testing.T) {
	// Test that handlers with confirmations still implement the base interfaces
	linkingHandler := &mockLinkingHandlerWithConfirmations{mockLinkingHandler: &mockLinkingHandler{}}
	provisioningHandler := &mockProvisioningHandlerWithConfirmations{mockProvisioningHandler: &mockProvisioningHandler{}}
	clearableHandler := &mockClearableHandlerWithConfirmations{mockClearableHandler: &mockClearableHandler{}}

	// Should implement base interfaces
	var _ LinkingHandler = linkingHandler
	var _ ProvisioningHandler = provisioningHandler
	var _ Clearable = clearableHandler

	// Should implement confirmation interfaces
	var _ LinkingHandlerWithConfirmations = linkingHandler
	var _ ProvisioningHandlerWithConfirmations = provisioningHandler
	var _ ClearableWithConfirmations = clearableHandler

	// Should be able to call base interface methods
	assert.Equal(t, "mock-linking", linkingHandler.Name())
	assert.Equal(t, "mock-provisioning", provisioningHandler.Name())

	// Should be able to call confirmation interface methods
	_, err := linkingHandler.ProcessLinkingWithConfirmations([]TriggerMatch{})
	assert.NoError(t, err)

	_, err = provisioningHandler.ProcessProvisioningWithConfirmations([]TriggerMatch{})
	assert.NoError(t, err)

	_, err = clearableHandler.GetClearConfirmations(ClearContext{})
	assert.NoError(t, err)
}
