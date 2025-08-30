// pkg/executor/executor_orchestration_test.go
// TEST TYPE: Business Logic Integration
// DEPENDENCIES: Mock DataStore, Memory FS, Mock Actions
// PURPOSE: Test executor orchestration logic for action processing

package executor

import (
	"errors"
	"testing"
	"time"

	"github.com/arthur-debert/dodot/pkg/testutil"
	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/rs/zerolog"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

// mockAction implements types.Action for testing
type mockAction struct {
	packName      string
	description   string
	executeError  error
	executeCalled bool
}

func (m *mockAction) Pack() string        { return m.packName }
func (m *mockAction) Description() string { return m.description }
func (m *mockAction) Execute(ds types.DataStore) error {
	m.executeCalled = true
	return m.executeError
}

func TestNew_CreatesExecutorCorrectly(t *testing.T) {
	t.Run("creates executor with all options", func(t *testing.T) {
		// Arrange
		env := testutil.NewTestEnvironment(t, testutil.EnvMemoryOnly)
		logger := zerolog.Nop()

		opts := Options{
			DataStore: env.DataStore,
			DryRun:    true,
			Logger:    logger,
			FS:        env.FS,
		}

		// Act
		executor := New(opts)

		// Assert
		require.NotNil(t, executor)
		assert.Equal(t, env.DataStore, executor.dataStore)
		assert.True(t, executor.dryRun)
		assert.Equal(t, env.FS, executor.fs)
	})

	t.Run("provides defaults for missing options", func(t *testing.T) {
		// Arrange
		env := testutil.NewTestEnvironment(t, testutil.EnvMemoryOnly)

		opts := Options{
			DataStore: env.DataStore,
			// DryRun defaults to false
			// Logger defaults to logging.GetLogger
			// FS defaults to filesystem.NewOS
		}

		// Act
		executor := New(opts)

		// Assert
		require.NotNil(t, executor)
		assert.Equal(t, env.DataStore, executor.dataStore)
		assert.False(t, executor.dryRun)
		assert.NotNil(t, executor.fs)
	})
}

func TestExecutor_Execute_MultipleActions(t *testing.T) {
	t.Run("orchestrates multiple action execution", func(t *testing.T) {
		// Arrange
		env := testutil.NewTestEnvironment(t, testutil.EnvMemoryOnly)
		executor := New(Options{
			DataStore: env.DataStore,
			DryRun:    false,
			Logger:    zerolog.Nop(),
			FS:        env.FS,
		})

		action1 := &mockAction{packName: "pack1", description: "action 1"}
		action2 := &mockAction{packName: "pack2", description: "action 2"}
		action3 := &mockAction{packName: "pack3", description: "action 3"}
		actions := []types.Action{action1, action2, action3}

		// Act
		results := executor.Execute(actions)

		// Assert
		require.Len(t, results, 3)

		// Verify all actions were executed
		assert.True(t, action1.executeCalled)
		assert.True(t, action2.executeCalled)
		assert.True(t, action3.executeCalled)

		// Verify all results are successful
		for i, result := range results {
			assert.True(t, result.Success)
			assert.False(t, result.Skipped)
			assert.NoError(t, result.Error)
			assert.Equal(t, actions[i], result.Action)
			assert.Greater(t, result.Duration, time.Duration(0))
		}
	})

	t.Run("continues execution despite individual failures", func(t *testing.T) {
		// Arrange
		env := testutil.NewTestEnvironment(t, testutil.EnvMemoryOnly)
		executor := New(Options{
			DataStore: env.DataStore,
			DryRun:    false,
			Logger:    zerolog.Nop(),
			FS:        env.FS,
		})

		action1 := &mockAction{packName: "pack1", description: "success"}
		action2 := &mockAction{packName: "pack2", description: "failure", executeError: errors.New("action failed")}
		action3 := &mockAction{packName: "pack3", description: "success after failure"}
		actions := []types.Action{action1, action2, action3}

		// Act
		results := executor.Execute(actions)

		// Assert
		require.Len(t, results, 3)

		// Verify all actions were attempted
		assert.True(t, action1.executeCalled)
		assert.True(t, action2.executeCalled)
		assert.True(t, action3.executeCalled)

		// Verify result states
		assert.True(t, results[0].Success)
		assert.False(t, results[1].Success)
		assert.Error(t, results[1].Error)
		assert.True(t, results[2].Success)
	})

	t.Run("handles empty action list", func(t *testing.T) {
		// Arrange
		env := testutil.NewTestEnvironment(t, testutil.EnvMemoryOnly)
		executor := New(Options{
			DataStore: env.DataStore,
			DryRun:    false,
			Logger:    zerolog.Nop(),
			FS:        env.FS,
		})

		// Act
		results := executor.Execute([]types.Action{})

		// Assert
		assert.Len(t, results, 0)
	})
}

func TestExecutor_Execute_DryRunBehavior(t *testing.T) {
	t.Run("skips execution in dry run mode", func(t *testing.T) {
		// Arrange
		env := testutil.NewTestEnvironment(t, testutil.EnvMemoryOnly)
		executor := New(Options{
			DataStore: env.DataStore,
			DryRun:    true,
			Logger:    zerolog.Nop(),
			FS:        env.FS,
		})

		action1 := &mockAction{packName: "pack1", description: "dry run test"}
		action2 := &mockAction{packName: "pack2", description: "also dry run"}
		actions := []types.Action{action1, action2}

		// Act
		results := executor.Execute(actions)

		// Assert
		require.Len(t, results, 2)

		// Verify actions were NOT executed
		assert.False(t, action1.executeCalled)
		assert.False(t, action2.executeCalled)

		// Verify dry run results
		for _, result := range results {
			assert.True(t, result.Success)
			assert.True(t, result.Skipped)
			assert.NoError(t, result.Error)
			assert.Equal(t, "Dry run - no changes made", result.Message)
			assert.Greater(t, result.Duration, time.Duration(0))
		}
	})
}

func TestExecutor_handlePostExecution_LinkAction(t *testing.T) {
	t.Run("orchestrates link action post execution", func(t *testing.T) {
		// Arrange
		env := testutil.NewTestEnvironment(t, testutil.EnvMemoryOnly)

		executor := New(Options{
			DataStore: env.DataStore,
			DryRun:    false,
			Logger:    zerolog.Nop(),
			FS:        env.FS,
		})

		linkAction := &types.LinkAction{
			PackName:   "test-pack",
			SourceFile: "source.txt",
			TargetFile: "/virtual/home/target.txt",
		}

		// Act
		err := executor.handlePostExecution(linkAction)

		// Assert
		require.NoError(t, err)

		// Verify datastore interaction - check that Link was called
		mockDS := env.DataStore.(*testutil.MockDataStore)
		calls := mockDS.GetCalls()
		assert.Contains(t, calls, "Link(test-pack,source.txt)")

		// Verify filesystem operations
		// Parent directory should be created
		parentDir := "/virtual/home"
		info, err := env.FS.Stat(parentDir)
		require.NoError(t, err)
		assert.True(t, info.IsDir())

		// Target symlink should be created
		linkTarget, err := env.FS.Readlink("/virtual/home/target.txt")
		require.NoError(t, err)
		// MockDataStore returns /home/.{sourceFile} as the target
		assert.Equal(t, "/home/.source.txt", linkTarget)
	})

	t.Run("handles datastore errors gracefully", func(t *testing.T) {
		// Arrange
		env := testutil.NewTestEnvironment(t, testutil.EnvMemoryOnly)

		mockDS := env.DataStore.(*testutil.MockDataStore)
		mockDS.WithError("Link", errors.New("datastore error"))

		executor := New(Options{
			DataStore: env.DataStore,
			DryRun:    false,
			Logger:    zerolog.Nop(),
			FS:        env.FS,
		})

		linkAction := &types.LinkAction{
			PackName:   "test-pack",
			SourceFile: "source.txt",
			TargetFile: "/virtual/home/target.txt",
		}

		// Act
		err := executor.handlePostExecution(linkAction)

		// Assert
		require.Error(t, err)
		assert.Contains(t, err.Error(), "failed to get intermediate link path")
		assert.Contains(t, err.Error(), "datastore error")
	})

	t.Run("replaces existing symlink", func(t *testing.T) {
		// Arrange
		env := testutil.NewTestEnvironment(t, testutil.EnvMemoryOnly)

		// Create existing file at target location
		targetPath := "/virtual/home/target.txt"
		parentDir := "/virtual/home"
		err := env.FS.MkdirAll(parentDir, 0755)
		require.NoError(t, err)

		// Create existing symlink
		err = env.FS.Symlink("/old/target", targetPath)
		require.NoError(t, err)

		executor := New(Options{
			DataStore: env.DataStore,
			DryRun:    false,
			Logger:    zerolog.Nop(),
			FS:        env.FS,
		})

		linkAction := &types.LinkAction{
			PackName:   "test-pack",
			SourceFile: "source.txt",
			TargetFile: targetPath,
		}

		// Act
		err = executor.handlePostExecution(linkAction)

		// Assert
		require.NoError(t, err)

		// Verify old symlink was replaced
		linkTarget, err := env.FS.Readlink(targetPath)
		require.NoError(t, err)
		assert.Equal(t, "/home/.source.txt", linkTarget)
	})
}

func TestExecutor_handlePostExecution_RunScriptAction(t *testing.T) {
	t.Run("skips execution when not needed", func(t *testing.T) {
		// Arrange
		env := testutil.NewTestEnvironment(t, testutil.EnvMemoryOnly)

		// Setup datastore to indicate provisioning not needed (already provisioned)
		mockDS := env.DataStore.(*testutil.MockDataStore)
		mockDS.WithProvisioningState("test-pack", "test-sentinel", true)

		executor := New(Options{
			DataStore: env.DataStore,
			DryRun:    false,
			Logger:    zerolog.Nop(),
			FS:        env.FS,
		})

		scriptAction := &types.RunScriptAction{
			PackName:     "test-pack",
			ScriptPath:   "/virtual/script.sh",
			SentinelName: "test-sentinel",
			Checksum:     "mock-checksum", // This matches the default mock checksum
		}

		// Act
		err := executor.handlePostExecution(scriptAction)

		// Assert
		require.NoError(t, err)

		// Verify NeedsProvisioning was called
		calls := mockDS.GetCalls()
		assert.Contains(t, calls, "NeedsProvisioning(test-pack,test-sentinel,mock-checksum)")

		// Verify RecordProvisioning was NOT called (since provisioning not needed)
		assert.NotContains(t, calls, "RecordProvisioning(test-pack,test-sentinel,mock-checksum)")
	})

	t.Run("handles datastore errors", func(t *testing.T) {
		// Arrange
		env := testutil.NewTestEnvironment(t, testutil.EnvMemoryOnly)

		mockDS := env.DataStore.(*testutil.MockDataStore)
		mockDS.WithError("NeedsProvisioning", errors.New("datastore error"))

		executor := New(Options{
			DataStore: env.DataStore,
			DryRun:    false,
			Logger:    zerolog.Nop(),
			FS:        env.FS,
		})

		scriptAction := &types.RunScriptAction{
			PackName:     "test-pack",
			ScriptPath:   "/virtual/script.sh",
			SentinelName: "test-sentinel",
			Checksum:     "checksum123",
		}

		// Act
		err := executor.handlePostExecution(scriptAction)

		// Assert
		require.Error(t, err)
		assert.Contains(t, err.Error(), "failed to check provisioning status")
		assert.Contains(t, err.Error(), "datastore error")
	})
}

func TestExecutor_handlePostExecution_BrewAction(t *testing.T) {
	t.Run("skips execution when not needed", func(t *testing.T) {
		// Arrange
		env := testutil.NewTestEnvironment(t, testutil.EnvMemoryOnly)

		sentinelName := "homebrew-test-pack.sentinel"
		mockDS := env.DataStore.(*testutil.MockDataStore)
		mockDS.WithProvisioningState("test-pack", sentinelName, true)

		executor := New(Options{
			DataStore: env.DataStore,
			DryRun:    false,
			Logger:    zerolog.Nop(),
			FS:        env.FS,
		})

		brewAction := &types.BrewAction{
			PackName:     "test-pack",
			BrewfilePath: "/virtual/Brewfile",
			Checksum:     "mock-checksum",
		}

		// Act
		err := executor.handlePostExecution(brewAction)

		// Assert
		require.NoError(t, err)

		// Verify NeedsProvisioning was called
		calls := mockDS.GetCalls()
		assert.Contains(t, calls, "NeedsProvisioning(test-pack,homebrew-test-pack.sentinel,mock-checksum)")

		// Verify RecordProvisioning was NOT called (since provisioning not needed)
		assert.NotContains(t, calls, "RecordProvisioning(test-pack,homebrew-test-pack.sentinel,mock-checksum)")
	})

	t.Run("handles datastore errors", func(t *testing.T) {
		// Arrange
		env := testutil.NewTestEnvironment(t, testutil.EnvMemoryOnly)

		mockDS := env.DataStore.(*testutil.MockDataStore)
		mockDS.WithError("NeedsProvisioning", errors.New("datastore error"))

		executor := New(Options{
			DataStore: env.DataStore,
			DryRun:    false,
			Logger:    zerolog.Nop(),
			FS:        env.FS,
		})

		brewAction := &types.BrewAction{
			PackName:     "test-pack",
			BrewfilePath: "/virtual/Brewfile",
			Checksum:     "checksum123",
		}

		// Act
		err := executor.handlePostExecution(brewAction)

		// Assert
		require.Error(t, err)
		assert.Contains(t, err.Error(), "failed to check provisioning status")
		assert.Contains(t, err.Error(), "datastore error")
	})
}

func TestExecutor_handlePostExecution_UnknownAction(t *testing.T) {
	t.Run("returns no error for unknown action types", func(t *testing.T) {
		// Arrange
		env := testutil.NewTestEnvironment(t, testutil.EnvMemoryOnly)
		executor := New(Options{
			DataStore: env.DataStore,
			DryRun:    false,
			Logger:    zerolog.Nop(),
			FS:        env.FS,
		})

		unknownAction := &mockAction{packName: "test", description: "unknown action"}

		// Act
		err := executor.handlePostExecution(unknownAction)

		// Assert
		require.NoError(t, err)
	})
}

func TestExecutor_executeAction_ErrorHandling(t *testing.T) {
	t.Run("handles action execution errors", func(t *testing.T) {
		// Arrange
		env := testutil.NewTestEnvironment(t, testutil.EnvMemoryOnly)
		executor := New(Options{
			DataStore: env.DataStore,
			DryRun:    false,
			Logger:    zerolog.Nop(),
			FS:        env.FS,
		})

		failingAction := &mockAction{
			packName:     "test-pack",
			description:  "failing action",
			executeError: errors.New("execution failed"),
		}

		// Act
		result := executor.executeAction(failingAction)

		// Assert
		assert.False(t, result.Success)
		assert.False(t, result.Skipped)
		assert.Error(t, result.Error)
		assert.Equal(t, "execution failed", result.Error.Error())
		assert.Equal(t, failingAction, result.Action)
		assert.Greater(t, result.Duration, time.Duration(0))
	})

	t.Run("handles post-execution errors", func(t *testing.T) {
		// Arrange
		env := testutil.NewTestEnvironment(t, testutil.EnvMemoryOnly)

		executor := New(Options{
			DataStore: env.DataStore,
			DryRun:    false,
			Logger:    zerolog.Nop(),
			FS:        env.FS,
		})

		// Create a target path that will cause filesystem error in post-execution
		// Create a regular file where we want to create a directory
		targetFile := "/virtual/home/target.txt"
		err := env.FS.WriteFile("/virtual/home", []byte("blocking file"), 0644)
		require.NoError(t, err)

		linkAction := &types.LinkAction{
			PackName:   "test-pack",
			SourceFile: "source.txt",
			TargetFile: targetFile,
		}

		// Act
		result := executor.executeAction(linkAction)

		// Assert
		assert.False(t, result.Success)
		assert.False(t, result.Skipped)
		assert.Error(t, result.Error)
		// This should fail in post-execution during directory creation
		assert.Contains(t, result.Error.Error(), "failed to create target directory")
		assert.Equal(t, linkAction, result.Action)
	})
}
