package matchers

import (
	"testing"

	"github.com/arthur-debert/dodot/pkg/registry"
	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	// Import to register factories
	_ "github.com/arthur-debert/dodot/pkg/powerups"
	_ "github.com/arthur-debert/dodot/pkg/triggers"
)

// Register test factories for testing
func init() {
	// Register test trigger factory
	_ = registry.RegisterTriggerFactory("test-trigger", func(config map[string]interface{}) (types.Trigger, error) {
		return nil, nil
	})

	// Register test power-up factory
	_ = registry.RegisterPowerUpFactory("test-powerup", func(config map[string]interface{}) (types.PowerUp, error) {
		return nil, nil
	})
}

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

func TestCreateMatcher(t *testing.T) {
	tests := []struct {
		name    string
		config  *types.MatcherConfig
		wantErr bool
		check   func(t *testing.T, m *types.Matcher)
	}{
		{
			name: "basic matcher",
			config: &types.MatcherConfig{
				Name:    "test",
				Trigger: "filename",
				PowerUp: "symlink",
			},
			check: func(t *testing.T, m *types.Matcher) {
				assert.Equal(t, "test", m.Name)
				assert.Equal(t, "filename", m.TriggerName)
				assert.Equal(t, "symlink", m.PowerUpName)
				assert.True(t, m.Enabled)
			},
		},
		{
			name: "with pattern convenience field",
			config: &types.MatcherConfig{
				Name:    "pattern-test",
				Trigger: "filename",
				PowerUp: "symlink",
				Pattern: "*.conf",
			},
			check: func(t *testing.T, m *types.Matcher) {
				assert.NotNil(t, m.TriggerOptions)
				assert.Equal(t, "*.conf", m.TriggerOptions["pattern"])
			},
		},
		{
			name: "with target convenience field",
			config: &types.MatcherConfig{
				Name:    "target-test",
				Trigger: "filename",
				PowerUp: "symlink",
				Target:  "/custom/path",
			},
			check: func(t *testing.T, m *types.Matcher) {
				assert.NotNil(t, m.PowerUpOptions)
				assert.Equal(t, "/custom/path", m.PowerUpOptions["target"])
			},
		},
		{
			name: "with explicit options",
			config: &types.MatcherConfig{
				Name:    "options-test",
				Trigger: "filename",
				PowerUp: "symlink",
				TriggerOptions: map[string]interface{}{
					"pattern": "specific.file",
				},
				PowerUpOptions: map[string]interface{}{
					"target": "/specific/target",
				},
			},
			check: func(t *testing.T, m *types.Matcher) {
				assert.Equal(t, "specific.file", m.TriggerOptions["pattern"])
				assert.Equal(t, "/specific/target", m.PowerUpOptions["target"])
			},
		},
		{
			name: "disabled matcher",
			config: &types.MatcherConfig{
				Name:    "disabled-test",
				Trigger: "filename",
				PowerUp: "symlink",
				Enabled: func() *bool { b := false; return &b }(),
			},
			check: func(t *testing.T, m *types.Matcher) {
				assert.False(t, m.Enabled)
			},
		},
		{
			name: "missing trigger",
			config: &types.MatcherConfig{
				Name:    "invalid",
				PowerUp: "symlink",
			},
			wantErr: true,
		},
		{
			name: "missing powerup",
			config: &types.MatcherConfig{
				Name:    "invalid",
				Trigger: "filename",
			},
			wantErr: true,
		},
		{
			name: "unknown trigger",
			config: &types.MatcherConfig{
				Name:    "invalid",
				Trigger: "unknown-trigger",
				PowerUp: "symlink",
			},
			wantErr: true,
		},
		{
			name: "unknown powerup",
			config: &types.MatcherConfig{
				Name:    "invalid",
				Trigger: "filename",
				PowerUp: "unknown-powerup",
			},
			wantErr: true,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			m, err := CreateMatcher(tt.config)

			if tt.wantErr {
				assert.Error(t, err)
				assert.Nil(t, m)
			} else {
				require.NoError(t, err)
				require.NotNil(t, m)
				if tt.check != nil {
					tt.check(t, m)
				}
			}
		})
	}
}

func TestValidateMatcher(t *testing.T) {
	tests := []struct {
		name    string
		matcher *types.Matcher
		wantErr bool
		errMsg  string
	}{
		{
			name: "valid matcher",
			matcher: &types.Matcher{
				TriggerName: "filename",
				PowerUpName: "symlink",
			},
			wantErr: false,
		},
		{
			name: "missing trigger name",
			matcher: &types.Matcher{
				PowerUpName: "symlink",
			},
			wantErr: true,
			errMsg:  "trigger name is required",
		},
		{
			name: "missing powerup name",
			matcher: &types.Matcher{
				TriggerName: "filename",
			},
			wantErr: true,
			errMsg:  "power-up name is required",
		},
		{
			name: "unknown trigger",
			matcher: &types.Matcher{
				TriggerName: "non-existent",
				PowerUpName: "symlink",
			},
			wantErr: true,
			errMsg:  "unknown trigger: non-existent",
		},
		{
			name: "unknown powerup",
			matcher: &types.Matcher{
				TriggerName: "filename",
				PowerUpName: "non-existent",
			},
			wantErr: true,
			errMsg:  "unknown power-up: non-existent",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			err := ValidateMatcher(tt.matcher)

			if tt.wantErr {
				assert.Error(t, err)
				assert.Contains(t, err.Error(), tt.errMsg)
			} else {
				assert.NoError(t, err)
			}
		})
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
