package addignore

import (
	"github.com/arthur-debert/dodot/pkg/config"
	"github.com/arthur-debert/dodot/pkg/core"
	"github.com/arthur-debert/dodot/pkg/filesystem"
	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/pack"
	"github.com/arthur-debert/dodot/pkg/types"
)

// AddIgnoreOptions holds options for the add-ignore command
type AddIgnoreOptions struct {
	DotfilesRoot string
	PackName     string
}

// AddIgnore creates a .dodotignore file using proper abstractions
func AddIgnore(opts AddIgnoreOptions) (*types.AddIgnoreResult, error) {
	logger := logging.GetLogger("commands.addignore")
	logger.Info().
		Str("pack", opts.PackName).
		Str("dotfiles_root", opts.DotfilesRoot).
		Msg("Creating ignore file for pack")

	// Get configuration
	cfg := config.Default()

	// Initialize filesystem
	fs := filesystem.NewOS()

	// Find the pack using core abstraction
	targetPack, err := core.FindPack(opts.DotfilesRoot, opts.PackName)
	if err != nil {
		return nil, err
	}

	// Wrap in our enhanced Pack type and delegate to AddIgnore method
	p := pack.New(targetPack)
	return p.AddIgnore(fs, cfg)
}
