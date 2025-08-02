// Package commands provides high-level command implementations for dodot.
//
// This package contains the command orchestration layer that coordinates
// between the CLI interface and the core pipeline functionality.
//
// Each command is implemented in its own file:
//   - list.go     - ListPacks command
//   - deploy.go   - DeployPacks command
//   - install.go  - InstallPacks command
//   - status.go   - StatusPacks command
//   - fill.go     - FillPack command
//   - init.go     - InitPack command
//   - execution.go - Shared execution pipeline logic
//
// This file serves as the main entry point and re-exports all command functions
// to maintain API compatibility.
package commands

// Re-export all command functions to maintain existing API
// These functions are implemented in their respective files:

// From list.go
// ListPacks finds all available packs in the dotfiles root directory.
// func ListPacks(opts ListPacksOptions) (*types.ListPacksResult, error)

// From deploy.go
// DeployPacks runs deployment logic for specified packs (RunModeMany power-ups).
// func DeployPacks(opts DeployPacksOptions) (*types.ExecutionResult, error)

// From install.go
// InstallPacks runs installation + deployment (RunModeOnce then RunModeMany power-ups).
// func InstallPacks(opts InstallPacksOptions) (*types.ExecutionResult, error)

// From status.go
// StatusPacks checks the deployment status of specified packs.
// func StatusPacks(opts StatusPacksOptions) (*types.PackStatusResult, error)

// From fill.go
// FillPack adds missing template files to an existing pack.
// func FillPack(opts FillPackOptions) (*types.FillResult, error)

// From init.go
// InitPack creates a new pack directory with template files and configuration.
// func InitPack(opts InitPackOptions) (*types.InitResult, error)

// Note: All actual implementations are in the respective command files.
// This approach provides:
// - Better code organization (each command in its own file)
// - Easier maintenance and testing
// - Clear separation of concerns
// - Maintained API compatibility
