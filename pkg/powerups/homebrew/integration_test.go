package homebrew

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/arthur-debert/dodot/pkg/paths"
	"github.com/arthur-debert/dodot/pkg/testutil"
	"github.com/arthur-debert/dodot/pkg/types"
)

// TestCalculateFileChecksum tests actual file I/O operations
// This is an integration test because it performs real file operations
func TestCalculateFileChecksum(t *testing.T) {
	tmpDir := testutil.TempDir(t, "homebrew-test")

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

// TestGetHomebrewSentinelPath tests filesystem path construction
// This is an integration test because it uses the paths package which constructs real filesystem paths
func TestGetHomebrewSentinelPath(t *testing.T) {
	// Create paths instance for testing
	tempDir := testutil.TempDir(t, "homebrew-test")
	pathsInstance, err := paths.New(tempDir)
	testutil.AssertNoError(t, err)

	pack := "mypack"
	path := GetHomebrewSentinelPath(pack, pathsInstance)

	expected := filepath.Join(pathsInstance.HomebrewDir(), pack)
	testutil.AssertEqual(t, expected, path)
}

// Benchmarks involve file I/O and are integration tests
func BenchmarkBrewfilePowerUp_Process(b *testing.B) {
	tmpDir := b.TempDir()
	brewfilePath := filepath.Join(tmpDir, "Brewfile")
	err := os.WriteFile(brewfilePath, []byte("brew \"git\"\nbrew \"node\""), 0644)
	if err != nil {
		b.Fatal(err)
	}

	powerup := NewHomebrewPowerUp()
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
