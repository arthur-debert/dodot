package handlers

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

// Initialize sets up the handler system by:
// 1. Importing handler packages to register their factories
// 2. Initializing configuration
// 3. Setting up any other handler components
//
// This function should be called at application startup before
// using any handlers functionality.
func Initialize() error {
	logger := logging.GetLogger("handlers.init")

	// Initialize configuration - this will trigger loading of
	// configuration files and setting up the global config
	config.Initialize(nil)

	logger.Debug().Msg("Handler initialization completed")
	return nil
}

// MustInitialize initializes the handler system and panics on error
func MustInitialize() {
	if err := Initialize(); err != nil {
		panic(err)
	}
}