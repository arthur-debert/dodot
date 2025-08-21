package state

import (
	"github.com/arthur-debert/dodot/pkg/types"
)

// DetectDanglingFromStatus creates DanglingLink objects from actions with error status
// This provides an alternative way to detect dangling links using the existing status checking
func DetectDanglingFromStatus(actions []types.Action, fs types.FS, paths types.Pather) ([]DanglingLink, error) {
	var dangling []DanglingLink

	for _, action := range actions {
		if action.Type != types.ActionTypeLink {
			continue
		}

		status, err := action.CheckStatus(fs, paths)
		if err != nil {
			continue
		}

		// Only interested in error states with details
		if status.State != types.StatusStateError || status.ErrorDetails == nil {
			continue
		}

		// Convert error details to dangling link
		dl := DanglingLink{
			DeployedPath:     status.ErrorDetails.DeployedPath,
			IntermediatePath: status.ErrorDetails.IntermediatePath,
			SourcePath:       status.ErrorDetails.SourcePath,
			Pack:             action.Pack,
		}

		// Map error type to problem description
		switch status.ErrorDetails.ErrorType {
		case "missing_source":
			dl.Problem = "source file missing"
		case "missing_intermediate":
			dl.Problem = "intermediate symlink missing"
		case "invalid_intermediate":
			dl.Problem = "intermediate is not a symlink"
		case "wrong_intermediate_target":
			dl.Problem = "intermediate points to wrong file"
		case "unreadable_intermediate":
			dl.Problem = "cannot read intermediate symlink"
		default:
			dl.Problem = "unknown error"
		}

		dangling = append(dangling, dl)
	}

	return dangling, nil
}
