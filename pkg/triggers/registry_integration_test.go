package triggers

import (
	"testing"

	"github.com/arthur-debert/dodot/pkg/registry"
	"github.com/arthur-debert/dodot/pkg/testutil"
	"github.com/arthur-debert/dodot/pkg/types"
)

func TestCatchallTrigger_FactoryRegistration(t *testing.T) {
	// Test that the factory is registered
	factory, err := registry.GetTriggerFactory(CatchallTriggerName)
	testutil.AssertNoError(t, err)
	testutil.AssertNotNil(t, factory)

	// Test creating a trigger through the factory
	trigger, err := factory(map[string]interface{}{
		"excludePatterns": []string{"test.txt"},
	})
	testutil.AssertNoError(t, err)
	testutil.AssertNotNil(t, trigger)
	testutil.AssertEqual(t, CatchallTriggerName, trigger.Name())
	testutil.AssertEqual(t, types.TriggerTypeCatchall, trigger.Type())
}
