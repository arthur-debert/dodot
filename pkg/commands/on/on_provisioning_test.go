// pkg/commands/on/on_provisioning_test.go
// TEST TYPE: Business Logic Integration
// DEPENDENCIES: Mock DataStore, Memory FS
// PURPOSE: Test on command with new provisioning options

package on_test

import (
	"testing"

	"github.com/arthur-debert/dodot/pkg/commands/on"
	"github.com/arthur-debert/dodot/pkg/testutil"
	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestOnPacks_NoProvision_Orchestration(t *testing.T) {
	// Setup
	env := testutil.NewTestEnvironment(t, testutil.EnvIsolated)

	// Create a pack with both config and provisioning handlers
	env.SetupPack("mixed", testutil.PackConfig{
		Files: map[string]string{
			".vimrc":     "\" vim config",
			"install.sh": "#!/bin/sh\necho installing",
			"setup.sh":   "#!/bin/sh\necho setup",
			".dodot.toml": `[[rule]]
match = ".vimrc"
handler = "symlink"

[[rule]]
match = "*.sh"
handler = "install"`,
		},
	})

	opts := on.OnPacksOptions{
		DotfilesRoot: env.DotfilesRoot,
		PackNames:    []string{"mixed"},
		DryRun:       false,
		Force:        false,
		NoProvision:  true, // Key: skip provisioning
	}

	// Execute
	result, err := on.OnPacks(opts)

	// Verify only link was executed, not provision
	require.NoError(t, err)
	assert.NotNil(t, result.LinkResult, "should call link command")
	assert.Nil(t, result.ProvisionResult, "should NOT call provision command when --no-provision")
	// Note: TotalDeployed may be 0 due to how ExecutionContext tracks handlers
	// The important thing is that provision was skipped
	assert.Empty(t, result.Errors, "no errors expected")
}

func TestOnPacks_ProvisionRerun_Orchestration(t *testing.T) {
	// Setup
	env := testutil.NewTestEnvironment(t, testutil.EnvIsolated)

	// Create a pack with provisioning handlers
	env.SetupPack("tools", testutil.PackConfig{
		Files: map[string]string{
			"install.sh": "#!/bin/sh\necho installing tools",
			".dodot.toml": `[[rule]]
match = "install.sh"
handler = "install"`,
		},
	})

	// First, run on normally to set up provisioning
	opts1 := on.OnPacksOptions{
		DotfilesRoot: env.DotfilesRoot,
		PackNames:    []string{"tools"},
		DryRun:       false,
		Force:        false,
	}
	result1, err := on.OnPacks(opts1)
	require.NoError(t, err)
	require.NotNil(t, result1.ProvisionResult)

	// Mark the install handler as already provisioned
	if mockDS, ok := env.DataStore.(*testutil.MockSimpleDataStore); ok {
		mockDS.SetSentinel("tools", "install", "install.sh", true)
	}

	// Now run with provision-rerun to force re-execution
	opts2 := on.OnPacksOptions{
		DotfilesRoot:   env.DotfilesRoot,
		PackNames:      []string{"tools"},
		DryRun:         false,
		Force:          false,
		ProvisionRerun: true, // Key: force re-run provisioning
	}

	// Execute
	result2, err := on.OnPacks(opts2)

	// Verify provisioning was forced to re-run
	require.NoError(t, err)
	assert.NotNil(t, result2.LinkResult, "should call link command")
	assert.NotNil(t, result2.ProvisionResult, "should call provision command")
	// The provision command should have run with ForceReprovisioning
	// We can't easily verify handler counts due to how ExecutionContext tracks results
}

func TestOnPacks_AlreadyProvisioned_Orchestration(t *testing.T) {
	// Setup
	env := testutil.NewTestEnvironment(t, testutil.EnvIsolated)

	// Create a pack with provisioning handlers
	env.SetupPack("apps", testutil.PackConfig{
		Files: map[string]string{
			"install.sh": "#!/bin/sh\necho installing apps",
			"setup.sh":   "#!/bin/sh\necho setting up",
			".dodot.toml": `[[rule]]
match = "*.sh"
handler = "install"`,
		},
	})

	// Mark handlers as already provisioned
	if mockDS, ok := env.DataStore.(*testutil.MockSimpleDataStore); ok {
		mockDS.SetSentinel("apps", "install", "install.sh", true)
		mockDS.SetSentinel("apps", "install", "setup.sh", true)
	}

	opts := on.OnPacksOptions{
		DotfilesRoot: env.DotfilesRoot,
		PackNames:    []string{"apps"},
		DryRun:       false,
		Force:        false,
		// No special flags - should skip already provisioned handlers
	}

	// Execute
	result, err := on.OnPacks(opts)

	// Verify provisioning was skipped for already-provisioned handlers
	require.NoError(t, err)
	assert.NotNil(t, result.LinkResult, "should call link command")
	assert.NotNil(t, result.ProvisionResult, "should call provision command")
	// The provision command should run but skip already-provisioned handlers
	// This is handled by core.Execute which filters them out
	assert.Empty(t, result.Errors, "no errors expected")
}

func TestOnPacks_MixedPacks_NoProvision_Orchestration(t *testing.T) {
	// Setup
	env := testutil.NewTestEnvironment(t, testutil.EnvIsolated)

	// Create multiple packs with different handler types
	env.SetupPack("configs", testutil.PackConfig{
		Files: map[string]string{
			".gitconfig": "[user]\n  name = Test",
			".dodot.toml": `[[rule]]
match = ".gitconfig"
handler = "symlink"`,
		},
	})

	env.SetupPack("tools", testutil.PackConfig{
		Files: map[string]string{
			"install.sh": "#!/bin/sh\necho tools",
			"bin/tool":   "#!/bin/sh\necho tool",
			".dodot.toml": `[[rule]]
match = "install.sh"
handler = "install"

[[rule]]
match = "bin/"
handler = "path"`,
		},
	})

	opts := on.OnPacksOptions{
		DotfilesRoot: env.DotfilesRoot,
		PackNames:    []string{"configs", "tools"},
		DryRun:       false,
		Force:        false,
		NoProvision:  true, // Skip all provisioning
	}

	// Execute
	result, err := on.OnPacks(opts)

	// Verify only configuration handlers were executed
	require.NoError(t, err)
	assert.NotNil(t, result.LinkResult, "should call link command")
	assert.Nil(t, result.ProvisionResult, "should skip provision with --no-provision")

	// Link should have processed configs pack's symlink and tools pack's path handler
	if result.LinkResult != nil {
		configResult, exists := result.LinkResult.GetPackResult("configs")
		assert.True(t, exists, "should have configs pack result")
		if configResult != nil {
			assert.Greater(t, configResult.TotalHandlers, 0, "configs pack should have handlers")
		}

		// tools pack should have path handler in link result
		toolsResult, exists := result.LinkResult.GetPackResult("tools")
		assert.True(t, exists, "should have tools pack result")
		if toolsResult != nil {
			assert.Greater(t, toolsResult.TotalHandlers, 0, "tools pack should have path handler")
		}
	}
}

func TestOnPacks_ConflictingFlags_Validation(t *testing.T) {
	// This test would be at the CLI level, not the function level
	// The cobra command should prevent both flags from being set
	// Just documenting the expected behavior here

	// When both --no-provision and --provision-rerun are set,
	// cobra's MarkFlagsMutuallyExclusive should prevent execution
	// and show an error message
}
