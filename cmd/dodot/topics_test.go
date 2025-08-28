package dodot

import (
	"testing"

	"github.com/spf13/cobra"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"

	// Import handler packages to register their factories
	_ "github.com/arthur-debert/dodot/pkg/handlers/homebrew"
	_ "github.com/arthur-debert/dodot/pkg/handlers/install"
	_ "github.com/arthur-debert/dodot/pkg/handlers/path"
	_ "github.com/arthur-debert/dodot/pkg/handlers/shell"
	_ "github.com/arthur-debert/dodot/pkg/handlers/symlink"
)

func TestTopicsCommand(t *testing.T) {
	// The topics command is implemented as a simple wrapper that calls "help topics"
	// Since the help system initialization depends on finding topic files relative
	// to the executable, it's difficult to test the full functionality in a unit test.
	// This test focuses on verifying the command structure and basic behavior.

	t.Run("topics command exists and has correct structure", func(t *testing.T) {
		cmd := NewRootCmd()

		// Find the topics command
		var topicsCmd *cobra.Command
		for _, c := range cmd.Commands() {
			if c.Name() == "topics" {
				topicsCmd = c
				break
			}
		}

		require.NotNil(t, topicsCmd, "topics command should exist")
		assert.Equal(t, "topics", topicsCmd.Use)
		assert.Equal(t, MsgTopicsShort, topicsCmd.Short)
		assert.Equal(t, MsgTopicsLong, topicsCmd.Long)
		assert.Equal(t, "misc", topicsCmd.GroupID)
		assert.NotNil(t, topicsCmd.RunE, "topics command should have RunE function")
	})

	t.Run("topics command returns expected error when help not found", func(t *testing.T) {
		// In the test environment, the help command is not properly initialized
		// because it depends on finding topic files relative to the executable.
		// The topics command should return "help command not found" error.

		cmd := NewRootCmd()
		cmd.SetArgs([]string{"topics"})

		err := cmd.Execute()
		assert.Error(t, err)
		assert.Contains(t, err.Error(), "help command not found")
	})

	t.Run("topics command has no subcommands", func(t *testing.T) {
		cmd := NewRootCmd()

		var topicsCmd *cobra.Command
		for _, c := range cmd.Commands() {
			if c.Name() == "topics" {
				topicsCmd = c
				break
			}
		}

		require.NotNil(t, topicsCmd)
		assert.Empty(t, topicsCmd.Commands(), "topics command should have no subcommands")
	})

	t.Run("topics command has no special flags", func(t *testing.T) {
		cmd := NewRootCmd()

		var topicsCmd *cobra.Command
		for _, c := range cmd.Commands() {
			if c.Name() == "topics" {
				topicsCmd = c
				break
			}
		}

		require.NotNil(t, topicsCmd)
		// Check that topics command has no local flags (only inherits persistent flags)
		assert.False(t, topicsCmd.HasLocalFlags(), "topics command should not have local flags")
	})

	t.Run("topics command implementation", func(t *testing.T) {
		// Verify that the topics command implementation tries to find and execute
		// the help command with "topics" as an argument

		cmd := NewRootCmd()
		var topicsCmd *cobra.Command
		for _, c := range cmd.Commands() {
			if c.Name() == "topics" {
				topicsCmd = c
				break
			}
		}

		require.NotNil(t, topicsCmd)
		require.NotNil(t, topicsCmd.RunE)

		// The RunE function should attempt to find the help command
		// In test environment, this will fail with "help command not found"
		err := topicsCmd.RunE(topicsCmd, []string{})
		assert.Error(t, err)
		assert.Contains(t, err.Error(), "help command not found")
	})
}

func TestTopicsCommandMessages(t *testing.T) {
	// Verify that the topics command uses the correct message constants

	t.Run("message constants are defined", func(t *testing.T) {
		assert.NotEmpty(t, MsgTopicsShort, "MsgTopicsShort should be defined")
		assert.NotEmpty(t, MsgTopicsLong, "MsgTopicsLong should be defined")
	})

	t.Run("messages are properly formatted", func(t *testing.T) {
		// Basic checks that messages don't have obvious issues
		assert.NotContains(t, MsgTopicsShort, "\n", "Short description should be single line")
		assert.Greater(t, len(MsgTopicsLong), len(MsgTopicsShort),
			"Long description should be longer than short description")
	})
}

// Note: Full integration testing of the topics command with actual topic files
// would require either:
// 1. Running the test with a compiled binary and proper directory structure
// 2. Mocking the help system initialization
// 3. Testing at a higher level (e.g., e2e tests)
//
// The current tests verify the command structure and basic behavior, which is
// appropriate for unit testing. The actual topic display functionality is
// tested in the pkg/cobrax/topics package.
