package install

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/arthur-debert/dodot/pkg/testutil"
	"github.com/arthur-debert/dodot/pkg/types"
)

func TestInstallScriptPowerUp_Basic(t *testing.T) {
	powerup := NewInstallScriptPowerUp()

	testutil.AssertEqual(t, InstallScriptPowerUpName, powerup.Name())
	testutil.AssertEqual(t, "Runs install.sh scripts for initial setup", powerup.Description())
	testutil.AssertEqual(t, types.RunModeOnce, powerup.RunMode())
}

func TestInstallScriptPowerUp_Process(t *testing.T) {
	// Create test files
	tmpDir := testutil.TempDir(t, "install-test")

	// Create a test install script
	installPath := filepath.Join(tmpDir, "install.sh")
	installContent := `#!/bin/bash
echo "Installing..."
npm install -g typescript`
	err := os.WriteFile(installPath, []byte(installContent), 0755)
	testutil.AssertNoError(t, err)

	powerup := NewInstallScriptPowerUp()

	matches := []types.TriggerMatch{
		{
			Path:         "install.sh",
			AbsolutePath: installPath,
			Pack:         "dev",
			Priority:     100,
		},
	}

	actions, err := powerup.Process(matches)
	testutil.AssertNoError(t, err)
	testutil.AssertEqual(t, 1, len(actions)) // Only install action now

	// Single action should be install
	installAction := actions[0]
	testutil.AssertEqual(t, types.ActionTypeInstall, installAction.Type)
	testutil.AssertEqual(t, installPath, installAction.Source)
	testutil.AssertEqual(t, installPath, installAction.Command)
	testutil.AssertEqual(t, "", installAction.Target)
	testutil.AssertEqual(t, "dev", installAction.Pack)
	testutil.AssertEqual(t, InstallScriptPowerUpName, installAction.PowerUpName)
	testutil.AssertEqual(t, 100, installAction.Priority)
	testutil.AssertContains(t, installAction.Description, "Run install script")
	testutil.AssertNotNil(t, installAction.Args)
	testutil.AssertEqual(t, 0, len(installAction.Args))

	// Check metadata
	testutil.AssertNotNil(t, installAction.Metadata)
	testutil.AssertEqual(t, "dev", installAction.Metadata["pack"])
	// Checksum should NOT be in metadata anymore
	_, hasChecksum := installAction.Metadata["checksum"]
	testutil.AssertFalse(t, hasChecksum, "Checksum should not be in metadata")
}

func TestInstallScriptPowerUp_Process_MultipleMatches(t *testing.T) {
	tmpDir := testutil.TempDir(t, "install-test")

	// Create multiple install scripts
	install1 := filepath.Join(tmpDir, "install1.sh")
	install2 := filepath.Join(tmpDir, "install2.sh")

	err := os.WriteFile(install1, []byte("#!/bin/bash\necho \"Install 1\""), 0755)
	testutil.AssertNoError(t, err)
	err = os.WriteFile(install2, []byte("#!/bin/bash\necho \"Install 2\""), 0755)
	testutil.AssertNoError(t, err)

	powerup := NewInstallScriptPowerUp()

	matches := []types.TriggerMatch{
		{
			Path:         "install1.sh",
			AbsolutePath: install1,
			Pack:         "pack1",
			Priority:     100,
		},
		{
			Path:         "install2.sh",
			AbsolutePath: install2,
			Pack:         "pack2",
			Priority:     200,
		},
	}

	actions, err := powerup.Process(matches)
	testutil.AssertNoError(t, err)
	testutil.AssertEqual(t, 2, len(actions)) // 2 install actions only

	// Verify actions are in correct order
	// First install for pack1
	testutil.AssertEqual(t, types.ActionTypeInstall, actions[0].Type)
	testutil.AssertEqual(t, "pack1", actions[0].Pack)
	testutil.AssertEqual(t, 100, actions[0].Priority)

	// Then install for pack2
	testutil.AssertEqual(t, types.ActionTypeInstall, actions[1].Type)
	testutil.AssertEqual(t, "pack2", actions[1].Pack)
	testutil.AssertEqual(t, 200, actions[1].Priority)
}

func TestInstallScriptPowerUp_Process_ChecksumError(t *testing.T) {
	powerup := NewInstallScriptPowerUp()

	matches := []types.TriggerMatch{
		{
			Path:         "install.sh",
			AbsolutePath: "/non/existent/file",
			Pack:         "dev",
			Priority:     100,
		},
	}

	// PowerUp should create action even with non-existent file
	actions, err := powerup.Process(matches)
	testutil.AssertNoError(t, err)
	testutil.AssertEqual(t, 1, len(actions))

	// Single action should be install
	testutil.AssertEqual(t, types.ActionTypeInstall, actions[0].Type)
	testutil.AssertEqual(t, "/non/existent/file", actions[0].Source)
}

func TestInstallScriptPowerUp_ValidateOptions(t *testing.T) {
	powerup := NewInstallScriptPowerUp()

	// Install script power-up doesn't have options, so any options should be accepted
	err := powerup.ValidateOptions(nil)
	testutil.AssertNoError(t, err)

	err = powerup.ValidateOptions(map[string]interface{}{})
	testutil.AssertNoError(t, err)

	err = powerup.ValidateOptions(map[string]interface{}{
		"some": "option",
	})
	testutil.AssertNoError(t, err)
}
