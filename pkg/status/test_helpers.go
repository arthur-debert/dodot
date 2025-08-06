package status

import (
	"github.com/arthur-debert/synthfs/pkg/synthfs"
	"github.com/arthur-debert/synthfs/pkg/synthfs/filesystem"
)

// newTestFileSystem creates a test filesystem that handles absolute paths
func newTestFileSystem() filesystem.FullFileSystem {
	testFS := filesystem.NewTestFileSystem()
	// Wrap with PathAwareFileSystem to handle absolute paths
	return synthfs.NewPathAwareFileSystem(testFS, "/").WithAbsolutePaths()
}
