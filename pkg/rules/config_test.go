package rules

import (
	"testing"

	"github.com/arthur-debert/dodot/pkg/config"
	"github.com/stretchr/testify/assert"
)

func TestGetDefaultRules(t *testing.T) {
	rules := getDefaultRules()

	// Check we have some rules
	assert.NotEmpty(t, rules)

	// Check exclusion rules
	exclusionCount := 0
	for _, r := range rules {
		if r.Pattern[0] == '!' {
			exclusionCount++
		}
	}
	assert.Greater(t, exclusionCount, 0, "Should have exclusion rules")

	// Check for essential handlers
	handlers := make(map[string]bool)
	for _, r := range rules {
		if r.Handler != "" {
			handlers[r.Handler] = true
		}
	}

	assert.True(t, handlers["symlink"], "Should have symlink handler")
	assert.True(t, handlers["install"], "Should have install handler")
	assert.True(t, handlers["shell"], "Should have shell handler")
	assert.True(t, handlers["path"], "Should have path handler")
	assert.True(t, handlers["homebrew"], "Should have homebrew handler")

	// Check catchall rule exists
	var catchall *config.Rule
	for i, r := range rules {
		if r.Pattern == "*" && r.Handler == "symlink" {
			catchall = &rules[i]
		}
	}
	assert.NotNil(t, catchall, "Should have catchall rule")
}

func TestMergeRules(t *testing.T) {
	global := []config.Rule{
		{Pattern: "*.sh", Handler: "shell"},
		{Pattern: "*", Handler: "symlink"},
	}

	packSpecific := []config.Rule{
		{Pattern: "special.sh", Handler: "install"},
	}

	merged := MergeRules(global, packSpecific)

	// Pack rules should come first
	assert.Equal(t, "special.sh", merged[0].Pattern)
	assert.Equal(t, "install", merged[0].Handler)

	// All rules should be present
	assert.Len(t, merged, 3)
}
