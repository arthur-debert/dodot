// Package commands provides high-level command implementations for dodot.
//
// This package contains the command orchestration layer that coordinates
// between the CLI interface and the core pipeline functionality.
//
// Each command is implemented in its own subdirectory:
//   - status/   - StatusPacks command
//   - fill/     - FillPack command
//   - initialize/ - InitPack command
//   - addignore/ - AddIgnore command
//   - adopt/    - AdoptFiles command
//   - on/       - OnPacks command (primary deployment)
//   - off/      - OffPacks command (primary removal)
//
// This file serves as the main entry point and re-exports all command functions
// to maintain API compatibility.
package commands

import (
	"github.com/arthur-debert/dodot/pkg/commands/addignore"
	"github.com/arthur-debert/dodot/pkg/commands/adopt"
	"github.com/arthur-debert/dodot/pkg/commands/fill"
	"github.com/arthur-debert/dodot/pkg/commands/genconfig"
	"github.com/arthur-debert/dodot/pkg/commands/initialize"
	"github.com/arthur-debert/dodot/pkg/commands/off"
	"github.com/arthur-debert/dodot/pkg/commands/on"
	"github.com/arthur-debert/dodot/pkg/commands/status"
	"github.com/arthur-debert/dodot/pkg/types"
)

// Re-export all command types and functions to maintain existing API

// StatusPacks shows the link status of specified packs.
type StatusPacksOptions = status.StatusPacksOptions

func StatusPacks(opts StatusPacksOptions) (*types.PackCommandResult, error) {
	return status.StatusPacks(opts)
}

// FillPack adds missing template files to an existing pack.
type FillPackOptions = fill.FillPackOptions

func FillPack(opts FillPackOptions) (*types.PackCommandResult, error) {
	return fill.FillPack(opts)
}

// InitPack creates a new pack directory with template files and configuration.
type InitPackOptions = initialize.InitPackOptions

func InitPack(opts InitPackOptions) (*types.PackCommandResult, error) {
	return initialize.InitPack(opts)
}

// AddIgnore creates a .dodotignore file in the specified pack.
type AddIgnoreOptions = addignore.AddIgnoreOptions

func AddIgnore(opts AddIgnoreOptions) (*types.PackCommandResult, error) {
	return addignore.AddIgnore(opts)
}

// AdoptFiles moves existing files into a pack and creates symlinks.
type AdoptFilesOptions = adopt.AdoptFilesOptions

func AdoptFiles(opts AdoptFilesOptions) (*types.PackCommandResult, error) {
	return adopt.AdoptFiles(opts)
}

// GenConfig outputs or writes default configuration.
type GenConfigOptions = genconfig.GenConfigOptions

func GenConfig(opts GenConfigOptions) (*types.GenConfigResult, error) {
	return genconfig.GenConfig(opts)
}

// OnPacks turns on the specified packs by deploying all handlers.
type OnPacksOptions = on.OnPacksOptions

func OnPacks(opts OnPacksOptions) (*types.PackCommandResult, error) {
	return on.OnPacks(opts)
}

// OffPacks turns off the specified packs by removing all handler state.
type OffPacksOptions = off.OffPacksOptions

func OffPacks(opts OffPacksOptions) (*types.PackCommandResult, error) {
	return off.OffPacks(opts)
}
