// Package commands provides high-level command implementations for dodot.
//
// This package contains the command orchestration layer that coordinates
// between the CLI interface and the core pipeline functionality.
//
// Each command is implemented in its own subdirectory:
//   - list/     - ListPacks command
//   - link/     - LinkPacks command
//   - provision/  - ProvisionPacks command
//   - status/   - StatusPacks command
//   - fill/     - FillPack command
//   - initialize/ - InitPack command
//   - addignore/ - AddIgnore command
//   - adopt/    - AdoptFiles command
//   - internal/ - Shared execution pipeline logic
//
// This file serves as the main entry point and re-exports all command functions
// to maintain API compatibility.
package commands

import (
	"github.com/arthur-debert/dodot/pkg/commands/addignore"
	"github.com/arthur-debert/dodot/pkg/commands/adopt"
	"github.com/arthur-debert/dodot/pkg/commands/fill"
	"github.com/arthur-debert/dodot/pkg/commands/initialize"
	"github.com/arthur-debert/dodot/pkg/commands/link"
	"github.com/arthur-debert/dodot/pkg/commands/list"
	"github.com/arthur-debert/dodot/pkg/commands/provision"
	"github.com/arthur-debert/dodot/pkg/commands/status"
	"github.com/arthur-debert/dodot/pkg/commands/unlink"
	"github.com/arthur-debert/dodot/pkg/types"
)

// Re-export all command types and functions to maintain existing API

// ListPacks finds all available packs in the dotfiles root directory.
type ListPacksOptions = list.ListPacksOptions

func ListPacks(opts ListPacksOptions) (*types.ListPacksResult, error) {
	return list.ListPacks(opts)
}

// LinkPacks runs link logic using the direct executor approach.
type LinkPacksOptions = link.LinkPacksOptions

func LinkPacks(opts LinkPacksOptions) (*types.ExecutionContext, error) {
	return link.LinkPacks(opts)
}

// ProvisionPacks runs provisioning + linking using the direct executor approach.
type ProvisionPacksOptions = provision.ProvisionPacksOptions

func ProvisionPacks(opts ProvisionPacksOptions) (*types.ExecutionContext, error) {
	return provision.ProvisionPacks(opts)
}

// StatusPacks shows the link status of specified packs.
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

// AddIgnore creates a .dodotignore file in the specified pack.
type AddIgnoreOptions = addignore.AddIgnoreOptions

func AddIgnore(opts AddIgnoreOptions) (*types.AddIgnoreResult, error) {
	return addignore.AddIgnore(opts)
}

// AdoptFiles moves existing files into a pack and creates symlinks.
type AdoptFilesOptions = adopt.AdoptFilesOptions

func AdoptFiles(opts AdoptFilesOptions) (*types.AdoptResult, error) {
	return adopt.AdoptFiles(opts)
}

// UnlinkPacks removes links for specified packs.
type UnlinkPacksOptions = unlink.UnlinkPacksOptions
type UnlinkResult = unlink.UnlinkResult

func UnlinkPacks(opts UnlinkPacksOptions) (*UnlinkResult, error) {
	return unlink.UnlinkPacks(opts)
}
