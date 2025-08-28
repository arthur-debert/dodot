// Test Type: Unit Test
// Description: Tests for the packs package - config loading wrapper

package packs_test

import (
	"testing"
)

func TestLoadPackConfig(t *testing.T) {
	// LoadPackConfig is a simple wrapper around config.LoadPackConfig
	// The actual functionality is tested in pkg/config tests
	// This just documents that the function exists as a public API
	t.Run("function_exists_as_wrapper", func(t *testing.T) {
		// The function packs.LoadPackConfig exists and delegates to config.LoadPackConfig
		// No additional testing needed here as it's a simple delegation
		t.Skip("LoadPackConfig is a wrapper - actual tests in pkg/config")
	})
}
