package core

import (
	"path/filepath"
	"strings"
	"testing"

	"github.com/arthur-debert/dodot/pkg/registry"
	"github.com/arthur-debert/dodot/pkg/testutil"
	"github.com/arthur-debert/dodot/pkg/types"
)

func TestListPacks(t *testing.T) {
	// Setup: Create a temporary dotfiles directory with some packs
	root := testutil.TempDir(t, "list-packs-test")
	testutil.CreateDir(t, root, "pack-one")
	testutil.CreateDir(t, root, "pack-two")
	testutil.CreateDir(t, root, ".hidden-pack") // Should be ignored

	// Create a pack with a config that disables it
	disabledPackPath := testutil.CreateDir(t, root, "disabled-pack")
	testutil.CreateFile(t, disabledPackPath, ".dodot.toml", `disabled = true`)

	// Execute
	opts := ListPacksOptions{DotfilesRoot: root}
	result, err := ListPacks(opts)

	// Assert
	testutil.AssertNoError(t, err)
	testutil.AssertNotNil(t, result)
	testutil.AssertEqual(t, 3, len(result.Packs), "expected three non-hidden packs")

	// Check pack names (they should be sorted)
	expectedNames := []string{"disabled-pack", "pack-one", "pack-two"}
	for i, pack := range result.Packs {
		testutil.AssertEqual(t, expectedNames[i], pack.Name)
	}
}

func TestListPacks_NoPacks(t *testing.T) {
	// Setup: Create an empty temporary directory
	root := testutil.TempDir(t, "list-packs-empty-test")

	// Execute
	opts := ListPacksOptions{DotfilesRoot: root}
	result, err := ListPacks(opts)

	// Assert
	testutil.AssertNoError(t, err)
	testutil.AssertNotNil(t, result)
	testutil.AssertEqual(t, 0, len(result.Packs), "expected zero packs in an empty directory")
}

func TestListPacks_NonExistentRoot(t *testing.T) {
	// Setup: A path that does not exist
	root := "/non/existent/path/for/testing"

	// Execute
	opts := ListPacksOptions{DotfilesRoot: root}
	_, err := ListPacks(opts)

	// Assert
	testutil.AssertError(t, err, "expected an error for a non-existent root directory")
}

func TestDeployPacks(t *testing.T) {
	root, _ := setupExecutionTest(t)

	opts := DeployPacksOptions{
		DotfilesRoot: root,
		PackNames:    []string{"test-pack"},
	}

	result, err := DeployPacks(opts)

	testutil.AssertNoError(t, err)
	testutil.AssertNotNil(t, result)

	// Assert that only operations from the 'many' power-up are present.
	hasManyOp := false
	hasOnceOp := false
	for _, op := range result.Operations {
		if strings.Contains(op.Description, "many-powerup") {
			hasManyOp = true
		}
		if strings.Contains(op.Target, "install") {
			hasOnceOp = true
		}
	}

	testutil.AssertTrue(t, hasManyOp, "Deploy result should contain operations from many-powerup")
	testutil.AssertFalse(t, hasOnceOp, "Deploy result should NOT contain operations from once-powerup")
}

func TestInstallPacks_RunOnceFiltering(t *testing.T) {
	root, packPath := setupExecutionTest(t)

	opts := InstallPacksOptions{
		DotfilesRoot: root,
		PackNames:    []string{"test-pack"},
	}

	// --- First Run ---
	result1, err := InstallPacks(opts)
	testutil.AssertNoError(t, err)
	testutil.AssertNotNil(t, result1)

	// Assert that operations from BOTH power-ups are present on the first run
	var hasManyOp1, hasOnceOp1 bool
	for _, op := range result1.Operations {
		if strings.Contains(op.Description, "many-powerup") {
			hasManyOp1 = true
		}
		if op.Type == types.OperationWriteFile && strings.Contains(op.Target, filepath.Join("install", "test-pack")) {
			hasOnceOp1 = true
			// Simulate the operation by creating the sentinel file for the next run
			testutil.CreateDir(t, filepath.Dir(op.Target), "")
			testutil.CreateFile(t, filepath.Dir(op.Target), filepath.Base(op.Target), op.Content)
		}
	}
	testutil.AssertTrue(t, hasManyOp1, "First install should contain operations from many-powerup")
	testutil.AssertTrue(t, hasOnceOp1, "First install should contain operations from once-powerup")

	// --- Second Run ---
	result2, err := InstallPacks(opts)
	testutil.AssertNoError(t, err)
	testutil.AssertNotNil(t, result2)

	// Assert that only the 'many' power-up operation is present on the second run
	var hasManyOp2, hasOnceOp2 bool
	for _, op := range result2.Operations {
		if strings.Contains(op.Description, "many-powerup") {
			hasManyOp2 = true
		}
		if op.Type == types.OperationWriteFile && strings.Contains(op.Target, filepath.Join("install", "test-pack")) {
			hasOnceOp2 = true
		}
	}
	testutil.AssertTrue(t, hasManyOp2, "Second install should still contain operations from many-powerup")
	testutil.AssertFalse(t, hasOnceOp2, "Second install should NOT contain operations from once-powerup")

	// --- Third Run (with --force) ---
	opts.Force = true
	result3, err := InstallPacks(opts)
	testutil.AssertNoError(t, err)
	testutil.AssertNotNil(t, result3)

	// Assert that operations from BOTH power-ups are present when forced
	var hasManyOp3, hasOnceOp3 bool
	for _, op := range result3.Operations {
		if strings.Contains(op.Description, "many-powerup") {
			hasManyOp3 = true
		}
		if op.Type == types.OperationWriteFile && strings.Contains(op.Target, filepath.Join("install", "test-pack")) {
			hasOnceOp3 = true
		}
	}
	testutil.AssertTrue(t, hasManyOp3, "Forced install should contain operations from many-powerup")
	testutil.AssertTrue(t, hasOnceOp3, "Forced install should contain operations from once-powerup")

	// --- Fourth Run (after file change) ---
	opts.Force = false
	// Modify the file to change its checksum
	testutil.CreateFile(t, packPath, "install.me", "new content")
	result4, err := InstallPacks(opts)
	testutil.AssertNoError(t, err)
	testutil.AssertNotNil(t, result4)

	// Assert that operations from BOTH power-ups are present after a change
	var hasManyOp4, hasOnceOp4 bool
	for _, op := range result4.Operations {
		if strings.Contains(op.Description, "many-powerup") {
			hasManyOp4 = true
		}
		if op.Type == types.OperationWriteFile && strings.Contains(op.Target, filepath.Join("install", "test-pack")) {
			hasOnceOp4 = true
		}
	}
	testutil.AssertTrue(t, hasManyOp4, "Post-change install should contain operations from many-powerup")
	testutil.AssertTrue(t, hasOnceOp4, "Post-change install should contain operations from once-powerup")
}

// setupExecutionTest creates a temporary directory structure for testing deploy/install.
func setupExecutionTest(t *testing.T) (root, packPath string) {
	t.Helper()

	// Mock Power-up for RunModeOnce
	oncePowerUp := &testutil.MockPowerUp{
		NameFunc:    func() string { return "once-powerup" },
		RunModeFunc: func() types.RunMode { return types.RunModeOnce },
		ProcessFunc: func(matches []types.TriggerMatch) ([]types.Action, error) {
			checksum, err := testutil.CalculateFileChecksum(matches[0].AbsolutePath)
			if err != nil {
				return nil, err
			}
			return []types.Action{{
				Type:        types.ActionTypeInstall,
				Description: "Install action from once-powerup",
				PowerUpName: "once-powerup",
				Source:      matches[0].AbsolutePath,
				Pack:        matches[0].Pack,
				Metadata: map[string]interface{}{
					"checksum": checksum,
					"pack":     matches[0].Pack,
				},
			}}, nil
		},
	}

	// Mock Power-up for RunModeMany
	manyPowerUp := &testutil.MockPowerUp{
		NameFunc:    func() string { return "many-powerup" },
		RunModeFunc: func() types.RunMode { return types.RunModeMany },
		ProcessFunc: func(matches []types.TriggerMatch) ([]types.Action, error) {
			return []types.Action{{
				Type:        types.ActionTypeLink,
				Description: "Link action from many-powerup",
				PowerUpName: "many-powerup",
				Source:      matches[0].AbsolutePath,
				Target:      "~/linked-file",
			}}, nil
		},
	}

	// Register mock power-up factories
	powerUpReg := registry.GetRegistry[types.PowerUpFactory]()
	_ = powerUpReg.Register("once-powerup", func(o map[string]interface{}) (types.PowerUp, error) { return oncePowerUp, nil })
	_ = powerUpReg.Register("many-powerup", func(o map[string]interface{}) (types.PowerUp, error) { return manyPowerUp, nil })

	t.Cleanup(func() {
		_ = powerUpReg.Remove("once-powerup")
		_ = powerUpReg.Remove("many-powerup")
	})

	// Test directory
	root = testutil.TempDir(t, "exec-test")
	packPath = testutil.CreateDir(t, root, "test-pack")

	// Create files to be matched by the power-ups
	testutil.CreateFile(t, packPath, "install.me", "content for install")
	testutil.CreateFile(t, packPath, "link.me", "content for link")

	// Create a .dodot.toml to map files to our mock power-ups.
	dodotToml := `
[[override]]
path = "install.me"
powerup = "once-powerup"

[[override]]
path = "link.me"
powerup = "many-powerup"
`
	testutil.CreateFile(t, packPath, ".dodot.toml", dodotToml)

	tempStateDir := testutil.TempDir(t, "dodot-state")
	testutil.Setenv(t, "HOME", tempStateDir)

	testutil.CreateDir(t, tempStateDir, ".local/share/dodot/symlinks")
	testutil.CreateDir(t, tempStateDir, ".local/share/dodot/install")

	return root, packPath
}
