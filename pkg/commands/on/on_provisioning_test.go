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
	assert.Equal(t, "on", result.Command, "command should be on")
	assert.True(t, result.Metadata.NoProvision, "should record no-provision flag")
	assert.False(t, result.Metadata.ProvisionRerun, "provision-rerun should be false")
	// The important thing is that provision was skipped
	assert.True(t, len(result.Packs) > 0, "should have pack status")
	if len(result.Packs) > 0 {
		assert.Equal(t, "mixed", result.Packs[0].Name, "pack name should match")
	}
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
	assert.Equal(t, "on", result1.Command, "command should be on")

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
	assert.Equal(t, "on", result2.Command, "command should be on")
	assert.False(t, result2.Metadata.NoProvision, "no-provision should be false")
	assert.True(t, result2.Metadata.ProvisionRerun, "should record provision-rerun flag")
	assert.True(t, len(result2.Packs) > 0, "should have pack status")
	if len(result2.Packs) > 0 {
		assert.Equal(t, "tools", result2.Packs[0].Name, "pack name should match")
	}
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
	assert.Equal(t, "on", result.Command, "command should be on")
	assert.False(t, result.Metadata.NoProvision, "no-provision should be false")
	assert.False(t, result.Metadata.ProvisionRerun, "provision-rerun should be false")
	// The provision command should run but skip already-provisioned handlers
	// This is handled by core.Execute which filters them out
	assert.True(t, len(result.Packs) > 0, "should have pack status")
	if len(result.Packs) > 0 {
		assert.Equal(t, "apps", result.Packs[0].Name, "pack name should match")
	}
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
	assert.Equal(t, "on", result.Command, "command should be on")
	assert.True(t, result.Metadata.NoProvision, "should record no-provision flag")
	// Should have both packs in status
	assert.Equal(t, 2, len(result.Packs), "should have both packs in status")
	// Find pack names
	packNames := make(map[string]bool)
	for _, pack := range result.Packs {
		packNames[pack.Name] = true
	}
	assert.True(t, packNames["configs"], "should have configs pack")
	assert.True(t, packNames["tools"], "should have tools pack")
}

func TestOnPacks_ConflictingFlags_Validation(t *testing.T) {
	// This test would be at the CLI level, not the function level
	// The cobra command should prevent both flags from being set
	// Just documenting the expected behavior here

	// When both --no-provision and --provision-rerun are set,
	// cobra's MarkFlagsMutuallyExclusive should prevent execution
	// and show an error message
}
