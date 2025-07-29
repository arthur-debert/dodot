package testutil

import (
	"io/fs"

	"github.com/arthur-debert/dodot/pkg/types"
)

// MockTrigger is a mock implementation of the types.Trigger interface for testing.
type MockTrigger struct {
	NameFunc        func() string
	DescriptionFunc func() string
	PriorityFunc    func() int
	MatchFunc       func(path string, info fs.FileInfo) (bool, map[string]interface{})
}

// Name returns the mock's name.
func (m *MockTrigger) Name() string {
	if m.NameFunc != nil {
		return m.NameFunc()
	}
	return "mock-trigger"
}

// Description returns the mock's description.
func (m *MockTrigger) Description() string {
	if m.DescriptionFunc != nil {
		return m.DescriptionFunc()
	}
	return "A mock trigger for testing."
}

// Priority returns the mock's priority.
func (m *MockTrigger) Priority() int {
	if m.PriorityFunc != nil {
		return m.PriorityFunc()
	}
	return 0
}

// Match runs the mock's match function.
func (m *MockTrigger) Match(path string, info fs.FileInfo) (bool, map[string]interface{}) {
	if m.MatchFunc != nil {
		return m.MatchFunc(path, info)
	}
	return false, nil
}

// MockPowerUp is a mock implementation of the types.PowerUp interface for testing.
type MockPowerUp struct {
	NameFunc            func() string
	DescriptionFunc     func() string
	RunModeFunc         func() types.RunMode
	ProcessFunc         func(matches []types.TriggerMatch) ([]types.Action, error)
	ValidateOptionsFunc func(options map[string]interface{}) error
}

// Name returns the mock's name.
func (m *MockPowerUp) Name() string {
	if m.NameFunc != nil {
		return m.NameFunc()
	}
	return "mock-powerup"
}

// Description returns the mock's description.
func (m *MockPowerUp) Description() string {
	if m.DescriptionFunc != nil {
		return m.DescriptionFunc()
	}
	return "A mock power-up for testing."
}

// RunMode returns the mock's run mode.
func (m *MockPowerUp) RunMode() types.RunMode {
	if m.RunModeFunc != nil {
		return m.RunModeFunc()
	}
	return types.RunModeMany
}

// Process runs the mock's process function.
func (m *MockPowerUp) Process(matches []types.TriggerMatch) ([]types.Action, error) {
	if m.ProcessFunc != nil {
		return m.ProcessFunc(matches)
	}
	return nil, nil
}

// ValidateOptions runs the mock's validate options function.
func (m *MockPowerUp) ValidateOptions(options map[string]interface{}) error {
	if m.ValidateOptionsFunc != nil {
		return m.ValidateOptionsFunc(options)
	}
	return nil
}
