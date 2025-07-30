package core

import (
	"fmt"
	"os"
	"path/filepath"
	"strings"

	"github.com/arthur-debert/dodot/pkg/errors"
	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/registry"
	"github.com/arthur-debert/dodot/pkg/types"
)

// ListPacksOptions defines the options for the ListPacks command.
type ListPacksOptions struct {
	// DotfilesRoot is the path to the root of the dotfiles directory.
	DotfilesRoot string
}

// DeployPacksOptions defines the options for the DeployPacks command.
type DeployPacksOptions struct {
	// DotfilesRoot is the path to the root of the dotfiles directory.
	DotfilesRoot string
	// PackNames is a list of specific packs to deploy. If empty, all packs are deployed.
	PackNames []string
	// DryRun specifies whether to perform a dry run without making changes.
	DryRun bool
}

// InstallPacksOptions defines the options for the InstallPacks command.
type InstallPacksOptions struct {
	// DotfilesRoot is the path to the root of the dotfiles directory.
	DotfilesRoot string
	// PackNames is a list of specific packs to install. If empty, all packs are installed.
	PackNames []string
	// DryRun specifies whether to perform a dry run without making changes.
	DryRun bool
	// Force re-runs power-ups that normally only run once.
	Force bool
}

// StatusPacksOptions defines the options for the StatusPacks command.
type StatusPacksOptions struct {
	// DotfilesRoot is the path to the root of the dotfiles directory.
	DotfilesRoot string
	// PackNames is a list of specific packs to check status for. If empty, all packs are checked.
	PackNames []string
}

// FillPackOptions defines the options for the FillPack command.
type FillPackOptions struct {
	// DotfilesRoot is the path to the root of the dotfiles directory.
	DotfilesRoot string
	// PackName is the name of the pack to fill with template files.
	PackName string
}

// InitPackOptions defines the options for the InitPack command.
type InitPackOptions struct {
	// DotfilesRoot is the path to the root of the dotfiles directory.
	DotfilesRoot string
	// PackName is the name of the new pack to create.
	PackName string
}

// ListPacks finds all available packs in the dotfiles root.
func ListPacks(opts ListPacksOptions) (*types.ListPacksResult, error) {
	log := logging.GetLogger("core.commands")
	log.Debug().Str("command", "ListPacks").Msg("Executing command")

	candidates, err := GetPackCandidates(opts.DotfilesRoot)
	if err != nil {
		return nil, err
	}

	packs, err := GetPacks(candidates)
	if err != nil {
		return nil, err
	}

	result := &types.ListPacksResult{
		Packs: make([]types.PackInfo, len(packs)),
	}

	for i, p := range packs {
		result.Packs[i] = types.PackInfo{
			Name: p.Name,
			Path: p.Path,
		}
	}

	log.Info().Str("command", "ListPacks").Int("packCount", len(result.Packs)).Msg("Command finished")
	return result, nil
}

// DeployPacks runs the deployment logic for the specified packs.
// This executes power-ups with RunModeMany.
func DeployPacks(opts DeployPacksOptions) (*types.ExecutionResult, error) {
	log := logging.GetLogger("core.commands")
	log.Debug().Str("command", "DeployPacks").Msg("Executing command")

	execOpts := executionOptions{
		DotfilesRoot: opts.DotfilesRoot,
		PackNames:    opts.PackNames,
		DryRun:       opts.DryRun,
		RunMode:      types.RunModeMany,
	}

	result, err := runExecutionPipeline(execOpts)
	if err != nil {
		return nil, err
	}

	log.Info().Str("command", "DeployPacks").Msg("Command finished")
	return result, nil
}

// InstallPacks runs the installation and deployment logic for the specified packs.
// It first executes power-ups with RunModeOnce, then those with RunModeMany.
func InstallPacks(opts InstallPacksOptions) (*types.ExecutionResult, error) {
	log := logging.GetLogger("core.commands")
	log.Debug().Str("command", "InstallPacks").Msg("Executing command")

	// Step 1: Run "once" power-ups
	onceOpts := executionOptions{
		DotfilesRoot: opts.DotfilesRoot,
		PackNames:    opts.PackNames,
		DryRun:       opts.DryRun,
		RunMode:      types.RunModeOnce,
		Force:        opts.Force,
	}
	onceResult, err := runExecutionPipeline(onceOpts)
	if err != nil {
		return nil, err
	}

	// Step 2: Run "many" power-ups (deploy)
	manyOpts := executionOptions{
		DotfilesRoot: opts.DotfilesRoot,
		PackNames:    opts.PackNames,
		DryRun:       opts.DryRun,
		RunMode:      types.RunModeMany,
	}
	manyResult, err := runExecutionPipeline(manyOpts)
	if err != nil {
		return nil, err
	}

	// Step 3: Merge results
	mergedResult := &types.ExecutionResult{
		Packs:      onceResult.Packs,
		Operations: append(onceResult.Operations, manyResult.Operations...),
		DryRun:     opts.DryRun,
	}

	log.Info().Str("command", "InstallPacks").Msg("Command finished")
	return mergedResult, nil
}

// executionOptions is an internal struct to pass to the pipeline runner.
type executionOptions struct {
	DotfilesRoot string
	PackNames    []string
	DryRun       bool
	RunMode      types.RunMode
	Force        bool
}

// runExecutionPipeline is the core logic for deploy and install.
func runExecutionPipeline(opts executionOptions) (*types.ExecutionResult, error) {
	// 1. Get all packs
	candidates, err := GetPackCandidates(opts.DotfilesRoot)
	if err != nil {
		return nil, err
	}
	allPacks, err := GetPacks(candidates)
	if err != nil {
		return nil, err
	}

	// 2. Filter to selected packs
	selectedPacks, err := SelectPacks(allPacks, opts.PackNames)
	if err != nil {
		return nil, err
	}

	// 3. Get all trigger matches for the selected packs
	matches, err := GetFiringTriggers(selectedPacks)
	if err != nil {
		return nil, err
	}

	// 4. Get all actions from the matches
	actions, err := GetActions(matches)
	if err != nil {
		return nil, err
	}

	// 5. Filter actions by the desired RunMode
	actions, err = filterActionsByRunMode(actions, opts.RunMode)
	if err != nil {
		return nil, err
	}

	// 6. For RunModeOnce, filter out actions that have already been executed
	if opts.RunMode == types.RunModeOnce {
		actions, err = FilterRunOnceActions(actions, opts.Force)
		if err != nil {
			return nil, err
		}
	}

	// 7. Convert the filtered actions to filesystem operations
	ops, err := GetFsOps(actions)
	if err != nil {
		return nil, err
	}

	// 8. Construct and return the result
	result := &types.ExecutionResult{
		Packs:      getPackNames(selectedPacks),
		Operations: ops,
		DryRun:     opts.DryRun,
	}

	return result, nil
}

// filterActionsByRunMode filters a slice of actions based on the RunMode of the
// power-up that generated them.
func filterActionsByRunMode(actions []types.Action, mode types.RunMode) ([]types.Action, error) {
	var filtered []types.Action
	for _, action := range actions {
		// The PowerUpName is stored in the action. We need to get the factory,
		// create a temporary instance (without options) just to check its RunMode.
		factory, err := registry.GetPowerUpFactory(action.PowerUpName)
		if err != nil {
			// This should be rare, as the power-up must have existed to create the action
			return nil, err
		}
		powerUp, err := factory(nil) // Options don't affect the RunMode
		if err != nil {
			return nil, err
		}

		if powerUp.RunMode() == mode {
			filtered = append(filtered, action)
		}
	}
	return filtered, nil
}

// StatusPacks checks the deployment status of the specified packs.
func StatusPacks(opts StatusPacksOptions) (*types.PackStatusResult, error) {
	log := logging.GetLogger("core.commands")
	log.Debug().Str("command", "StatusPacks").Msg("Executing command")

	// 1. Get all packs
	candidates, err := GetPackCandidates(opts.DotfilesRoot)
	if err != nil {
		return nil, err
	}
	allPacks, err := GetPacks(candidates)
	if err != nil {
		return nil, err
	}

	// 2. Filter to selected packs
	selectedPacks, err := SelectPacks(allPacks, opts.PackNames)
	if err != nil {
		return nil, err
	}

	// 3. Check status for each pack
	result := &types.PackStatusResult{
		Packs: make([]types.PackStatus, 0, len(selectedPacks)),
	}

	for _, pack := range selectedPacks {
		packStatus := types.PackStatus{
			Name:         pack.Name,
			PowerUpState: make([]types.PowerUpStatus, 0),
		}

		// Check run-once power-up status (install, brewfile)
		installStatus, err := GetRunOnceStatus(pack.Path, "install")
		if err == nil && installStatus != nil {
			state := "Not Installed"
			description := "Install script not yet executed"
			if installStatus.Executed {
				state = "Installed"
				description = fmt.Sprintf("Installed on %s", installStatus.ExecutedAt.Format("2006-01-02 15:04:05"))
				if installStatus.Changed {
					state = "Changed"
					description += " (script has changed since execution)"
				}
			}
			packStatus.PowerUpState = append(packStatus.PowerUpState, types.PowerUpStatus{
				Name:        "install",
				State:       state,
				Description: description,
			})
		}

		brewfileStatus, err := GetRunOnceStatus(pack.Path, "brewfile")
		if err == nil && brewfileStatus != nil {
			state := "Not Installed"
			description := "Brewfile not yet executed"
			if brewfileStatus.Executed {
				state = "Installed"
				description = fmt.Sprintf("Installed on %s", brewfileStatus.ExecutedAt.Format("2006-01-02 15:04:05"))
				if brewfileStatus.Changed {
					state = "Changed"
					description += " (Brewfile has changed since execution)"
				}
			}
			packStatus.PowerUpState = append(packStatus.PowerUpState, types.PowerUpStatus{
				Name:        "brewfile",
				State:       state,
				Description: description,
			})
		}

		// For symlink status, we'd need to check actual symlinks in the filesystem
		// This is a simplified version - in reality we'd check if symlinks exist
		// and point to the correct locations
		packStatus.PowerUpState = append(packStatus.PowerUpState, types.PowerUpStatus{
			Name:        "symlink",
			State:       "Unknown",
			Description: "Symlink status checking not yet implemented",
		})

		result.Packs = append(result.Packs, packStatus)
	}

	log.Info().Str("command", "StatusPacks").Int("packCount", len(result.Packs)).Msg("Command finished")
	return result, nil
}

// FillPack adds placeholder files for power-ups to an existing pack.
func FillPack(opts FillPackOptions) (*types.FillResult, error) {
	log := logging.GetLogger("core.commands")
	log.Debug().Str("command", "FillPack").Str("pack", opts.PackName).Msg("Executing command")

	// 1. Get all packs to verify the pack exists
	candidates, err := GetPackCandidates(opts.DotfilesRoot)
	if err != nil {
		return nil, err
	}
	allPacks, err := GetPacks(candidates)
	if err != nil {
		return nil, err
	}

	// 2. Find the specific pack
	var targetPack *types.Pack
	for i := range allPacks {
		if allPacks[i].Name == opts.PackName {
			targetPack = &allPacks[i]
			break
		}
	}
	if targetPack == nil {
		return nil, errors.Newf(errors.ErrPackNotFound, "pack %q not found", opts.PackName)
	}

	// 3. Create placeholder files
	result := &types.FillResult{
		PackName:     opts.PackName,
		FilesCreated: []string{},
	}

	// Define template files for each power-up
	templates := []struct {
		filename string
		content  string
	}{
		{
			filename: "aliases.sh",
			content: `#!/usr/bin/env sh
# Shell aliases for ` + opts.PackName + ` pack
# Add your aliases below

# Example:
# alias ll='ls -la'
`,
		},
		{
			filename: "install.sh",
			content: `#!/usr/bin/env bash
# Installation script for ` + opts.PackName + ` pack
# This script runs once during 'dodot install'

set -euo pipefail

echo "Installing ` + opts.PackName + ` pack..."

# Add your installation commands below
`,
		},
		{
			filename: "Brewfile",
			content: `# Homebrew dependencies for ` + opts.PackName + ` pack
# This file is processed during 'dodot install'

# Examples:
# brew 'git'
# brew 'tmux'
# cask 'visual-studio-code'
`,
		},
		{
			filename: "path.sh",
			content: `#!/usr/bin/env sh
# PATH additions for ` + opts.PackName + ` pack
# Export PATH modifications below

# Example:
# export PATH="$HOME/.local/bin:$PATH"
`,
		},
	}

	// Create each template file if it doesn't exist
	for _, tmpl := range templates {
		filePath := filepath.Join(targetPack.Path, tmpl.filename)

		// Check if file already exists
		if _, err := os.Stat(filePath); err == nil {
			log.Debug().Str("file", tmpl.filename).Msg("File already exists, skipping")
			continue
		}

		// Write the template file
		err := os.WriteFile(filePath, []byte(tmpl.content), 0644)
		if err != nil {
			return nil, errors.Wrapf(err, errors.ErrFileAccess, "failed to create %s", tmpl.filename)
		}

		// Make shell scripts executable
		if strings.HasSuffix(tmpl.filename, ".sh") {
			err = os.Chmod(filePath, 0755)
			if err != nil {
				return nil, errors.Wrapf(err, errors.ErrFileAccess, "failed to make %s executable", tmpl.filename)
			}
		}

		result.FilesCreated = append(result.FilesCreated, tmpl.filename)
		log.Info().Str("file", tmpl.filename).Msg("Created template file")
	}

	log.Info().Str("command", "FillPack").
		Str("pack", opts.PackName).
		Int("filesCreated", len(result.FilesCreated)).
		Msg("Command finished")
	return result, nil
}

// InitPack creates a new pack directory with template files and configuration.
func InitPack(opts InitPackOptions) (*types.InitResult, error) {
	log := logging.GetLogger("core.commands")
	log.Debug().Str("command", "InitPack").Str("pack", opts.PackName).Msg("Executing command")

	// 1. Validate pack name
	if opts.PackName == "" {
		return nil, errors.New(errors.ErrInvalidInput, "pack name cannot be empty")
	}
	
	// Check for invalid characters in pack name
	if strings.ContainsAny(opts.PackName, "/\\:*?\"<>|") {
		return nil, errors.Newf(errors.ErrInvalidInput, "pack name contains invalid characters: %s", opts.PackName)
	}

	// 2. Create the pack directory
	packPath := filepath.Join(opts.DotfilesRoot, opts.PackName)
	
	// Check if pack already exists
	if _, err := os.Stat(packPath); err == nil {
		return nil, errors.Newf(errors.ErrPackExists, "pack %q already exists", opts.PackName)
	}

	// Create the directory
	err := os.MkdirAll(packPath, 0755)
	if err != nil {
		return nil, errors.Wrapf(err, errors.ErrFileAccess, "failed to create pack directory")
	}

	result := &types.InitResult{
		PackName:     opts.PackName,
		Path:         packPath,
		FilesCreated: []string{},
	}

	// 3. Create .dodot.toml configuration file
	configContent := `# dodot configuration for ` + opts.PackName + ` pack
# See https://github.com/arthur-debert/dodot for documentation

# Uncomment to skip this pack during deployment
# skip = true

# File-specific rules
[files]
# Ignore specific files
# "*.bak" = "ignore"
# "*.tmp" = "ignore"

# Override default power-up for specific files
# "my-script.sh" = "install"
# "my-aliases.sh" = "profile"
`

	configPath := filepath.Join(packPath, ".dodot.toml")
	err = os.WriteFile(configPath, []byte(configContent), 0644)
	if err != nil {
		return nil, errors.Wrapf(err, errors.ErrFileAccess, "failed to create .dodot.toml")
	}
	result.FilesCreated = append(result.FilesCreated, ".dodot.toml")

	// 4. Create README.txt
	readmeContent := `dodot Pack: ` + opts.PackName + `
====================

This pack was created by dodot init. It contains configuration files and scripts
for the ` + opts.PackName + ` environment.

Files in this pack:
- .dodot.toml     - Pack configuration
- aliases.sh      - Shell aliases (sourced in shell profile)
- install.sh      - Installation script (runs once during 'dodot install')
- Brewfile        - Homebrew dependencies (processed during 'dodot install')
- path.sh         - PATH modifications (sourced in shell profile)
- README.txt      - This file

Getting Started:
1. Add your dotfiles to this directory
2. Edit the template files to add your configurations
3. Run 'dodot deploy ` + opts.PackName + `' to deploy this pack

For more information, see: https://github.com/arthur-debert/dodot
`

	readmePath := filepath.Join(packPath, "README.txt")
	err = os.WriteFile(readmePath, []byte(readmeContent), 0644)
	if err != nil {
		return nil, errors.Wrapf(err, errors.ErrFileAccess, "failed to create README.txt")
	}
	result.FilesCreated = append(result.FilesCreated, "README.txt")

	// 5. Use FillPack to create the template files
	//nolint:staticcheck // Explicit struct construction is clearer than conversion
	fillOpts := FillPackOptions{
		DotfilesRoot: opts.DotfilesRoot,
		PackName:     opts.PackName,
	}
	fillResult, err := FillPack(fillOpts)
	if err != nil {
		return nil, errors.Wrapf(err, errors.ErrPackInit, "failed to create template files")
	}

	// Add the filled files to our result
	result.FilesCreated = append(result.FilesCreated, fillResult.FilesCreated...)

	log.Info().Str("command", "InitPack").
		Str("pack", opts.PackName).
		Str("path", packPath).
		Int("filesCreated", len(result.FilesCreated)).
		Msg("Command finished")
	return result, nil
}
