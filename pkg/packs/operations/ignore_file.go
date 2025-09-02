package operations

import (
	"github.com/arthur-debert/dodot/pkg/config"
	"github.com/arthur-debert/dodot/pkg/types"
)

// CreatePackIgnoreFile creates a .dodotignore file in the pack
func CreatePackIgnoreFile(pack *types.Pack, fs types.FS, cfg *config.Config) error {
	if cfg == nil {
		cfg = config.Default()
	}
	ignoreFile := cfg.Patterns.SpecialFiles.IgnoreFile
	return CreatePackFile(pack, fs, ignoreFile, "")
}

// PackHasIgnoreFile checks if the pack has an ignore file
func PackHasIgnoreFile(pack *types.Pack, fs types.FS, cfg *config.Config) (bool, error) {
	if cfg == nil {
		cfg = config.Default()
	}
	ignoreFile := cfg.Patterns.SpecialFiles.IgnoreFile
	return PackFileExists(pack, fs, ignoreFile)
}
