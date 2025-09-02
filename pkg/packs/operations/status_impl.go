package operations

import (
	"fmt"

	"github.com/arthur-debert/dodot/pkg/datastore"
	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/paths"
	"github.com/arthur-debert/dodot/pkg/rules"
	"github.com/arthur-debert/dodot/pkg/types"
)

// GetStatus retrieves the status of a pack
func GetStatus(opts StatusOptions) (*StatusResult, error) {
	logger := logging.GetLogger("operations.status")
	logger.Debug().
		Str("pack", opts.Pack.Name).
		Msg("Getting pack status")

	result := &StatusResult{
		Name:      opts.Pack.Name,
		Path:      opts.Pack.Path,
		HasConfig: false, // TODO: Check for .dodot.toml file
		IsIgnored: false, // TODO: Check for .dodotignore file
		Status:    "unknown",
		Files:     []FileStatus{},
	}

	// If pack is ignored, return early
	if result.IsIgnored {
		result.Status = "ignored"
		return result, nil
	}

	// Get matches for this pack
	matches, err := rules.NewMatcher().GetMatchesFS([]types.Pack{opts.Pack}, opts.FileSystem)
	if err != nil {
		return nil, fmt.Errorf("failed to get matches: %w", err)
	}

	// Check status for each match
	for _, match := range matches {
		fileStatus, err := getHandlerStatus(match, opts.Pack, opts.DataStore, opts.FileSystem, opts.Paths)
		if err != nil {
			logger.Error().
				Err(err).
				Str("file", match.Path).
				Str("handler", match.HandlerName).
				Msg("Failed to get handler status")
			continue
		}

		result.Files = append(result.Files, FileStatus{
			Handler:        match.HandlerName,
			Path:           match.Path,
			Status:         fileStatus,
			AdditionalInfo: "",
		})
	}

	// Determine overall pack status based on file statuses
	result.Status = determinePackStatus(result.Files)

	return result, nil
}

// getHandlerStatus checks the deployment status for a specific match
func getHandlerStatus(match rules.RuleMatch, pack types.Pack, dataStore datastore.DataStore, fs types.FS, pathsInstance paths.Paths) (Status, error) {
	// This is a simplified implementation
	// In a real implementation, this would check datastore state, symlinks, etc.
	return Status{
		State:   StatusStatePending,
		Message: "Not deployed",
	}, nil
}

// determinePackStatus calculates the overall pack status from file statuses
func determinePackStatus(files []FileStatus) string {
	if len(files) == 0 {
		return "empty"
	}

	hasError := false
	hasSuccess := false
	hasPending := false

	for _, file := range files {
		switch file.Status.State {
		case StatusStateError:
			hasError = true
		case StatusStateReady, StatusStateSuccess:
			hasSuccess = true
		case StatusStatePending, StatusStateMissing:
			hasPending = true
		}
	}

	if hasError {
		return "error"
	}
	if hasPending && !hasSuccess {
		return "pending"
	}
	if hasPending && hasSuccess {
		return "partial"
	}
	if hasSuccess {
		return "success"
	}

	return "unknown"
}

// statusStateToDisplayStatus converts internal status states to display status strings
func statusStateToDisplayStatus(state StatusState) string {
	switch state {
	case StatusStateReady, StatusStateSuccess:
		return "success"
	case StatusStateMissing:
		return "queue"
	case StatusStatePending:
		return "queue"
	case StatusStateError:
		return "error"
	case StatusStateIgnored:
		return "ignored"
	case StatusStateConfig:
		return "config"
	default:
		return "unknown"
	}
}
