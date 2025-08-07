// Package commands provides high-level command implementations for dodot.
//
// This package contains the command orchestration layer that coordinates
// between the CLI interface and the core pipeline functionality.
//
// Each command is implemented in its own subdirectory:
//   - list/     - ListPacks command
//   - deploy/   - DeployPacks command
//   - install/  - InstallPacks command
//   - status/   - StatusPacks command
//   - fill/     - FillPack command
//   - initialize/ - InitPack command
//   - internal/ - Shared execution pipeline logic
//
// This file serves as the main entry point and re-exports all command functions
// to maintain API compatibility.
package commands

import (
	"github.com/arthur-debert/dodot/pkg/commands/deploy"
	"github.com/arthur-debert/dodot/pkg/commands/fill"
	"github.com/arthur-debert/dodot/pkg/commands/initialize"
	"github.com/arthur-debert/dodot/pkg/commands/install"
	"github.com/arthur-debert/dodot/pkg/commands/list"
	"github.com/arthur-debert/dodot/pkg/commands/status"
	"github.com/arthur-debert/dodot/pkg/types"
)

// Re-export all command types and functions to maintain existing API

// ListPacks finds all available packs in the dotfiles root directory.
type ListPacksOptions = list.ListPacksOptions

func ListPacks(opts ListPacksOptions) (*types.ListPacksResult, error) {
	return list.ListPacks(opts)
}

// DeployPacks runs deployment logic for specified packs (RunModeMany power-ups).
type DeployPacksOptions = deploy.DeployPacksOptions

func DeployPacks(opts DeployPacksOptions) (*types.ExecutionResult, error) {
	return deploy.DeployPacks(opts)
}

// DeployPacksDirect runs deployment logic using the direct executor approach.
func DeployPacksDirect(opts DeployPacksOptions) (*types.ExecutionContext, error) {
	return deploy.DeployPacksDirect(opts)
}

// InstallPacks runs installation + deployment (RunModeOnce then RunModeMany power-ups).
type InstallPacksOptions = install.InstallPacksOptions

func InstallPacks(opts InstallPacksOptions) (*types.ExecutionResult, error) {
	return install.InstallPacks(opts)
}

// InstallPacksDirect runs installation + deployment using the direct executor approach.
func InstallPacksDirect(opts InstallPacksOptions) (*types.ExecutionContext, error) {
	return install.InstallPacksDirect(opts)
}

// StatusPacks checks the deployment status of specified packs.
type StatusPacksOptions = status.StatusPacksOptions

func StatusPacks(opts StatusPacksOptions) (*types.DisplayResult, error) {
	return status.StatusPacks(opts)
}

// FillPack adds missing template files to an existing pack.
type FillPackOptions = fill.FillPackOptions

func FillPack(opts FillPackOptions) (*types.FillResult, error) {
	return fill.FillPack(opts)
}

// InitPack creates a new pack directory with template files and configuration.
type InitPackOptions = initialize.InitPackOptions

func InitPack(opts InitPackOptions) (*types.InitResult, error) {
	return initialize.InitPack(opts)
}
