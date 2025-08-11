// Package shell_add_path provides an alternative PATH management powerup for dodot.
//
// # Overview
//
// The ShellAddPathPowerUp provides PATH management functionality similar to the
// path powerup. It adds directories from your dotfile packs to the system PATH
// by creating the necessary shell integration.
//
// Note: In most cases, you should use the `path` powerup instead, which has
// more complete functionality and better duplicate detection.
//
// # When It Runs
//
// - **Deploy Mode**: YES - Runs during `dodot deploy` (RunModeMany)
// - **Install Mode**: NO - Does not run during `dodot install`
// - **Idempotent**: YES - Implementation ensures no duplicate PATH entries
//
// # Relationship to Path PowerUp
//
// This powerup generates the same `ActionTypePathAdd` actions as the main
// `path` powerup. Both integrate with the same execution pipeline and produce
// identical results. The `path` powerup is recommended as it includes:
// - Better option validation
// - Duplicate detection within runs
// - More comprehensive error handling
//
// # Effects on User Environment
//
// Same as the `path` powerup:
// - Creates symlinks in ~/.local/share/dodot/deployed/path/
// - Appends PATH exports to ~/.local/share/dodot/shell/init.sh
// - Modifies system PATH on shell startup
//
// # Environment Variable Tracking
//
// PATH additions are tracked via the `DODOT_PATH_DIRS` environment variable,
// the same as the main `path` powerup.
//
// For detailed documentation, see the `path` powerup documentation.
package shell_add_path
