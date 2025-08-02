// Package powerups implements various power-up types that process
// matched files and generate actions. Power-ups are responsible for
// determining what operations should be performed on matched files.
package powerups

import (
	// Import all powerup implementations to ensure they register themselves
	_ "github.com/arthur-debert/dodot/pkg/powerups/bin"
	_ "github.com/arthur-debert/dodot/pkg/powerups/brewfile"
	_ "github.com/arthur-debert/dodot/pkg/powerups/install"
	_ "github.com/arthur-debert/dodot/pkg/powerups/shell_add_path"
	_ "github.com/arthur-debert/dodot/pkg/powerups/shell_profile"
	_ "github.com/arthur-debert/dodot/pkg/powerups/symlink"
	_ "github.com/arthur-debert/dodot/pkg/powerups/template"
)
