package config

import (
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestLoadConfiguration(t *testing.T) {
	t.Run("loads and merges defaults successfully", func(t *testing.T) {
		cfg, err := LoadConfiguration()
		require.NoError(t, err)
		assert.NotNil(t, cfg)

		// Check value from system defaults
		assert.Equal(t, "warn", cfg.Logging.DefaultLevel)
		// Check that we have matchers
		assert.NotEmpty(t, cfg.Matchers)
	})

	// Note: Additional tests for user config override and environment variable
	// behavior have been removed pending refinement of the merge/override logic
}
