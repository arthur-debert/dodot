package commands

import (
	"fmt"
	"path/filepath"
	"strings"

	"github.com/arthur-debert/dodot/pkg/errors"
	"github.com/arthur-debert/dodot/pkg/filesystem"
	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/packs/execution"
	"github.com/arthur-debert/dodot/pkg/types"
)

// InitCommand implements the "init" command using the pack execution.
// It creates a new pack and then uses FillCommand to populate it.
type InitCommand struct {
	// PackName is the name of the pack to create
	PackName string
}

// Name returns the command name.
func (c *InitCommand) Name() string {
	return "init"
}

// ExecuteForPack creates a new pack and fills it with template files.
// Note: This is special because it creates the pack first, then operates on it.
func (c *InitCommand) ExecuteForPack(pack types.Pack, opts execution.Options) (*execution.PackResult, error) {
	// This should not be called with discovered packs - init creates its own
	if pack.Name != c.PackName {
		return &execution.PackResult{
			Pack:    pack,
			Success: false,
			Error:   fmt.Errorf("init command called with wrong pack"),
		}, fmt.Errorf("init command called with wrong pack")
	}

	// The pack directory should already exist (created by InitPreprocess)
	// Now we just need to fill it using FillCommand
	fillCmd := &FillCommand{}
	return fillCmd.ExecuteForPack(pack, opts)
}

// InitPreprocess creates the pack directory before the pipeline runs.
// This should be called before executing the pack execution.
func InitPreprocess(packName string, dotfilesRoot string, fs types.FS) error {
	logger := logging.GetLogger("execution.init.preprocess")
	logger.Debug().
		Str("pack", packName).
		Str("dotfilesRoot", dotfilesRoot).
		Msg("Preprocessing init command")

	// Validate pack name
	if packName == "" {
		return errors.New(errors.ErrInvalidInput, "pack name cannot be empty")
	}

	// Check for invalid characters in pack name
	if strings.ContainsAny(packName, "/\\:*?\"<>|") {
		return errors.Newf(errors.ErrInvalidInput, "pack name contains invalid characters: %s", packName)
	}

	// Use provided filesystem or default
	if fs == nil {
		fs = filesystem.NewOS()
	}

	// Create pack directory
	packPath := filepath.Join(dotfilesRoot, packName)

	// Check if pack already exists
	if info, err := fs.Stat(packPath); err == nil {
		if info.IsDir() {
			return errors.Newf(errors.ErrPackExists, "pack '%s' already exists", packName)
		}
		return errors.Newf(errors.ErrInvalidInput, "a file named '%s' already exists", packName)
	}

	// Create the pack directory
	if err := fs.MkdirAll(packPath, 0755); err != nil {
		return errors.Wrapf(err, errors.ErrDirCreate, "failed to create pack directory")
	}

	logger.Info().
		Str("pack", packName).
		Str("path", packPath).
		Msg("Created pack directory")

	return nil
}
