package triggers

import (
	"testing"

	"github.com/arthur-debert/dodot/pkg/registry"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestFileNameTrigger_FactoryRegistration(t *testing.T) {
	// Test that the factory is registered
	factory, err := registry.GetTriggerFactory(FileNameTriggerName)
	require.NoError(t, err)
	require.NotNil(t, factory)

	// Test factory with pattern config
	trigger, err := factory(map[string]interface{}{
		"pattern": "*.md",
	})
	require.NoError(t, err)
	require.NotNil(t, trigger)

	assert.Equal(t, FileNameTriggerName, trigger.Name())
	assert.Equal(t, "Matches files by glob pattern: *.md", trigger.Description())

	// Test factory with default pattern
	trigger2, err := factory(map[string]interface{}{})
	require.NoError(t, err)
	require.NotNil(t, trigger2)

	assert.Equal(t, FileNameTriggerName, trigger2.Name())
	assert.Equal(t, "Matches files by glob pattern: *", trigger2.Description())
}
