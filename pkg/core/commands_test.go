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
	testutil.AssertEqual(t, 2, len(result.Packs), "expected two non-hidden, non-disabled packs")

	// Check pack names (they should be sorted)
	expectedNames := []string{"pack-one", "pack-two"}
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

func TestInstallPacks(t *testing.T) {
	root, _ := setupExecutionTest(t)

	opts := InstallPacksOptions{
		DotfilesRoot: root,
		PackNames:    []string{"test-pack"},
	}

	result, err := InstallPacks(opts)

	testutil.AssertNoError(t, err)
	testutil.AssertNotNil(t, result)

	// Assert that operations from BOTH power-ups are present.
	hasManyOp := false
	hasOnceOp := false
	for _, op := range result.Operations {
		if strings.Contains(op.Description, "many-powerup") {
			hasManyOp = true
		}
		if op.Type == types.OperationWriteFile && strings.Contains(op.Target, filepath.Join("install", "test-pack")) {
			hasOnceOp = true
		}
	}

	testutil.AssertTrue(t, hasManyOp, "Install result should contain operations from many-powerup")
	testutil.AssertTrue(t, hasOnceOp, "Install result should contain operations from once-powerup")
}

// setupExecutionTest creates a temporary directory structure for testing deploy/install.
func setupExecutionTest(t *testing.T) (root, packPath string) {
	t.Helper()

	// Mock Power-up for RunModeOnce
	oncePowerUp := &testutil.MockPowerUp{
		NameFunc:    func() string { return "once-powerup" },
		RunModeFunc: func() types.RunMode { return types.RunModeOnce },
		ProcessFunc: func(matches []types.TriggerMatch) ([]types.Action, error) {
			// A real install action includes metadata needed by GetFsOps
			return []types.Action{{
				Type:        types.ActionTypeInstall,
				Description: "Install action from once-powerup",
				PowerUpName: "once-powerup",
				Source:      matches[0].AbsolutePath,
				Pack:        matches[0].Pack,
				Metadata: map[string]interface{}{
					"checksum": "dummy-checksum",
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
	// This uses the `[files]` table which is processed by `GetFiringTriggers`.
	dodotToml := `
[files]
"install.me" = "once-powerup"
"link.me" = "many-powerup"
`
	testutil.CreateFile(t, packPath, ".dodot.toml", dodotToml)

	// Also need to ensure the base directories for operations exist to avoid errors
	// during the test run when GetFsOps tries to create paths.
	// This is a bit of a leak from the implementation, but necessary for the test.
	// In a real run, these dirs are in ~/.local/share/dodot
	// For the test, we can just create them in a temp dir.
	tempStateDir := testutil.TempDir(t, "dodot-state")
	testutil.Setenv(t, "HOME", tempStateDir) // Redirect home to a temp dir for isolation

	// Pre-create the directories that GetFsOps will expect, relative to the temp home
	testutil.CreateDir(t, tempStateDir, ".local/share/dodot/symlinks")
	testutil.CreateDir(t, tempStateDir, ".local/share/dodot/install")

	return root, packPath
}