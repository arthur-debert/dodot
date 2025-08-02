// Package packs provides functionality for discovering, loading, and managing
// dotfile packs within the dodot system.
//
// A pack is a directory containing dotfiles and optional configuration that
// defines how those files should be deployed. This package handles:
//
//   - Pack discovery and validation
//   - Pack configuration loading (.dodot.toml files)
//   - Pack ignore functionality (.dodotignore files)
//   - Pack filtering and selection
//
// The package implements the core pack management logic that feeds into
// the dodot deployment pipeline.
package packs
