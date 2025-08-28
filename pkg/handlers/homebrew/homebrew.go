package homebrew

import (
	_ "embed"
	"fmt"
	"os"
	"path/filepath"
	"strings"

	"github.com/arthur-debert/dodot/pkg/handlers"
	"github.com/arthur-debert/dodot/pkg/internal/hashutil"
	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/types"
)

// HomebrewHandlerName is the name of the homebrew handler
const HomebrewHandlerName = "homebrew"

//go:embed homebrew-template.txt
var brewfileTemplate string

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

// Type returns the fundamental nature of this handler's operations
func (h *HomebrewHandler) Type() types.HandlerType {
	return types.HandlerTypeCodeExecution
}

// ProcessProvisioning takes Brewfile matches and generates RunScriptAction instances
func (h *HomebrewHandler) ProcessProvisioning(matches []types.RuleMatch) ([]types.ProvisioningAction, error) {
	result, err := h.ProcessProvisioningWithConfirmations(matches)
	if err != nil {
		return nil, err
	}

	// Convert ProcessingResult actions to ProvisioningAction slice for backward compatibility
	provisioningActions := make([]types.ProvisioningAction, 0, len(result.Actions))
	for _, action := range result.Actions {
		if provAction, ok := action.(types.ProvisioningAction); ok {
			provisioningActions = append(provisioningActions, provAction)
		}
	}

	return provisioningActions, nil
}

// ProcessProvisioningWithConfirmations implements ProvisioningHandlerWithConfirmations
func (h *HomebrewHandler) ProcessProvisioningWithConfirmations(matches []types.RuleMatch) (types.ProcessingResult, error) {
	logger := logging.GetLogger("handlers.homebrew")
	actions := make([]types.Action, 0, len(matches))

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
			return types.ProcessingResult{}, fmt.Errorf("failed to calculate checksum for %s: %w", match.AbsolutePath, err)
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

	// Homebrew provisioning doesn't need confirmation - it's just installing packages
	// Confirmation is only needed for clearing/uninstalling
	return types.NewProcessingResult(actions), nil
}

// ValidateOptions checks if the provided options are valid for this handler
func (h *HomebrewHandler) ValidateOptions(options map[string]interface{}) error {
	// Homebrew handler doesn't have any options
	return nil
}

// GetTemplateContent returns the template content for this handler
func (h *HomebrewHandler) GetTemplateContent() string {
	return brewfileTemplate
}

// Clear prepares for homebrew uninstallation (reads state, optionally uninstalls)
func (h *HomebrewHandler) Clear(ctx types.ClearContext) ([]types.ClearedItem, error) {
	// Check if full uninstall is enabled
	if os.Getenv("DODOT_HOMEBREW_UNINSTALL") == "true" {
		return h.ClearWithUninstall(ctx)
	}

	// Otherwise use the basic implementation
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

		logger.Info().
			Str("brewfile", brewfileName).
			Str("sentinel", entry.Name()).
			Msg("Found Brewfile installation record")

		if ctx.DryRun {
			clearedItems = append(clearedItems, types.ClearedItem{
				Type:        "homebrew_state",
				Path:        filepath.Join(stateDir, entry.Name()),
				Description: fmt.Sprintf("Would remove Homebrew state for %s (set DODOT_HOMEBREW_UNINSTALL=true to uninstall packages)", brewfileName),
			})
		} else {
			clearedItems = append(clearedItems, types.ClearedItem{
				Type:        "homebrew_state",
				Path:        filepath.Join(stateDir, entry.Name()),
				Description: fmt.Sprintf("Removing Homebrew state for %s (set DODOT_HOMEBREW_UNINSTALL=true to uninstall packages)", brewfileName),
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

// GetClearConfirmations implements ClearableWithConfirmations interface
func (h *HomebrewHandler) GetClearConfirmations(ctx types.ClearContext) ([]types.ConfirmationRequest, error) {
	// Only provide confirmations if DODOT_HOMEBREW_UNINSTALL is enabled
	if os.Getenv("DODOT_HOMEBREW_UNINSTALL") != "true" {
		return []types.ConfirmationRequest{}, nil
	}

	logger := logging.GetLogger("handlers.homebrew").With().
		Str("pack", ctx.Pack.Name).
		Logger()

	// Read state to find packages to uninstall
	stateDir := ctx.Paths.PackHandlerDir(ctx.Pack.Name, "homebrew")
	entries, err := ctx.FS.ReadDir(stateDir)
	if err != nil {
		if os.IsNotExist(err) {
			logger.Debug().Msg("No homebrew state directory")
			return []types.ConfirmationRequest{}, nil
		}
		return nil, fmt.Errorf("failed to read homebrew state: %w", err)
	}

	// Get installed packages list
	installedPackages, err := getInstalledPackages()
	if err != nil {
		logger.Warn().Err(err).Msg("Failed to get installed packages list")
		// Continue anyway - we'll generate a generic confirmation
	}

	var allPackagesToUninstall []brewPackage

	// Find all packages from Brewfiles that are currently installed
	for _, entry := range entries {
		if entry.IsDir() || !strings.HasSuffix(entry.Name(), ".sentinel") {
			continue
		}

		// Find the corresponding Brewfile
		brewfileName := strings.TrimSuffix(entry.Name(), ".sentinel")
		if idx := strings.Index(brewfileName, "_"); idx >= 0 {
			brewfileName = brewfileName[idx+1:]
		}

		brewfilePath := filepath.Join(ctx.Pack.Path, brewfileName)

		// Parse the Brewfile to get packages
		packages, err := parseBrewfile(ctx.FS, brewfilePath)
		if err != nil {
			logger.Warn().
				Err(err).
				Str("brewfile", brewfilePath).
				Msg("Failed to parse Brewfile for confirmation")
			continue
		}

		// Filter to only installed packages
		for _, pkg := range packages {
			if installedPackages == nil || installedPackages[pkg.Name] {
				allPackagesToUninstall = append(allPackagesToUninstall, pkg)
			}
		}
	}

	if len(allPackagesToUninstall) == 0 {
		return []types.ConfirmationRequest{}, nil
	}

	// Group packages by type for display
	var brewNames, caskNames []string
	for _, pkg := range allPackagesToUninstall {
		switch pkg.Type {
		case "brew":
			brewNames = append(brewNames, pkg.Name)
		case "cask":
			caskNames = append(caskNames, pkg.Name)
		}
	}

	// Build description
	var description strings.Builder
	description.WriteString("Uninstall Homebrew packages from this pack?")

	var items []string
	if len(brewNames) > 0 {
		items = append(items, fmt.Sprintf("Formulae: %s", strings.Join(brewNames, ", ")))
	}
	if len(caskNames) > 0 {
		items = append(items, fmt.Sprintf("Casks: %s", strings.Join(caskNames, ", ")))
	}

	confirmationID := fmt.Sprintf("homebrew-clear-%s", ctx.Pack.Name)
	confirmation := types.ConfirmationRequest{
		ID:          confirmationID,
		Pack:        ctx.Pack.Name,
		Handler:     "homebrew",
		Operation:   "clear",
		Title:       "Uninstall Homebrew packages",
		Description: description.String(),
		Items:       items,
		Default:     false, // Default to No for package uninstallation
	}

	return []types.ConfirmationRequest{confirmation}, nil
}

// ClearWithConfirmations implements ClearableWithConfirmations interface
func (h *HomebrewHandler) ClearWithConfirmations(ctx types.ClearContext, confirmations *types.ConfirmationContext) ([]types.ClearedItem, error) {
	// Check if uninstall is enabled and user approved
	shouldUninstall := os.Getenv("DODOT_HOMEBREW_UNINSTALL") == "true"

	if shouldUninstall && confirmations != nil {
		confirmationID := fmt.Sprintf("homebrew-clear-%s", ctx.Pack.Name)
		shouldUninstall = confirmations.IsApproved(confirmationID)
	}

	if shouldUninstall {
		return h.ClearWithUninstall(ctx)
	}

	// Fall back to basic clear (just remove state)
	return h.Clear(ctx)
}

// init registers the homebrew handler factory
// func init() {
// 	handlerFactoryRegistry := registry.GetRegistry[registry.HandlerFactory]()
// 	registry.MustRegister(handlerFactoryRegistry, HomebrewHandlerName, func(options map[string]interface{}) (interface{}, error) {
// 		handler := NewHomebrewHandler()
//
// 		// Apply options if provided
// 		if options != nil {
// 			if err := handler.ValidateOptions(options); err != nil {
// 				return nil, err
// 			}
// 		}
//
// 		return handler, nil
// 	})
// }

// Verify interface compliance
var _ handlers.ProvisioningHandler = (*HomebrewHandler)(nil)
var _ handlers.ProvisioningHandlerWithConfirmations = (*HomebrewHandler)(nil)
var _ handlers.Clearable = (*HomebrewHandler)(nil)
var _ handlers.ClearableWithConfirmations = (*HomebrewHandler)(nil)
