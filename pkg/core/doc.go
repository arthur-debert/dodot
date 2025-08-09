// Package core implements the core pipeline functions for dodot.
// It provides the main execution flow from pack discovery through
// to filesystem operations.
//
// # Symlink Deployment Strategy
//
// dodot uses a two-symlink strategy for deploying configuration files:
//
//  1. Intermediate symlink in dodot's data directory:
//     ~/.local/share/dodot/deployed/symlink/.vimrc -> ~/dotfiles/vim/vimrc
//
//  2. Target symlink in the user's home directory:
//     ~/.vimrc -> ~/.local/share/dodot/deployed/symlink/.vimrc
//
// This two-link approach provides several benefits:
//
//   - Atomic updates: The intermediate link can be updated without touching
//     files in the user's home directory
//   - Clean uninstall: All dodot-managed symlinks point to a known location
//   - Conflict detection: dodot can distinguish between its own symlinks and
//     user-created symlinks
//   - Pack organization: The intermediate directory maintains the structure
//     of deployed files
//
// # Conflict Resolution
//
// When deploying symlinks, dodot handles conflicts intelligently:
//
//  1. If the target is already a dodot-managed symlink (points to the
//     intermediate link), the operation is idempotent - no action needed
//
//  2. If the target is a regular file with identical content to the source,
//     dodot automatically replaces it with a symlink. This makes adoption
//     easier when users already have the same config files
//
//  3. If the target exists but differs (different symlink target or different
//     file content), dodot protects user data by requiring the --force flag
//
//  4. The intermediate symlink is always safe to update since it's in dodot's
//     private data directory
//
// This strategy ensures that repeated deploys are idempotent (no errors on
// re-run) while protecting user data from accidental overwrites.
package core
