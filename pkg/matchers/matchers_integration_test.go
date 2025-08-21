package matchers

import (
	"testing"

	"github.com/arthur-debert/dodot/pkg/registry"
	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	// Import to register factories
	_ "github.com/arthur-debert/dodot/pkg/handlers"
	_ "github.com/arthur-debert/dodot/pkg/triggers"
)

// Register test factories for testing
func init() {
	// Register test trigger factory
	_ = registry.RegisterTriggerFactory("test-trigger", func(config map[string]interface{}) (types.Trigger, error) {
		return nil, nil
	})

	// Register test power-up factory
	_ = registry.RegisterHandlerFactory("test-handler", func(config map[string]interface{}) (types.Handler, error) {
		return nil, nil
	})
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
				Handler: "symlink",
			},
			check: func(t *testing.T, m *types.Matcher) {
				assert.Equal(t, "test", m.Name)
				assert.Equal(t, "filename", m.TriggerName)
				assert.Equal(t, "symlink", m.HandlerName)
				assert.True(t, m.Enabled)
			},
		},
		{
			name: "with pattern convenience field",
			config: &types.MatcherConfig{
				Name:    "pattern-test",
				Trigger: "filename",
				Handler: "symlink",
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
				Handler: "symlink",
				Target:  "/custom/path",
			},
			check: func(t *testing.T, m *types.Matcher) {
				assert.NotNil(t, m.HandlerOptions)
				assert.Equal(t, "/custom/path", m.HandlerOptions["target"])
			},
		},
		{
			name: "with explicit options",
			config: &types.MatcherConfig{
				Name:    "options-test",
				Trigger: "filename",
				Handler: "symlink",
				TriggerOptions: map[string]interface{}{
					"pattern": "specific.file",
				},
				HandlerOptions: map[string]interface{}{
					"target": "/specific/target",
				},
			},
			check: func(t *testing.T, m *types.Matcher) {
				assert.Equal(t, "specific.file", m.TriggerOptions["pattern"])
				assert.Equal(t, "/specific/target", m.HandlerOptions["target"])
			},
		},
		{
			name: "disabled matcher",
			config: &types.MatcherConfig{
				Name:    "disabled-test",
				Trigger: "filename",
				Handler: "symlink",
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
				Handler: "symlink",
			},
			wantErr: true,
		},
		{
			name: "missing handler",
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
				Handler: "symlink",
			},
			wantErr: true,
		},
		{
			name: "unknown handler",
			config: &types.MatcherConfig{
				Name:    "invalid",
				Trigger: "filename",
				Handler: "unknown-handler",
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
				HandlerName: "symlink",
			},
			wantErr: false,
		},
		{
			name: "missing trigger name",
			matcher: &types.Matcher{
				HandlerName: "symlink",
			},
			wantErr: true,
			errMsg:  "trigger name is required",
		},
		{
			name: "missing handler name",
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
				HandlerName: "symlink",
			},
			wantErr: true,
			errMsg:  "unknown trigger: non-existent",
		},
		{
			name: "unknown handler",
			matcher: &types.Matcher{
				TriggerName: "filename",
				HandlerName: "non-existent",
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
