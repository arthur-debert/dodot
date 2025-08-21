package install

import (
	"os"
	"path/filepath"
	"testing"

	"github.com/arthur-debert/dodot/pkg/paths"
	"github.com/arthur-debert/dodot/pkg/testutil"
	"github.com/arthur-debert/dodot/pkg/types"
)

// TestGetInstallSentinelPath tests filesystem path construction
// This is an integration test because it uses the paths package which constructs real filesystem paths
func TestGetInstallSentinelPath(t *testing.T) {
	// Create paths instance for testing
	tempDir := testutil.TempDir(t, "install-test")
	pathsInstance, err := paths.New(tempDir)
	testutil.AssertNoError(t, err)

	pack := "mypack"
	path := GetInstallSentinelPath(pack, pathsInstance)

	expected := filepath.Join(pathsInstance.InstallDir(), "sentinels", pack)
	testutil.AssertEqual(t, expected, path)
}

// Benchmarks involve file I/O and are integration tests
func BenchmarkInstallScriptHandler_Process(b *testing.B) {
	tmpDir := b.TempDir()
	installPath := filepath.Join(tmpDir, "install.sh")
	err := os.WriteFile(installPath, []byte("#!/bin/bash\necho \"Installing...\""), 0755)
	if err != nil {
		b.Fatal(err)
	}

	handler := NewInstallScriptHandler()
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
		_, err := handler.Process(matches)
		if err != nil {
			b.Fatal(err)
		}
	}
}
