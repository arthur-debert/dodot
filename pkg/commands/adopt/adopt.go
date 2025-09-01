package adopt

import (
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
func AdoptFiles(opts AdoptFilesOptions) (*types.AdoptResult, error) {
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
	return pack.AdoptOrCreate(fs, opts.DotfilesRoot, opts.PackName, pack.AdoptOptions{
		SourcePaths:  opts.SourcePaths,
		Force:        opts.Force,
		DotfilesRoot: opts.DotfilesRoot,
	})
}
