package homebrew

import (
	_ "embed"
	"fmt"
	"os"
	"path/filepath"

	"github.com/arthur-debert/dodot/pkg/handlers"
	"github.com/arthur-debert/dodot/pkg/operations"
	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/arthur-debert/dodot/pkg/utils"
)

const HomebrewHandlerName = "homebrew"

//go:embed homebrew-template.txt
var brewfileTemplate string

// Handler implements the new simplified handler interface.
// It transforms Brewfile requests into operations without performing any I/O.
type Handler struct {
	operations.BaseHandler
}

// NewHandler creates a new simplified homebrew handler.
func NewHandler() *Handler {
	return &Handler{
		BaseHandler: operations.NewBaseHandler(HomebrewHandlerName, handlers.CategoryCodeExecution),
	}
}

// ToOperations converts rule matches to homebrew operations.
// Brewfiles use RunCommand with brew bundle for installation.
func (h *Handler) ToOperations(matches []types.RuleMatch) ([]operations.Operation, error) {
	var ops []operations.Operation

	for _, match := range matches {
		// Calculate checksum for idempotency
		checksum, err := utils.CalculateFileChecksum(match.AbsolutePath)
		if err != nil {
			return nil, fmt.Errorf("failed to calculate checksum for %s: %w", match.AbsolutePath, err)
		}

		// Create sentinel name from Brewfile and checksum
		sentinelName := fmt.Sprintf("%s_%s-%s", match.Pack, filepath.Base(match.Path), checksum)

		// Brewfiles are processed with brew bundle
		// The executor will check the sentinel and skip if already run
		ops = append(ops, operations.Operation{
			Type:     operations.RunCommand,
			Pack:     match.Pack,
			Handler:  HomebrewHandlerName,
			Command:  fmt.Sprintf("brew bundle --file='%s'", match.AbsolutePath),
			Sentinel: sentinelName,
		})
	}

	return ops, nil
}

// GetMetadata returns handler metadata.
func (h *Handler) GetMetadata() operations.HandlerMetadata {
	return operations.HandlerMetadata{
		Description:     "Processes Brewfiles to install Homebrew packages",
		RequiresConfirm: false, // Installation doesn't need confirmation
		CanRunMultiple:  false, // Only run once per checksum
	}
}

// GetTemplateContent returns the template content for this handler.
func (h *Handler) GetTemplateContent() string {
	return brewfileTemplate
}

// GetStateDirectoryName returns the directory name for storing state.
func (h *Handler) GetStateDirectoryName() string {
	return "homebrew"
}

// GetClearConfirmation returns confirmation request for clearing if needed.
// Homebrew uninstalls require explicit confirmation via DODOT_HOMEBREW_UNINSTALL.
func (h *Handler) GetClearConfirmation(ctx operations.ClearContext) *operations.ConfirmationRequest {
	if os.Getenv("DODOT_HOMEBREW_UNINSTALL") != "true" {
		return nil
	}

	return &operations.ConfirmationRequest{
		ID:          fmt.Sprintf("homebrew_uninstall_%s", ctx.Pack.Name),
		Title:       "Uninstall Homebrew packages?",
		Description: fmt.Sprintf("This will uninstall Homebrew packages from %s pack", ctx.Pack.Name),
		Items:       []string{"Package uninstallation may affect other applications"},
	}
}

// FormatClearedItem formats a cleared item for display.
func (h *Handler) FormatClearedItem(item operations.ClearedItem, dryRun bool) string {
	uninstallEnabled := os.Getenv("DODOT_HOMEBREW_UNINSTALL") == "true"

	if dryRun {
		if uninstallEnabled {
			return "Would uninstall Homebrew packages and remove state"
		}
		return "Would remove Homebrew state (set DODOT_HOMEBREW_UNINSTALL=true to uninstall packages)"
	}

	if uninstallEnabled {
		return "Uninstalling Homebrew packages and removing state"
	}
	return "Removing Homebrew state (set DODOT_HOMEBREW_UNINSTALL=true to uninstall packages)"
}

// Verify interface compliance
var _ operations.Handler = (*Handler)(nil)
