package state

import (
	"github.com/arthur-debert/dodot/pkg/types"
)

// DetectDanglingFromStatus creates DanglingLink objects from actions with error status
// This provides an alternative way to detect dangling links using the existing status checking
// TODO: Update to work with ActionV2 system
func DetectDanglingFromStatus(actions []types.ActionV2, fs types.FS, paths types.Pather) ([]DanglingLink, error) {
	// Temporarily disabled while migrating to V2 action system
	return nil, nil
}