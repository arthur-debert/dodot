package status

import (
	"strings"

	"github.com/arthur-debert/synthfs/pkg/synthfs"
	"github.com/arthur-debert/synthfs/pkg/synthfs/filesystem"
)

// newTestFileSystem creates a test filesystem that handles absolute paths
func newTestFileSystem() filesystem.FullFileSystem {
	testFS := filesystem.NewTestFileSystem()
	// Wrap with PathAwareFileSystem to handle absolute paths
	return synthfs.NewPathAwareFileSystem(testFS, "/").WithAbsolutePaths()
}

// trimLeadingSlash removes a leading slash from a path for test filesystem compatibility
func trimLeadingSlash(path string) string {
	return strings.TrimPrefix(path, "/")
}
