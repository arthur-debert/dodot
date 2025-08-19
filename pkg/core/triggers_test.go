package core

import (
	"os"
	"testing"

	"github.com/arthur-debert/dodot/pkg/matchers"
	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/stretchr/testify/assert"
)

func TestGetPackMatchers(t *testing.T) {
	tests := []struct {
		name             string
		pack             types.Pack
		expectedMatchers int
		description      string
	}{
		{
			name: "returns default matchers",
			pack: types.Pack{
				Name: "test-pack",
				Path: "/test/path",
			},
			expectedMatchers: len(matchers.DefaultMatchers()),
			description:      "should always return the default matchers regardless of pack",
		},
		{
			name: "pack with config",
			pack: types.Pack{
				Name: "configured-pack",
				Path: "/test/path",
				Config: types.PackConfig{
					Ignore: []types.IgnoreRule{{Path: "*.tmp"}},
					Override: []types.OverrideRule{{
						Path:    "*.sh",
						Powerup: "install_script",
					}},
				},
			},
			expectedMatchers: len(matchers.DefaultMatchers()),
			description:      "should return default matchers even when pack has config",
		},
		{
			name:             "empty pack struct",
			pack:             types.Pack{},
			expectedMatchers: len(matchers.DefaultMatchers()),
			description:      "should handle empty pack struct gracefully",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result := getPackMatchers(tt.pack)

			assert.Len(t, result, tt.expectedMatchers, tt.description)

			// Verify we got the actual default matchers, not a copy or modified version
			defaultMatchers := matchers.DefaultMatchers()
			assert.Equal(t, defaultMatchers, result, "should return exactly the default matchers")
		})
	}
}

func TestGetPackMatchers_DefaultMatchersContent(t *testing.T) {
	// This test verifies that getPackMatchers returns the expected default matchers
	// It will need to be updated if the default matchers change
	pack := types.Pack{Name: "test"}
	result := getPackMatchers(pack)

	// Check that we have some matchers (the exact count may change)
	assert.NotEmpty(t, result, "should return at least one default matcher")

	// Verify all returned matchers are valid
	for _, matcher := range result {
		assert.NotEmpty(t, matcher.Name, "matcher should have a name")
		assert.NotEmpty(t, matcher.TriggerName, "matcher should have a trigger name")
		assert.NotEmpty(t, matcher.PowerUpName, "matcher should have a powerup name")
	}
}

// MockTrigger implements the Trigger interface for testing
type MockTrigger struct {
	shouldMatch bool
	metadata    map[string]interface{}
}

func (m *MockTrigger) Match(path string, info os.FileInfo) (bool, map[string]interface{}) {
	return m.shouldMatch, m.metadata
}

func (m *MockTrigger) Type() string {
	return "mock"
}

func TestTestMatcher(t *testing.T) {
	// Note: This test requires setting up the registry, which is a global state.
	// In a real unit test, we'd want to refactor testMatcher to accept a registry interface
	// For now, we'll document that this function is better tested through integration tests
	t.Skip("testMatcher requires global registry setup - covered by integration tests")
}
