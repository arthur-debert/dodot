package testutil

import (
	"github.com/arthur-debert/dodot/pkg/types"
)

// MockHandler is a mock implementation of the types.Handler interface for testing.
type MockHandler struct {
	NameFunc               func() string
	DescriptionFunc        func() string
	RunModeFunc            func() types.RunMode
	ProcessFunc            func(matches []types.RuleMatch) ([]types.Action, error)
	ValidateOptionsFunc    func(options map[string]interface{}) error
	GetTemplateContentFunc func() string
}

// Name returns the mock's name.
func (m *MockHandler) Name() string {
	if m.NameFunc != nil {
		return m.NameFunc()
	}
	return "mock-handler"
}

// Description returns the mock's description.
func (m *MockHandler) Description() string {
	if m.DescriptionFunc != nil {
		return m.DescriptionFunc()
	}
	return "A mock handler for testing."
}

// RunMode returns the mock's run mode.
func (m *MockHandler) RunMode() types.RunMode {
	if m.RunModeFunc != nil {
		return m.RunModeFunc()
	}
	return types.RunModeLinking
}

// Process runs the mock's process function.
func (m *MockHandler) Process(matches []types.RuleMatch) ([]types.Action, error) {
	if m.ProcessFunc != nil {
		return m.ProcessFunc(matches)
	}
	return nil, nil
}

// ValidateOptions runs the mock's validate options function.
func (m *MockHandler) ValidateOptions(options map[string]interface{}) error {
	if m.ValidateOptionsFunc != nil {
		return m.ValidateOptionsFunc(options)
	}
	return nil
}

// GetTemplateContent returns the mock's template content.
func (m *MockHandler) GetTemplateContent() string {
	if m.GetTemplateContentFunc != nil {
		return m.GetTemplateContentFunc()
	}
	return ""
}
