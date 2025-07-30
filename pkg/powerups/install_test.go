package powerups

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

	// Calculate expected checksum
	expectedChecksum, err := testutil.CalculateFileChecksum(installPath)
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
	testutil.AssertEqual(t, 1, len(actions))

	action := actions[0]
	testutil.AssertEqual(t, types.ActionTypeInstall, action.Type)
	testutil.AssertEqual(t, installPath, action.Source)
	testutil.AssertEqual(t, installPath, action.Command)
	testutil.AssertEqual(t, "", action.Target)
	testutil.AssertEqual(t, "dev", action.Pack)
	testutil.AssertEqual(t, InstallScriptPowerUpName, action.PowerUpName)
	testutil.AssertEqual(t, 100, action.Priority)
	testutil.AssertContains(t, action.Description, "Run install script")
	testutil.AssertNotNil(t, action.Args)
	testutil.AssertEqual(t, 0, len(action.Args))

	// Check metadata
	testutil.AssertNotNil(t, action.Metadata)
	testutil.AssertEqual(t, expectedChecksum, action.Metadata["checksum"])
	testutil.AssertEqual(t, "dev", action.Metadata["pack"])
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
	testutil.AssertEqual(t, 2, len(actions))

	// Verify each action
	testutil.AssertEqual(t, "pack1", actions[0].Pack)
	testutil.AssertEqual(t, "pack2", actions[1].Pack)
	testutil.AssertEqual(t, 100, actions[0].Priority)
	testutil.AssertEqual(t, 200, actions[1].Priority)
	testutil.AssertEqual(t, install1, actions[0].Command)
	testutil.AssertEqual(t, install2, actions[1].Command)
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

	actions, err := powerup.Process(matches)
	testutil.AssertError(t, err)
	testutil.AssertNil(t, actions)
	testutil.AssertContains(t, err.Error(), "failed to calculate checksum")
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

func TestGetInstallSentinelPath(t *testing.T) {
	pack := "mypack"
	path := GetInstallSentinelPath(pack)

	expected := filepath.Join(types.GetInstallDir(), pack)
	testutil.AssertEqual(t, expected, path)
}

// Benchmarks
func BenchmarkInstallScriptPowerUp_Process(b *testing.B) {
	tmpDir := b.TempDir()
	installPath := filepath.Join(tmpDir, "install.sh")
	err := os.WriteFile(installPath, []byte("#!/bin/bash\necho \"Installing...\""), 0755)
	if err != nil {
		b.Fatal(err)
	}

	powerup := NewInstallScriptPowerUp()
	matches := []types.TriggerMatch{
		{
			Path:         "install.sh",
			AbsolutePath: installPath,
			Pack:         "dev",
			Priority:     100,
		},
	}

	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		_, err := powerup.Process(matches)
		if err != nil {
			b.Fatal(err)
		}
	}
}
