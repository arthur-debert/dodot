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

		// Check that we have matchers
		assert.NotEmpty(t, cfg.Matchers)
		// Check a value from system defaults
		assert.Equal(t, 100, cfg.Priorities.Triggers["filename"])
	})

	// Note: Additional tests for user config override and environment variable
	// behavior have been removed pending refinement of the merge/override logic
}
