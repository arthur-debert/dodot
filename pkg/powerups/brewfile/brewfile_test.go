package brewfile

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/arthur-debert/dodot/pkg/paths"
	"github.com/arthur-debert/dodot/pkg/testutil"
	"github.com/arthur-debert/dodot/pkg/types"
)

func TestBrewfilePowerUp_Basic(t *testing.T) {
	powerup := NewBrewfilePowerUp()

	testutil.AssertEqual(t, BrewfilePowerUpName, powerup.Name())
	testutil.AssertEqual(t, "Processes Brewfiles to install Homebrew packages", powerup.Description())
	testutil.AssertEqual(t, types.RunModeOnce, powerup.RunMode())
}

func TestBrewfilePowerUp_Process(t *testing.T) {
	// Create test files
	tmpDir := testutil.TempDir(t, "brewfile-test")

	// Create a test Brewfile
	brewfilePath := filepath.Join(tmpDir, "Brewfile")
	brewfileContent := `brew "git"
brew "node"
cask "visual-studio-code"`
	err := os.WriteFile(brewfilePath, []byte(brewfileContent), 0644)
	testutil.AssertNoError(t, err)

	powerup := NewBrewfilePowerUp()

	matches := []types.TriggerMatch{
		{
			Path:         "Brewfile",
			AbsolutePath: brewfilePath,
			Pack:         "tools",
			Priority:     100,
		},
	}

	actions, err := powerup.Process(matches)
	testutil.AssertNoError(t, err)
	testutil.AssertEqual(t, 2, len(actions)) // Now we have checksum + brew actions

	// First action should be checksum
	checksumAction := actions[0]
	testutil.AssertEqual(t, types.ActionTypeChecksum, checksumAction.Type)
	testutil.AssertEqual(t, brewfilePath, checksumAction.Source)
	testutil.AssertEqual(t, "tools", checksumAction.Pack)
	testutil.AssertEqual(t, BrewfilePowerUpName, checksumAction.PowerUpName)
	testutil.AssertEqual(t, 101, checksumAction.Priority) // Higher priority
	testutil.AssertContains(t, checksumAction.Description, "Calculate checksum")

	// Second action should be brew
	brewAction := actions[1]
	testutil.AssertEqual(t, types.ActionTypeBrew, brewAction.Type)
	testutil.AssertEqual(t, brewfilePath, brewAction.Source)
	testutil.AssertEqual(t, "tools", brewAction.Pack)
	testutil.AssertEqual(t, BrewfilePowerUpName, brewAction.PowerUpName)
	testutil.AssertEqual(t, 100, brewAction.Priority)
	testutil.AssertContains(t, brewAction.Description, "Install packages from")

	// Check metadata
	testutil.AssertNotNil(t, brewAction.Metadata)
	testutil.AssertEqual(t, "tools", brewAction.Metadata["pack"])
}

func TestBrewfilePowerUp_Process_MultipleMatches(t *testing.T) {
	tmpDir := testutil.TempDir(t, "brewfile-test")

	// Create multiple Brewfiles
	brewfile1 := filepath.Join(tmpDir, "Brewfile1")
	brewfile2 := filepath.Join(tmpDir, "Brewfile2")

	err := os.WriteFile(brewfile1, []byte("brew \"git\""), 0644)
	testutil.AssertNoError(t, err)
	err = os.WriteFile(brewfile2, []byte("brew \"node\""), 0644)
	testutil.AssertNoError(t, err)

	powerup := NewBrewfilePowerUp()

	matches := []types.TriggerMatch{
		{
			Path:         "Brewfile1",
			AbsolutePath: brewfile1,
			Pack:         "pack1",
			Priority:     100,
		},
		{
			Path:         "Brewfile2",
			AbsolutePath: brewfile2,
			Pack:         "pack2",
			Priority:     200,
		},
	}

	actions, err := powerup.Process(matches)
	testutil.AssertNoError(t, err)
	testutil.AssertEqual(t, 4, len(actions)) // 2 checksum + 2 brew actions

	// Verify actions are in correct order
	// First checksum for pack1
	testutil.AssertEqual(t, types.ActionTypeChecksum, actions[0].Type)
	testutil.AssertEqual(t, "pack1", actions[0].Pack)
	testutil.AssertEqual(t, 101, actions[0].Priority)

	// Then brew for pack1
	testutil.AssertEqual(t, types.ActionTypeBrew, actions[1].Type)
	testutil.AssertEqual(t, "pack1", actions[1].Pack)
	testutil.AssertEqual(t, 100, actions[1].Priority)

	// Then checksum for pack2
	testutil.AssertEqual(t, types.ActionTypeChecksum, actions[2].Type)
	testutil.AssertEqual(t, "pack2", actions[2].Pack)
	testutil.AssertEqual(t, 201, actions[2].Priority)

	// Finally brew for pack2
	testutil.AssertEqual(t, types.ActionTypeBrew, actions[3].Type)
	testutil.AssertEqual(t, "pack2", actions[3].Pack)
	testutil.AssertEqual(t, 200, actions[3].Priority)
}

func TestBrewfilePowerUp_Process_ChecksumError(t *testing.T) {
	powerup := NewBrewfilePowerUp()

	matches := []types.TriggerMatch{
		{
			Path:         "Brewfile",
			AbsolutePath: "/non/existent/file",
			Pack:         "tools",
			Priority:     100,
		},
	}

	// Since we're no longer calculating checksums directly in the PowerUp,
	// it should not fail but instead create actions
	actions, err := powerup.Process(matches)
	testutil.AssertNoError(t, err)
	testutil.AssertEqual(t, 2, len(actions))

	// First action should be checksum (which will fail when executed)
	testutil.AssertEqual(t, types.ActionTypeChecksum, actions[0].Type)
	testutil.AssertEqual(t, "/non/existent/file", actions[0].Source)

	// Second action should be brew
	testutil.AssertEqual(t, types.ActionTypeBrew, actions[1].Type)
}

func TestBrewfilePowerUp_ValidateOptions(t *testing.T) {
	powerup := NewBrewfilePowerUp()

	// Brewfile power-up doesn't have options, so any options should be accepted
	err := powerup.ValidateOptions(nil)
	testutil.AssertNoError(t, err)

	err = powerup.ValidateOptions(map[string]interface{}{})
	testutil.AssertNoError(t, err)

	err = powerup.ValidateOptions(map[string]interface{}{
		"some": "option",
	})
	testutil.AssertNoError(t, err)
}

func TestCalculateFileChecksum(t *testing.T) {
	tmpDir := testutil.TempDir(t, "brewfile-test")

	// Test with a known content
	testFile := filepath.Join(tmpDir, "test.txt")
	content := "Hello, World!"
	err := os.WriteFile(testFile, []byte(content), 0644)
	testutil.AssertNoError(t, err)

	checksum, err := testutil.CalculateFileChecksum(testFile)
	testutil.AssertNoError(t, err)
	// SHA256 of "Hello, World!" is:
	testutil.AssertEqual(t, "dffd6021bb2bd5b0af676290809ec3a53191dd81c7f70a4b28688a362182986f", checksum)

	// Test with non-existent file
	_, err = testutil.CalculateFileChecksum("/non/existent/file")
	testutil.AssertError(t, err)
}

func TestGetBrewfileSentinelPath(t *testing.T) {
	pack := "mypack"
	path := GetBrewfileSentinelPath(pack)

	expected := filepath.Join(paths.GetBrewfileDir(), pack)
	testutil.AssertEqual(t, expected, path)
}

// Benchmarks
func BenchmarkBrewfilePowerUp_Process(b *testing.B) {
	tmpDir := b.TempDir()
	brewfilePath := filepath.Join(tmpDir, "Brewfile")
	err := os.WriteFile(brewfilePath, []byte("brew \"git\"\nbrew \"node\""), 0644)
	if err != nil {
		b.Fatal(err)
	}

	powerup := NewBrewfilePowerUp()
	matches := []types.TriggerMatch{
		{
			Path:         "Brewfile",
			AbsolutePath: brewfilePath,
			Pack:         "tools",
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

func BenchmarkCalculateFileChecksum(b *testing.B) {
	tmpDir := b.TempDir()
	testFile := filepath.Join(tmpDir, "test.txt")

	// Create a 1MB file for benchmarking
	data := make([]byte, 1024*1024)
	for i := range data {
		data[i] = byte(i % 256)
	}
	err := os.WriteFile(testFile, data, 0644)
	if err != nil {
		b.Fatal(err)
	}

	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		_, err := testutil.CalculateFileChecksum(testFile)
		if err != nil {
			b.Fatal(err)
		}
	}
}
