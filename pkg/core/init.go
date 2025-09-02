package core

import (
	"github.com/arthur-debert/dodot/pkg/config"
	"github.com/arthur-debert/dodot/pkg/logging"

	// Import all handler packages to register their factories
	_ "github.com/arthur-debert/dodot/pkg/handlers/lib/homebrew"
	_ "github.com/arthur-debert/dodot/pkg/handlers/lib/install"
	_ "github.com/arthur-debert/dodot/pkg/handlers/lib/path"
	_ "github.com/arthur-debert/dodot/pkg/handlers/lib/shell"
	_ "github.com/arthur-debert/dodot/pkg/handlers/lib/symlink"
)

// Initialize sets up the core system by:
// 1. Importing handler packages to register their factories
// 2. Initializing configuration
// 3. Setting up any other core components
//
// This function should be called at application startup before
// using any handlers or matchers functionality.
func Initialize() error {
	logger := logging.GetLogger("core.init")

	// Initialize configuration - this will trigger loading of
	// configuration files and setting up the global config
	config.Initialize(nil)

	logger.Debug().Msg("Core initialization completed")
	return nil
}

// MustInitialize calls Initialize and panics on error.
// This is useful for main() functions where initialization failure
// should terminate the program.
func MustInitialize() {
	if err := Initialize(); err != nil {
		panic("Core initialization failed: " + err.Error())
	}
}
