package commands

import (
	"fmt"
	"os"
	"path/filepath"
	"strings"

	"github.com/arthur-debert/dodot/pkg/core"
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

	candidates, err := core.GetPackCandidates(opts.DotfilesRoot)
	if err != nil {
		return nil, err
	}

	packs, err := core.GetPacks(candidates)
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
		Force:        opts.Force,
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
	logger := logging.GetLogger("core.commands")

	// 1. Get all packs
	candidates, err := core.GetPackCandidates(opts.DotfilesRoot)
	if err != nil {
		return nil, err
	}
	allPacks, err := core.GetPacks(candidates)
	if err != nil {
		return nil, err
	}

	// 2. Filter to selected packs
	selectedPacks, err := core.SelectPacks(allPacks, opts.PackNames)
	if err != nil {
		return nil, err
	}

	// 3. Get all trigger matches for the selected packs
	matches, err := core.GetFiringTriggers(selectedPacks)
	if err != nil {
		return nil, err
	}

	// 4. Get all actions from the matches
	actions, err := core.GetActions(matches)
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
		actions, err = core.FilterRunOnceActions(actions, opts.Force)
		if err != nil {
			return nil, err
		}
	}

	// 7. Check if we need to handle checksum operations
	ctx := core.NewExecutionContext(opts.Force)

	// Check if any actions need checksums
	hasChecksumActions := false
	for _, action := range actions {
		if action.Type == types.ActionTypeChecksum ||
			action.Type == types.ActionTypeBrew ||
			action.Type == types.ActionTypeInstall {
			hasChecksumActions = true
			break
		}
	}

	var ops []types.Operation

	if hasChecksumActions {
		// First pass: generate all operations to find checksum operations
		initialOps, err := core.GetFileOperationsWithContext(actions, ctx)
		if err != nil {
			return nil, err
		}

		// Execute checksum operations to get results
		checksumResults, err := ctx.ExecuteChecksumOperations(initialOps)
		if err != nil {
			return nil, err
		}

		if len(checksumResults) > 0 {
			logger.Info().Int("checksumCount", len(checksumResults)).Msg("Executed checksum operations")
		}

		// Re-generate final operations with checksum context.
		// This is necessary because the first pass might have skipped some actions
		// if their checksums weren't available yet.
		ops, err = core.GetFileOperationsWithContext(actions, ctx)
		if err != nil {
			return nil, err
		}
	} else {
		// No checksum operations needed, generate operations once
		ops, err = core.GetFileOperationsWithContext(actions, ctx)
		if err != nil {
			return nil, err
		}
	}

	// 9. Construct and return the result
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
	candidates, err := core.GetPackCandidates(opts.DotfilesRoot)
	if err != nil {
		return nil, err
	}
	allPacks, err := core.GetPacks(candidates)
	if err != nil {
		return nil, err
	}

	// 2. Filter to selected packs
	selectedPacks, err := core.SelectPacks(allPacks, opts.PackNames)
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
		installStatus, err := core.GetRunOnceStatus(pack.Path, "install")
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

		brewfileStatus, err := core.GetRunOnceStatus(pack.Path, "brewfile")
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
	candidates, err := core.GetPackCandidates(opts.DotfilesRoot)
	if err != nil {
		return nil, err
	}
	allPacks, err := core.GetPacks(candidates)
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

	// 3. Get missing template files
	missingTemplates, err := core.GetMissingTemplateFiles(targetPack.Path, opts.PackName)
	if err != nil {
		return nil, errors.Wrapf(err, errors.ErrInternal, "failed to get missing templates")
	}

	// 4. Generate actions for missing templates
	var actions []types.Action
	for _, template := range missingTemplates {
		action := types.Action{
			Type:        types.ActionTypeWrite,
			Description: fmt.Sprintf("Create template file %s", template.Filename),
			Target:      filepath.Join(targetPack.Path, template.Filename),
			Content:     template.Content,
			Mode:        template.Mode,
			Pack:        opts.PackName,
			PowerUpName: template.PowerUpName,
			Priority:    50, // Lower priority for template files
		}
		actions = append(actions, action)
	}

	// 5. Convert actions to operations
	ops, err := core.GetFileOperations(actions)
	if err != nil {
		return nil, errors.Wrapf(err, errors.ErrActionInvalid, "failed to convert actions to operations")
	}

	// 6. Return result with operations
	result := &types.FillResult{
		PackName:     opts.PackName,
		FilesCreated: []string{},
		Operations:   ops,
	}

	// List files that will be created
	for _, template := range missingTemplates {
		result.FilesCreated = append(result.FilesCreated, template.Filename)
		log.Info().
			Str("file", template.Filename).
			Str("powerup", template.PowerUpName).
			Msg("Template file to be created")
	}

	log.Debug().
		Int("actionCount", len(actions)).
		Int("operationCount", len(ops)).
		Msg("Generated operations for FillPack")

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

	// 2. Build pack path and check if it exists
	packPath := filepath.Join(opts.DotfilesRoot, opts.PackName)

	// Check if pack already exists
	if _, err := os.Stat(packPath); err == nil {
		return nil, errors.Newf(errors.ErrPackExists, "pack %q already exists", opts.PackName)
	}

	// 3. Generate actions for creating pack
	var actions []types.Action

	// Create directory action
	mkdirAction := types.Action{
		Type:        types.ActionTypeMkdir,
		Description: fmt.Sprintf("Create pack directory %s", opts.PackName),
		Target:      packPath,
		Mode:        0755,
		Pack:        opts.PackName,
		PowerUpName: "init_pack_internal",
		Priority:    200, // Higher priority to create dir first
	}
	actions = append(actions, mkdirAction)

	// Create .dodot.toml configuration file
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
	configAction := types.Action{
		Type:        types.ActionTypeWrite,
		Description: "Create .dodot.toml configuration",
		Target:      configPath,
		Content:     configContent,
		Mode:        0644,
		Pack:        opts.PackName,
		PowerUpName: "init_pack_internal",
		Priority:    100,
	}
	actions = append(actions, configAction)

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
	readmeAction := types.Action{
		Type:        types.ActionTypeWrite,
		Description: "Create README.txt",
		Target:      readmePath,
		Content:     readmeContent,
		Mode:        0644,
		Pack:        opts.PackName,
		PowerUpName: "init_pack_internal",
		Priority:    100,
	}
	actions = append(actions, readmeAction)

	// 5. Get all template files for the pack
	templates, err := core.GetCompletePackTemplate(opts.PackName)
	if err != nil {
		return nil, errors.Wrapf(err, errors.ErrInternal, "failed to get pack templates")
	}

	// Add template file actions
	for _, template := range templates {
		action := types.Action{
			Type:        types.ActionTypeWrite,
			Description: fmt.Sprintf("Create template file %s", template.Filename),
			Target:      filepath.Join(packPath, template.Filename),
			Content:     template.Content,
			Mode:        template.Mode,
			Pack:        opts.PackName,
			PowerUpName: template.PowerUpName,
			Priority:    50, // Lower priority for template files
		}
		actions = append(actions, action)
	}

	// 6. Convert actions to operations
	ops, err := core.GetFileOperations(actions)
	if err != nil {
		return nil, errors.Wrapf(err, errors.ErrActionInvalid, "failed to convert actions to operations")
	}

	// 7. Return result with operations
	result := &types.InitResult{
		PackName:     opts.PackName,
		Path:         packPath,
		FilesCreated: []string{},
		Operations:   ops,
	}

	// Report all files that would be created
	for _, action := range actions {
		switch action.Type {
		case types.ActionTypeMkdir:
			log.Info().
				Str("directory", action.Target).
				Str("operation", "would create").
				Msg("Pack directory to be created")
		case types.ActionTypeWrite:
			filename := filepath.Base(action.Target)
			result.FilesCreated = append(result.FilesCreated, filename)
			log.Info().
				Str("file", filename).
				Str("operation", "would create").
				Msg("File to be created")
		}
	}

	log.Debug().
		Int("actionCount", len(actions)).
		Int("operationCount", len(ops)).
		Msg("Generated operations for InitPack")

	log.Info().Str("command", "InitPack").
		Str("pack", opts.PackName).
		Str("path", packPath).
		Int("filesCreated", len(result.FilesCreated)).
		Msg("Command finished")
	return result, nil
}

// getPackNames returns a list of pack names
func getPackNames(packs []types.Pack) []string {
	names := make([]string, len(packs))
	for i, pack := range packs {
		names[i] = pack.Name
	}
	return names
}
