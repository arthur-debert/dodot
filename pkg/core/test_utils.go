package core

import (
	"testing"

	"github.com/arthur-debert/dodot/pkg/paths"
	"github.com/arthur-debert/dodot/pkg/testutil"
)

// createTestPaths creates a Paths instance for testing
func createTestPaths(t testing.TB) *paths.Paths {
	t.Helper()
	var tempDir string
	if tt, ok := t.(*testing.T); ok {
		tempDir = testutil.TempDir(tt, "test-dotfiles")
	} else if bt, ok := t.(*testing.B); ok {
		tempDir = bt.TempDir()
	} else {
		t.Fatal("Unsupported test type")
	}

	paths, err := paths.New(tempDir)
	if err != nil {
		t.Fatalf("Failed to create test paths: %v", err)
	}
	return paths
}
