// Package handlers implements various handler types that process
// matched files and generate actions. Handlers are responsible for
// determining what operations should be performed on matched files.
package handlers

import (
	// Import all handler implementations to ensure they register themselves
	_ "github.com/arthur-debert/dodot/pkg/handlers/homebrew"
	_ "github.com/arthur-debert/dodot/pkg/handlers/install"
	_ "github.com/arthur-debert/dodot/pkg/handlers/path"
	_ "github.com/arthur-debert/dodot/pkg/handlers/shell"
	_ "github.com/arthur-debert/dodot/pkg/handlers/symlink"
)
