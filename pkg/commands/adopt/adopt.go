package adopt

import (
	"fmt"
	"time"

	"github.com/arthur-debert/dodot/pkg/commands/status"
	"github.com/arthur-debert/dodot/pkg/filesystem"
	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/pack"
	"github.com/arthur-debert/dodot/pkg/types"
)

// AdoptFilesOptions holds options for the adopt command
type AdoptFilesOptions struct {
	DotfilesRoot string
	PackName     string
	SourcePaths  []string
	Force        bool
	FileSystem   types.FS // Allow injecting a filesystem for testing
}

// AdoptFiles moves existing files into a pack and creates symlinks back to their original locations
func AdoptFiles(opts AdoptFilesOptions) (*types.PackCommandResult, error) {
	logger := logging.GetLogger("commands.adopt")
	logger.Info().
		Str("pack", opts.PackName).
		Str("dotfiles_root", opts.DotfilesRoot).
		Strs("source_paths", opts.SourcePaths).
		Bool("force", opts.Force).
		Msg("Adopting files into pack")

	// Use provided filesystem or default to OS
	fs := opts.FileSystem
	if fs == nil {
		fs = filesystem.NewOS()
	}

	// Use pack.AdoptOrCreate which handles pack creation if needed
	adoptResult, err := pack.AdoptOrCreate(fs, opts.DotfilesRoot, opts.PackName, pack.AdoptOptions{
		SourcePaths:  opts.SourcePaths,
		Force:        opts.Force,
		DotfilesRoot: opts.DotfilesRoot,
	})
	if err != nil {
		return nil, err
	}

	// Get current pack status
	statusOpts := status.StatusPacksOptions{
		DotfilesRoot: opts.DotfilesRoot,
		PackNames:    []string{opts.PackName},
		FileSystem:   fs,
	}
	packStatus, statusErr := status.StatusPacks(statusOpts)
	if statusErr != nil {
		logger.Error().Err(statusErr).Msg("Failed to get pack status")
	}

	// Build result
	result := &types.PackCommandResult{
		Command:   "adopt",
		Timestamp: time.Now(),
		DryRun:    false,
		Packs:     []types.DisplayPack{},
		Metadata: types.CommandMetadata{
			FilesAdopted: len(adoptResult.AdoptedFiles),
			AdoptedPaths: make([]string, 0, len(adoptResult.AdoptedFiles)),
		},
	}

	// Collect adopted paths
	for _, file := range adoptResult.AdoptedFiles {
		result.Metadata.AdoptedPaths = append(result.Metadata.AdoptedPaths, file.OriginalPath)
	}

	// Copy packs from status if available
	if packStatus != nil {
		result.Packs = packStatus.Packs
	}

	// Generate message
	if len(adoptResult.AdoptedFiles) == 1 {
		result.Message = "1 file has been adopted into the pack " + opts.PackName + "."
	} else {
		result.Message = types.FormatCommandMessage("files have been adopted into the pack "+opts.PackName, []string{})
		// Override with custom message for adopt
		result.Message = fmt.Sprintf("%d files have been adopted into the pack %s.", len(adoptResult.AdoptedFiles), opts.PackName)
	}

	return result, nil
}
