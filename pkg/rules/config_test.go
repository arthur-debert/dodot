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

	// Check catchall rule exists with lowest priority
	var catchall *Rule
	for i, r := range rules {
		if r.Pattern == "*" && r.Handler == "symlink" {
			catchall = &rules[i]
		}
	}
	assert.NotNil(t, catchall, "Should have catchall rule")
	assert.Equal(t, 0, catchall.Priority, "Catchall should have priority 0")
}

func TestAdaptConfigMatchersToRules(t *testing.T) {
	tests := []struct {
		name     string
		matcher  config.MatcherConfig
		expected Rule
	}{
		{
			name: "filename trigger",
			matcher: config.MatcherConfig{
				Name:     "install-script",
				Priority: 90,
				Trigger: config.TriggerConfig{
					Type: "filename",
					Data: map[string]interface{}{
						"pattern": "install.sh",
					},
				},
				Handler: config.HandlerConfig{
					Type: "install",
					Data: map[string]interface{}{},
				},
			},
			expected: Rule{
				Pattern:  "install.sh",
				Handler:  "install",
				Priority: 90,
				Options:  map[string]interface{}{},
			},
		},
		{
			name: "directory trigger gets trailing slash",
			matcher: config.MatcherConfig{
				Name:     "bin-dir",
				Priority: 80,
				Trigger: config.TriggerConfig{
					Type: "directory",
					Data: map[string]interface{}{
						"pattern": "bin",
					},
				},
				Handler: config.HandlerConfig{
					Type: "path",
					Data: map[string]interface{}{},
				},
			},
			expected: Rule{
				Pattern:  "bin/",
				Handler:  "path",
				Priority: 80,
				Options:  map[string]interface{}{},
			},
		},
		{
			name: "handler with options",
			matcher: config.MatcherConfig{
				Name:     "shell-aliases",
				Priority: 70,
				Trigger: config.TriggerConfig{
					Type: "filename",
					Data: map[string]interface{}{
						"pattern": "*aliases.sh",
					},
				},
				Handler: config.HandlerConfig{
					Type: "shell",
					Data: map[string]interface{}{
						"placement": "aliases",
					},
				},
			},
			expected: Rule{
				Pattern:  "*aliases.sh",
				Handler:  "shell",
				Priority: 70,
				Options: map[string]interface{}{
					"placement": "aliases",
				},
			},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			rules := adaptConfigMatchersToRules([]config.MatcherConfig{tt.matcher})
			assert.Len(t, rules, 1)
			assert.Equal(t, tt.expected, rules[0])
		})
	}
}

func TestMergeRules(t *testing.T) {
	global := []Rule{
		{Pattern: "*.sh", Handler: "shell", Priority: 50},
		{Pattern: "*", Handler: "symlink", Priority: 0},
	}

	packSpecific := []Rule{
		{Pattern: "special.sh", Handler: "install", Priority: 60},
	}

	merged := MergeRules(global, packSpecific)

	// Pack rules should be boosted
	assert.Equal(t, 1060, merged[0].Priority, "Pack rule priority should be boosted")

	// All rules should be present
	assert.Len(t, merged, 3)
}
