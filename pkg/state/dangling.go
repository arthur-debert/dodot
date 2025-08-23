package state

import (
	"github.com/arthur-debert/dodot/pkg/types"
)

// DanglingLink represents a dangling symlink in the deployment
type DanglingLink struct {
	// DeployedPath is the user-facing symlink (e.g., ~/.vimrc)
	DeployedPath string
	// IntermediatePath is the dodot state symlink
	IntermediatePath string
	// SourcePath is the expected source file that's missing
	SourcePath string
	// Pack is the pack that originally deployed this link
	Pack string
	// Problem describes what's wrong with the link
	Problem string
}

// LinkDetector handles detection of dangling and orphaned links
type LinkDetector struct {
	fs    types.FS
	paths types.Pather
}

// NewLinkDetector creates a new LinkDetector
func NewLinkDetector(fs types.FS, paths types.Pather) *LinkDetector {
	return &LinkDetector{
		fs:    fs,
		paths: paths,
	}
}

// DetectDanglingLinks scans for dangling symlinks in the deployment
// Returns only user-facing dangling links (not orphaned intermediate links)
// TODO: Update to work with ActionV2 system
func (ld *LinkDetector) DetectDanglingLinks(actions []types.ActionV2) ([]DanglingLink, error) {
	// Temporarily disabled while migrating to V2 action system
	return nil, nil
}
