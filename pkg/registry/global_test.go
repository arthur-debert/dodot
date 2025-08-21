package registry

import (
	"io/fs"
	"testing"

	"github.com/arthur-debert/dodot/pkg/testutil"
	"github.com/arthur-debert/dodot/pkg/types"
)

// Mock trigger for testing
type mockTrigger struct {
	name string
}

func (m *mockTrigger) Name() string        { return m.name }
func (m *mockTrigger) Description() string { return "mock trigger" }
func (m *mockTrigger) Priority() int       { return 1 }
func (m *mockTrigger) Match(path string, info fs.FileInfo) (bool, map[string]interface{}) {
	return true, nil
}
func (m *mockTrigger) Type() types.TriggerType { return types.TriggerTypeSpecific }

// Mock power-up for testing
type mockHandler struct {
	name string
}

func (m *mockHandler) Name() string           { return m.name }
func (m *mockHandler) Description() string    { return "mock power-up" }
func (m *mockHandler) RunMode() types.RunMode { return types.RunModeMany }
func (m *mockHandler) Process(matches []types.TriggerMatch) ([]types.Action, error) {
	return []types.Action{}, nil
}
func (m *mockHandler) ValidateOptions(options map[string]interface{}) error {
	return nil
}
func (m *mockHandler) GetTemplateContent() string {
	return ""
}

func TestGetRegistry(t *testing.T) {
	// Test getting trigger registry
	triggerReg := GetRegistry[types.Trigger]()
	testutil.AssertNotNil(t, triggerReg)

	// Test getting power-up registry
	powerUpReg := GetRegistry[types.Handler]()
	testutil.AssertNotNil(t, powerUpReg)

	// Test getting trigger factory registry
	triggerFactoryReg := GetRegistry[types.TriggerFactory]()
	testutil.AssertNotNil(t, triggerFactoryReg)

	// Test getting power-up factory registry
	powerUpFactoryReg := GetRegistry[types.HandlerFactory]()
	testutil.AssertNotNil(t, powerUpFactoryReg)

	// Test getting registry for unknown type (should create new one)
	type unknownType struct{}
	unknownReg := GetRegistry[unknownType]()
	testutil.AssertNotNil(t, unknownReg)
}

func TestRegisterAndGetTriggerFactory(t *testing.T) {
	// Create a factory function
	factory := func(options map[string]interface{}) (types.Trigger, error) {
		return &mockTrigger{name: "test-trigger"}, nil
	}

	// Register the factory
	err := RegisterTriggerFactory("test-trigger", factory)
	testutil.AssertNoError(t, err)

	// Retrieve the factory
	retrievedFactory, err := GetTriggerFactory("test-trigger")
	testutil.AssertNoError(t, err)
	testutil.AssertNotNil(t, retrievedFactory)

	// Create trigger using the factory
	trigger, err := retrievedFactory(nil)
	testutil.AssertNoError(t, err)
	testutil.AssertEqual(t, "test-trigger", trigger.Name())

	// Test getting non-existent factory
	_, err = GetTriggerFactory("non-existent")
	testutil.AssertError(t, err)
	testutil.AssertContains(t, err.Error(), "trigger factory not found")

	// Clean up
	triggerFactoryReg := GetRegistry[types.TriggerFactory]()
	_ = triggerFactoryReg.Remove("test-trigger")
}

func TestRegisterAndGetHandlerFactory(t *testing.T) {
	// Create a factory function
	factory := func(options map[string]interface{}) (types.Handler, error) {
		return &mockHandler{name: "test-handler"}, nil
	}

	// Register the factory
	err := RegisterHandlerFactory("test-handler", factory)
	testutil.AssertNoError(t, err)

	// Retrieve the factory
	retrievedFactory, err := GetHandlerFactory("test-handler")
	testutil.AssertNoError(t, err)
	testutil.AssertNotNil(t, retrievedFactory)

	// Create power-up using the factory
	powerUp, err := retrievedFactory(nil)
	testutil.AssertNoError(t, err)
	testutil.AssertEqual(t, "test-handler", powerUp.Name())

	// Test getting non-existent factory
	_, err = GetHandlerFactory("non-existent")
	testutil.AssertError(t, err)
	testutil.AssertContains(t, err.Error(), "power-up factory not found")

	// Clean up
	powerUpFactoryReg := GetRegistry[types.HandlerFactory]()
	_ = powerUpFactoryReg.Remove("test-handler")
}
