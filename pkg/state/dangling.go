package state

import (
	"os"
	"path/filepath"

	"github.com/arthur-debert/dodot/pkg/errors"
	"github.com/arthur-debert/dodot/pkg/logging"
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
func (ld *LinkDetector) DetectDanglingLinks(actions []types.Action) ([]DanglingLink, error) {
	var dangling []DanglingLink

	for _, action := range actions {
		if action.Type != types.ActionTypeLink {
			continue
		}

		// Check the symlink chain
		dl, err := ld.checkSymlinkChain(&action)
		if err != nil {
			logger := logging.GetLogger("state.dangling")
			logger.Error().
				Err(err).
				Str("pack", action.Pack).
				Str("target", action.Target).
				Msg("error checking symlink chain")
			continue
		}

		if dl != nil {
			dangling = append(dangling, *dl)
		}
	}

	return dangling, nil
}

// checkSymlinkChain checks a single symlink action for dangling links
func (ld *LinkDetector) checkSymlinkChain(action *types.Action) (*DanglingLink, error) {
	logger := logging.GetLogger("state.dangling")

	// Get intermediate path
	intermediatePath, err := action.GetDeployedSymlinkPath(ld.paths)
	if err != nil {
		return nil, errors.Wrap(err, errors.ErrConfigParse, "failed to get intermediate symlink path")
	}

	logger.Debug().
		Str("target", action.Target).
		Str("intermediate", intermediatePath).
		Str("source", action.Source).
		Msg("checking symlink chain")

	// Check if deployed symlink exists
	targetInfo, err := ld.fs.Lstat(action.Target)
	if err != nil {
		// Target doesn't exist - not deployed, not dangling
		logger.Debug().Str("target", action.Target).Msg("target doesn't exist - not deployed")
		return nil, nil
	}

	// Check if it's a symlink
	if targetInfo.Mode()&os.ModeSymlink == 0 {
		// Not a symlink - not our concern
		return nil, nil
	}

	// Read where the deployed symlink points
	targetDest, err := ld.fs.Readlink(action.Target)
	if err != nil {
		// Can't read symlink - treat as not ours
		logger.Debug().Err(err).Msg("can't read deployed symlink")
		return nil, nil
	}

	// Resolve the target destination
	resolvedTarget := targetDest
	if !filepath.IsAbs(targetDest) {
		resolvedTarget = filepath.Join(filepath.Dir(action.Target), targetDest)
	}

	logger.Debug().
		Str("targetDest", targetDest).
		Str("resolvedTarget", resolvedTarget).
		Str("expectedIntermediate", intermediatePath).
		Msg("checking if deployed points to intermediate")

	// Check if it points to our intermediate symlink
	// We need to handle both absolute and relative paths
	if !pathsMatch(targetDest, intermediatePath, resolvedTarget) {
		// Points somewhere else - not managed by us
		logger.Debug().Msg("deployed symlink points elsewhere - not managed by us")
		return nil, nil
	}

	// Now check the intermediate symlink
	intermediateInfo, err := ld.fs.Lstat(intermediatePath)
	if err != nil {
		// Intermediate missing - this is dangling
		return &DanglingLink{
			DeployedPath:     action.Target,
			IntermediatePath: intermediatePath,
			SourcePath:       action.Source,
			Pack:             action.Pack,
			Problem:          "intermediate symlink missing",
		}, nil
	}

	// Check if intermediate is a symlink
	if intermediateInfo.Mode()&os.ModeSymlink == 0 {
		// Intermediate is not a symlink - corrupted state
		return &DanglingLink{
			DeployedPath:     action.Target,
			IntermediatePath: intermediatePath,
			SourcePath:       action.Source,
			Pack:             action.Pack,
			Problem:          "intermediate is not a symlink",
		}, nil
	}

	// Read where intermediate points
	intermediateDest, err := ld.fs.Readlink(intermediatePath)
	if err != nil {
		return &DanglingLink{
			DeployedPath:     action.Target,
			IntermediatePath: intermediatePath,
			SourcePath:       action.Source,
			Pack:             action.Pack,
			Problem:          "cannot read intermediate symlink",
		}, nil
	}

	// Resolve intermediate destination
	resolvedIntermediate := intermediateDest
	if !filepath.IsAbs(intermediateDest) {
		resolvedIntermediate = filepath.Join(filepath.Dir(intermediatePath), intermediateDest)
	}

	// Check if intermediate points to the correct source
	if !pathsMatch(intermediateDest, action.Source, resolvedIntermediate) {
		// Points to wrong file
		logger.Debug().
			Str("intermediateDest", intermediateDest).
			Str("expectedSource", action.Source).
			Str("resolvedIntermediate", resolvedIntermediate).
			Msg("intermediate points to wrong file")
		return &DanglingLink{
			DeployedPath:     action.Target,
			IntermediatePath: intermediatePath,
			SourcePath:       action.Source,
			Pack:             action.Pack,
			Problem:          "intermediate points to wrong file",
		}, nil
	}

	// Finally check if source exists
	if _, err := ld.fs.Stat(action.Source); err != nil {
		// Source missing - this is dangling
		logger.Debug().
			Str("source", action.Source).
			Err(err).
			Msg("source file missing - link is dangling")
		return &DanglingLink{
			DeployedPath:     action.Target,
			IntermediatePath: intermediatePath,
			SourcePath:       action.Source,
			Pack:             action.Pack,
			Problem:          "source file missing",
		}, nil
	}

	// Everything is fine
	logger.Debug().Msg("symlink chain is healthy")
	return nil, nil
}

// pathsMatch checks if symlink paths match, handling both relative and absolute paths
func pathsMatch(targetDest, expectedPath, resolvedPath string) bool {
	// Direct match
	if targetDest == expectedPath {
		return true
	}

	// Clean and compare
	if filepath.Clean(targetDest) == filepath.Clean(expectedPath) {
		return true
	}

	// Compare resolved path
	if filepath.Clean(resolvedPath) == filepath.Clean(expectedPath) {
		return true
	}

	// In test environments with relative paths, the target might be exactly what we expect
	// without resolution (e.g., "data/dodot/deployed/symlink/.vimrc")
	if targetDest == expectedPath {
		return true
	}

	return false
}

// RemoveDanglingLink safely removes a dangling symlink
// It verifies ownership before removal to avoid affecting user-created symlinks
func (ld *LinkDetector) RemoveDanglingLink(dl *DanglingLink) error {
	logger := logging.GetLogger("state.dangling")

	// First verify the deployed symlink still points to our intermediate
	targetDest, err := ld.fs.Readlink(dl.DeployedPath)
	if err != nil {
		// Can't read - maybe already removed
		logger.Debug().
			Str("path", dl.DeployedPath).
			Err(err).
			Msg("deployed symlink no longer exists or unreadable")
		return nil
	}

	// Resolve the target
	resolvedTarget := targetDest
	if !filepath.IsAbs(targetDest) {
		resolvedTarget = filepath.Join(filepath.Dir(dl.DeployedPath), targetDest)
	}

	// Only remove if it still points to our intermediate
	if !pathsMatch(targetDest, dl.IntermediatePath, resolvedTarget) {
		logger.Debug().
			Str("deployed", dl.DeployedPath).
			Str("expected_intermediate", dl.IntermediatePath).
			Str("actual_target", resolvedTarget).
			Msg("not removing symlink - no longer points to our intermediate")
		return nil
	}

	// Remove the deployed symlink first
	logger.Debug().
		Str("path", dl.DeployedPath).
		Msg("attempting to remove deployed symlink")

	if err := ld.fs.Remove(dl.DeployedPath); err != nil && !os.IsNotExist(err) {
		logger.Error().
			Err(err).
			Str("path", dl.DeployedPath).
			Msg("error removing deployed symlink")
		return errors.Wrapf(err, errors.ErrFileAccess, "failed to remove deployed symlink %s", dl.DeployedPath)
	}

	logger.Info().
		Str("path", dl.DeployedPath).
		Str("pack", dl.Pack).
		Str("problem", dl.Problem).
		Msg("removed dangling symlink")

	// Try to remove the intermediate symlink if it exists
	if _, err := ld.fs.Lstat(dl.IntermediatePath); err == nil {
		if err := ld.fs.Remove(dl.IntermediatePath); err != nil && !os.IsNotExist(err) {
			// Log but don't fail - the important part is removing the user-facing link
			logger.Debug().
				Err(err).
				Str("path", dl.IntermediatePath).
				Msg("failed to remove intermediate symlink")
		} else {
			logger.Debug().
				Str("path", dl.IntermediatePath).
				Msg("removed intermediate symlink")
		}
	}

	return nil
}
