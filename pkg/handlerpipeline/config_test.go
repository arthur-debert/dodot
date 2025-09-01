// Test Type: Unit Test
// Description: Tests for the rules package - configuration loading and rule management

package handlerpipeline_test

import (
	"testing"

	"github.com/arthur-debert/dodot/pkg/config"
	"github.com/arthur-debert/dodot/pkg/handlerpipeline"
	"github.com/arthur-debert/dodot/pkg/testutil"
	"github.com/knadh/koanf/providers/confmap"
	"github.com/knadh/koanf/v2"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestLoadRules(t *testing.T) {
	t.Run("loads_rules_from_config", func(t *testing.T) {
		// Create a koanf instance with rules
		data := map[string]interface{}{
			"rules": []interface{}{
				map[string]interface{}{
					"pattern": "*.sh",
					"handler": "shell",
				},
				map[string]interface{}{
					"pattern": "bin/",
					"handler": "path",
				},
				map[string]interface{}{
					"pattern": "*",
					"handler": "symlink",
				},
			},
		}

		k := koanf.New(".")
		err := k.Load(confmap.Provider(data, "."), nil)
		require.NoError(t, err)

		rules, err := handlerpipeline.LoadRules(k)
		assert.NoError(t, err)
		assert.Len(t, rules, 3)

		assert.Equal(t, "*.sh", rules[0].Pattern)
		assert.Equal(t, "shell", rules[0].Handler)

		assert.Equal(t, "bin/", rules[1].Pattern)
		assert.Equal(t, "path", rules[1].Handler)

		assert.Equal(t, "*", rules[2].Pattern)
		assert.Equal(t, "symlink", rules[2].Handler)
	})

	t.Run("returns_defaults_when_no_rules_configured", func(t *testing.T) {
		k := koanf.New(".")
		// Empty config

		rules, err := handlerpipeline.LoadRules(k)
		assert.NoError(t, err)
		assert.NotEmpty(t, rules)

		// Check we have exclusions, exact matches, and catchall
		var hasExclusions, hasExactMatches, hasCatchall bool
		for _, rule := range rules {
			if rule.Pattern[0] == '!' {
				hasExclusions = true
			}
			if rule.Pattern == "install.sh" {
				hasExactMatches = true
			}
			if rule.Pattern == "*" && rule.Handler == "symlink" {
				hasCatchall = true
			}
		}
		assert.True(t, hasExclusions, "Default rules should have exclusions")
		assert.True(t, hasExactMatches, "Default rules should have exact matches")
		assert.True(t, hasCatchall, "Default rules should have catchall")
	})

	t.Run("validates_rules", func(t *testing.T) {
		// Rule with empty pattern
		data := map[string]interface{}{
			"rules": []interface{}{
				map[string]interface{}{
					"pattern": "",
					"handler": "shell",
				},
			},
		}

		k := koanf.New(".")
		err := k.Load(confmap.Provider(data, "."), nil)
		require.NoError(t, err)

		_, err = handlerpipeline.LoadRules(k)
		assert.Error(t, err)
		assert.Contains(t, err.Error(), "empty pattern")

		// Rule with empty handler (non-exclusion)
		data = map[string]interface{}{
			"rules": []interface{}{
				map[string]interface{}{
					"pattern": "*.sh",
					"handler": "",
				},
			},
		}

		k = koanf.New(".")
		err = k.Load(confmap.Provider(data, "."), nil)
		require.NoError(t, err)

		_, err = handlerpipeline.LoadRules(k)
		assert.Error(t, err)
		assert.Contains(t, err.Error(), "empty handler")

		// Exclusion rule with empty handler is OK
		data = map[string]interface{}{
			"rules": []interface{}{
				map[string]interface{}{
					"pattern": "!*.tmp",
					"handler": "",
				},
			},
		}

		k = koanf.New(".")
		err = k.Load(confmap.Provider(data, "."), nil)
		require.NoError(t, err)

		rules, err := handlerpipeline.LoadRules(k)
		assert.NoError(t, err)
		assert.Len(t, rules, 1)
	})
}

func TestMergeRules(t *testing.T) {
	t.Run("pack_rules_take_precedence", func(t *testing.T) {
		global := []config.Rule{
			{Pattern: "*.sh", Handler: "shell"},
			{Pattern: "*", Handler: "symlink"},
		}

		packSpecific := []config.Rule{
			{Pattern: "special.sh", Handler: "install"},
			{Pattern: "*.sh", Handler: "custom"},
		}

		merged := handlerpipeline.MergeRules(global, packSpecific)

		// Pack rules should come first
		assert.Equal(t, packSpecific[0], merged[0])
		assert.Equal(t, packSpecific[1], merged[1])
		assert.Equal(t, global[0], merged[2])
		assert.Equal(t, global[1], merged[3])

		assert.Len(t, merged, 4)
	})

	t.Run("handles_empty_rule_sets", func(t *testing.T) {
		// Empty pack rules
		global := []config.Rule{
			{Pattern: "*", Handler: "symlink"},
		}
		merged := handlerpipeline.MergeRules(global, nil)
		assert.Equal(t, global, merged)

		// Empty global rules
		packSpecific := []config.Rule{
			{Pattern: "*.sh", Handler: "shell"},
		}
		merged = handlerpipeline.MergeRules(nil, packSpecific)
		assert.Equal(t, packSpecific, merged)

		// Both empty
		merged = handlerpipeline.MergeRules(nil, nil)
		assert.Empty(t, merged)
	})
}

func TestLoadPackRulesFS(t *testing.T) {
	t.Run("returns_empty_rules_when_no_config", func(t *testing.T) {
		// This test documents current behavior - LoadPackRulesFS always returns empty rules
		// This is noted in the source code as a TODO

		// Create a memory filesystem with no config file
		fs := testutil.NewMemoryFS()
		// Don't create any files, so Stat will fail

		rules, err := handlerpipeline.LoadPackRulesFS("/some/path", fs)
		assert.NoError(t, err)
		assert.Empty(t, rules)
	})
}

// Remove mockFS completely - we'll use testutil.MemoryFS instead
