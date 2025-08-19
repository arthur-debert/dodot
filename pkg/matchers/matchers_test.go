package matchers

import (
	"testing"

	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/stretchr/testify/assert"
)

func TestDefaultMatchers(t *testing.T) {
	defaults := DefaultMatchers()

	// Should have exactly the matchers we defined
	assert.Equal(t, 9, len(defaults)) // 2 install + 2 shell + 4 bin + 1 catchall

	// Check some expected matchers exist
	expectedNames := map[string]bool{
		"install-script":   false,
		"brewfile":         false,
		"shell-aliases":    false,
		"shell-profile":    false,
		"bin-dir":          false,
		"bin-path":         false,
		"local-bin-dir":    false,
		"local-bin-path":   false,
		"symlink-catchall": false,
	}

	for _, m := range defaults {
		if _, exists := expectedNames[m.Name]; exists {
			expectedNames[m.Name] = true
		}

		// All default matchers should be enabled
		assert.True(t, m.Enabled)

		// Should have trigger options
		assert.NotNil(t, m.TriggerOptions)

		// Check trigger-specific options
		switch m.TriggerName {
		case "filename", "directory", "path_pattern":
			assert.Contains(t, m.TriggerOptions, "pattern")
		case "extension":
			assert.Contains(t, m.TriggerOptions, "extension")
		}
	}

	// Ensure all expected matchers were found
	for name, found := range expectedNames {
		assert.True(t, found, "expected matcher %s not found", name)
	}
}

func TestSortMatchersByPriority(t *testing.T) {
	matchers := []types.Matcher{
		{Name: "low", Priority: 10},
		{Name: "high", Priority: 100},
		{Name: "medium", Priority: 50},
		{Name: "also-high", Priority: 100},
		{Name: "also-medium", Priority: 50},
	}

	SortMatchersByPriority(matchers)

	// Check ordering
	assert.Equal(t, 100, matchers[0].Priority)
	assert.Equal(t, 100, matchers[1].Priority)
	assert.Equal(t, 50, matchers[2].Priority)
	assert.Equal(t, 50, matchers[3].Priority)
	assert.Equal(t, 10, matchers[4].Priority)

	// For same priority, should be sorted by name
	assert.Equal(t, "also-high", matchers[0].Name)
	assert.Equal(t, "high", matchers[1].Name)
	assert.Equal(t, "also-medium", matchers[2].Name)
	assert.Equal(t, "medium", matchers[3].Name)
}

func TestFilterEnabledMatchers(t *testing.T) {
	matchers := []types.Matcher{
		{Name: "enabled1", Enabled: true},
		{Name: "disabled1", Enabled: false},
		{Name: "enabled2", Enabled: true},
		{Name: "disabled2", Enabled: false},
		{Name: "enabled3", Enabled: true},
	}

	enabled := FilterEnabledMatchers(matchers)

	assert.Len(t, enabled, 3)
	for _, m := range enabled {
		assert.True(t, m.Enabled)
		assert.Contains(t, []string{"enabled1", "enabled2", "enabled3"}, m.Name)
	}
}

func TestMergeMatchers(t *testing.T) {
	set1 := []types.Matcher{
		{Name: "common", TriggerName: "t1", PowerUpName: "p1", Priority: 10},
		{Name: "only-in-1", TriggerName: "t2", PowerUpName: "p2", Priority: 20},
	}

	set2 := []types.Matcher{
		{Name: "common", TriggerName: "t3", PowerUpName: "p3", Priority: 30}, // Override
		{Name: "only-in-2", TriggerName: "t4", PowerUpName: "p4", Priority: 40},
	}

	set3 := []types.Matcher{
		{Name: "only-in-3", TriggerName: "t5", PowerUpName: "p5", Priority: 50},
		// Unnamed matcher
		{TriggerName: "t6", PowerUpName: "p6", Priority: 60},
	}

	merged := MergeMatchers(set1, set2, set3)

	// Should have 5 unique matchers
	assert.Len(t, merged, 5)

	// Check that "common" was overridden by set2
	for _, m := range merged {
		if m.Name == "common" {
			assert.Equal(t, "t3", m.TriggerName)
			assert.Equal(t, "p3", m.PowerUpName)
			assert.Equal(t, 30, m.Priority)
			break
		}
	}

	// Should be sorted by priority (highest first)
	for i := 1; i < len(merged); i++ {
		assert.GreaterOrEqual(t, merged[i-1].Priority, merged[i].Priority)
	}

	// Check all expected matchers are present
	names := make(map[string]bool)
	for _, m := range merged {
		if m.Name != "" {
			names[m.Name] = true
		}
	}
	assert.True(t, names["common"])
	assert.True(t, names["only-in-1"])
	assert.True(t, names["only-in-2"])
	assert.True(t, names["only-in-3"])
}
