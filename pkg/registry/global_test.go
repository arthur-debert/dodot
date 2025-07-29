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

// Mock power-up for testing
type mockPowerUp struct {
	name string
}

func (m *mockPowerUp) Name() string        { return m.name }
func (m *mockPowerUp) Description() string { return "mock power-up" }
func (m *mockPowerUp) RunMode() types.RunMode { return types.RunModeMany }
func (m *mockPowerUp) Process(matches []types.TriggerMatch) ([]types.Action, error) {
	return []types.Action{}, nil
}
func (m *mockPowerUp) ValidateOptions(options map[string]interface{}) error {
	return nil
}

func TestGetRegistry(t *testing.T) {
	// Test getting trigger registry
	triggerReg := GetRegistry[types.Trigger]()
	testutil.AssertNotNil(t, triggerReg)
	
	// Test getting power-up registry
	powerUpReg := GetRegistry[types.PowerUp]()
	testutil.AssertNotNil(t, powerUpReg)
	
	// Test getting trigger factory registry
	triggerFactoryReg := GetRegistry[types.TriggerFactory]()
	testutil.AssertNotNil(t, triggerFactoryReg)
	
	// Test getting power-up factory registry
	powerUpFactoryReg := GetRegistry[types.PowerUpFactory]()
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

func TestRegisterAndGetPowerUpFactory(t *testing.T) {
	// Create a factory function
	factory := func(options map[string]interface{}) (types.PowerUp, error) {
		return &mockPowerUp{name: "test-powerup"}, nil
	}
	
	// Register the factory
	err := RegisterPowerUpFactory("test-powerup", factory)
	testutil.AssertNoError(t, err)
	
	// Retrieve the factory
	retrievedFactory, err := GetPowerUpFactory("test-powerup")
	testutil.AssertNoError(t, err)
	testutil.AssertNotNil(t, retrievedFactory)
	
	// Create power-up using the factory
	powerUp, err := retrievedFactory(nil)
	testutil.AssertNoError(t, err)
	testutil.AssertEqual(t, "test-powerup", powerUp.Name())
	
	// Test getting non-existent factory
	_, err = GetPowerUpFactory("non-existent")
	testutil.AssertError(t, err)
	testutil.AssertContains(t, err.Error(), "power-up factory not found")
	
	// Clean up
	powerUpFactoryReg := GetRegistry[types.PowerUpFactory]()
	_ = powerUpFactoryReg.Remove("test-powerup")
}