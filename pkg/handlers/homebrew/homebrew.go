package homebrew

import (
	_ "embed"
	"fmt"
	"os"
	"path/filepath"
	"strings"

	"github.com/arthur-debert/dodot/pkg/internal/hashutil"
	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/types"
)

// HomebrewHandlerName is the name of the homebrew handler
const HomebrewHandlerName = "homebrew"

// homebrewTemplate is the template content for Brewfile
const homebrewTemplate = `# Homebrew dependencies for PACK_NAME pack
# 
# This file is processed by 'dodot install PACK_NAME' to install
# packages using Homebrew. Each package is installed once during
# initial deployment. The deployment is tracked by checksum, so
# modifying this file will trigger a re-run.
#
# Safe to keep empty or remove. By keeping it, you can add
# homebrew packages later without redeploying the pack.

# Examples:
# brew "git"
# brew "vim"
# cask "visual-studio-code"`

// HomebrewHandler processes Brewfiles to install packages via Homebrew
type HomebrewHandler struct{}

// NewHomebrewHandler creates a new instance of the Homebrew handler
func NewHomebrewHandler() *HomebrewHandler {
	return &HomebrewHandler{}
}

// Name returns the unique name of this handler
func (h *HomebrewHandler) Name() string {
	return HomebrewHandlerName
}

// Description returns a human-readable description of what this handler does
func (h *HomebrewHandler) Description() string {
	return "Processes Brewfiles to install Homebrew packages"
}

// RunMode returns whether this handler runs once or many times
func (h *HomebrewHandler) RunMode() types.RunMode {
	return types.RunModeProvisioning
}

// ProcessProvisioning takes Brewfile matches and generates RunScriptAction instances
func (h *HomebrewHandler) ProcessProvisioning(matches []types.TriggerMatch) ([]types.ProvisioningAction, error) {
	logger := logging.GetLogger("handlers.homebrew")
	actions := make([]types.ProvisioningAction, 0, len(matches))

	for _, match := range matches {
		logger.Debug().
			Str("path", match.Path).
			Str("pack", match.Pack).
			Msg("Processing Brewfile")

		// Calculate checksum of the Brewfile
		checksum, err := hashutil.CalculateFileChecksum(match.AbsolutePath)
		if err != nil {
			logger.Error().
				Err(err).
				Str("path", match.AbsolutePath).
				Msg("Failed to calculate checksum")
			return nil, fmt.Errorf("failed to calculate checksum for %s: %w", match.AbsolutePath, err)
		}

		// Create a BrewAction for the Brewfile
		action := &types.BrewAction{
			PackName:     match.Pack,
			BrewfilePath: match.AbsolutePath,
			Checksum:     checksum,
		}

		actions = append(actions, action)
	}

	logger.Info().
		Int("match_count", len(matches)).
		Int("action_count", len(actions)).
		Msg("processed Brewfile matches")

	return actions, nil
}

// ValidateOptions checks if the provided options are valid for this handler
func (h *HomebrewHandler) ValidateOptions(options map[string]interface{}) error {
	// Homebrew handler doesn't have any options
	return nil
}

// GetTemplateContent returns the template content for this handler
func (h *HomebrewHandler) GetTemplateContent() string {
	return homebrewTemplate
}

// Clear prepares for homebrew uninstallation (reads state, future: uninstalls)
func (h *HomebrewHandler) Clear(ctx types.ClearContext) ([]types.ClearedItem, error) {
	logger := logging.GetLogger("handlers.homebrew").With().
		Str("pack", ctx.Pack.Name).
		Bool("dryRun", ctx.DryRun).
		Logger()

	clearedItems := []types.ClearedItem{}

	// Read state to understand what was installed
	stateDir := ctx.Paths.PackHandlerDir(ctx.Pack.Name, "homebrew")
	entries, err := ctx.FS.ReadDir(stateDir)
	if err != nil {
		if os.IsNotExist(err) {
			logger.Debug().Msg("No homebrew state directory")
			return clearedItems, nil
		}
		return nil, fmt.Errorf("failed to read homebrew state: %w", err)
	}

	// Find Brewfile sentinels and extract what was installed
	for _, entry := range entries {
		if entry.IsDir() || !strings.HasSuffix(entry.Name(), ".sentinel") {
			continue
		}

		// Extract Brewfile name from sentinel (e.g., "testpack_Brewfile.sentinel" -> "Brewfile")
		brewfileName := strings.TrimSuffix(entry.Name(), ".sentinel")
		if idx := strings.Index(brewfileName, "_"); idx >= 0 {
			brewfileName = brewfileName[idx+1:]
		}

		// TODO: In a future release:
		// 1. Read the actual Brewfile from the pack to see what packages were specified
		// 2. Prompt user: "These packages were installed by this pack: X, Y, Z. Uninstall them?"
		// 3. Run `brew uninstall` for confirmed packages
		// 4. Return list of uninstalled packages

		logger.Info().
			Str("brewfile", brewfileName).
			Str("sentinel", entry.Name()).
			Msg("Found Brewfile installation record")

		if ctx.DryRun {
			clearedItems = append(clearedItems, types.ClearedItem{
				Type:        "homebrew_state",
				Path:        filepath.Join(stateDir, entry.Name()),
				Description: fmt.Sprintf("Would remove Homebrew state for %s (packages not uninstalled)", brewfileName),
			})
		} else {
			clearedItems = append(clearedItems, types.ClearedItem{
				Type:        "homebrew_state",
				Path:        filepath.Join(stateDir, entry.Name()),
				Description: fmt.Sprintf("Removing Homebrew state for %s (uninstall not yet implemented)", brewfileName),
			})
		}
	}

	if len(clearedItems) == 0 && len(entries) > 0 {
		// Had entries but no sentinels
		if ctx.DryRun {
			clearedItems = append(clearedItems, types.ClearedItem{
				Type:        "homebrew_state",
				Path:        stateDir,
				Description: "Would remove Homebrew state directory",
			})
		} else {
			clearedItems = append(clearedItems, types.ClearedItem{
				Type:        "homebrew_state",
				Path:        stateDir,
				Description: "Removing Homebrew state directory",
			})
		}
	}

	return clearedItems, nil
}

// Verify interface compliance
var _ types.ProvisioningHandler = (*HomebrewHandler)(nil)
var _ types.Clearable = (*HomebrewHandler)(nil)
