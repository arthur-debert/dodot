package core

import (
	"fmt"

	"github.com/arthur-debert/dodot/pkg/handlers"
	"github.com/arthur-debert/dodot/pkg/handlers/registry"
	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/types"
)

// ActionGenerationResult holds both actions and confirmations from handler processing
type ActionGenerationResult struct {
	Actions       []types.Action
	Confirmations []types.ConfirmationRequest
}

// HasConfirmations returns true if there are any confirmation requests
func (r ActionGenerationResult) HasConfirmations() bool {
	return len(r.Confirmations) > 0
}

// GetActions takes trigger matches grouped by handler and calls the appropriate handler methods (backward compatibility)
func GetActions(matches []types.TriggerMatch) ([]types.Action, error) {
	result, err := GetActionsWithConfirmations(matches)
	if err != nil {
		return nil, err
	}
	return result.Actions, nil
}

// GetActionsWithConfirmations takes trigger matches and returns both actions and confirmation requests
func GetActionsWithConfirmations(matches []types.TriggerMatch) (ActionGenerationResult, error) {
	logger := logging.GetLogger("core.actions")

	// Group matches by handler
	handlerGroups := groupMatchesByHandler(matches)

	var allActions []types.Action
	var allConfirmations []types.ConfirmationRequest

	for handlerName, handlerMatches := range handlerGroups {
		logger.Debug().
			Str("handler", handlerName).
			Int("matches", len(handlerMatches)).
			Msg("Processing matches for handler")

		// Get handler directly
		handler := registry.GetHandler(handlerName)
		if handler == nil {
			logger.Warn().
				Str("handler", handlerName).
				Msg("No V2 handler found, skipping")
			continue
		}

		// Process based on handler type, preferring confirmation-capable interfaces
		switch h := handler.(type) {
		case handlers.LinkingHandlerWithConfirmations:
			// Use confirmation-capable interface
			result, err := h.ProcessLinkingWithConfirmations(handlerMatches)
			if err != nil {
				return ActionGenerationResult{}, fmt.Errorf("handler %s failed to process linking with confirmations: %w", handlerName, err)
			}
			allActions = append(allActions, result.Actions...)
			allConfirmations = append(allConfirmations, result.Confirmations...)

		case handlers.ProvisioningHandlerWithConfirmations:
			// Use confirmation-capable interface
			result, err := h.ProcessProvisioningWithConfirmations(handlerMatches)
			if err != nil {
				return ActionGenerationResult{}, fmt.Errorf("handler %s failed to process provisioning with confirmations: %w", handlerName, err)
			}
			allActions = append(allActions, result.Actions...)
			allConfirmations = append(allConfirmations, result.Confirmations...)

		case handlers.LinkingHandler:
			// Fallback to basic linking interface
			linkingActions, err := h.ProcessLinking(handlerMatches)
			if err != nil {
				return ActionGenerationResult{}, fmt.Errorf("handler %s failed to process linking: %w", handlerName, err)
			}
			// Convert LinkingAction to Action
			for _, action := range linkingActions {
				allActions = append(allActions, action)
			}

		case handlers.ProvisioningHandler:
			// Fallback to basic provisioning interface
			provisioningActions, err := h.ProcessProvisioning(handlerMatches)
			if err != nil {
				return ActionGenerationResult{}, fmt.Errorf("handler %s failed to process provisioning: %w", handlerName, err)
			}
			// Convert ProvisioningAction to Action
			for _, action := range provisioningActions {
				allActions = append(allActions, action)
			}

		default:
			logger.Warn().
				Str("handler", handlerName).
				Msg("Handler does not implement V2 interfaces, skipping")
		}
	}

	logger.Info().
		Int("totalActions", len(allActions)).
		Int("totalConfirmations", len(allConfirmations)).
		Msg("Generated V2 actions and confirmations from trigger matches")

	return ActionGenerationResult{
		Actions:       allActions,
		Confirmations: allConfirmations,
	}, nil
}

// groupMatchesByHandler groups trigger matches by their handler name
func groupMatchesByHandler(matches []types.TriggerMatch) map[string][]types.TriggerMatch {
	groups := make(map[string][]types.TriggerMatch)

	for _, match := range matches {
		if match.HandlerName != "" {
			groups[match.HandlerName] = append(groups[match.HandlerName], match)
		}
	}

	return groups
}

// FilterActionsByRunMode filters actions based on their type
func FilterActionsByRunMode(actions []types.Action, mode types.RunMode) []types.Action {
	var filtered []types.Action

	for _, action := range actions {
		// Check if action is a linking or provisioning type
		switch action.(type) {
		case types.LinkingAction:
			if mode == types.RunModeLinking {
				filtered = append(filtered, action)
			}
		case types.ProvisioningAction:
			if mode == types.RunModeProvisioning {
				filtered = append(filtered, action)
			}
		default:
			// Include unknown action types in all modes
			filtered = append(filtered, action)
		}
	}

	return filtered
}

// FilterProvisioningActions filters provisioning actions based on whether they need to run
func FilterProvisioningActions(actions []types.Action, force bool, dataStore types.DataStore) ([]types.Action, error) {
	if force {
		// If force is true, run all actions
		return actions, nil
	}

	logger := logging.GetLogger("core.actions")
	var filtered []types.Action

	for _, action := range actions {
		// Only filter provisioning actions
		switch a := action.(type) {
		case *types.RunScriptAction:
			// Check if needs provisioning
			needs, err := dataStore.NeedsProvisioning(a.PackName, a.SentinelName, a.Checksum)
			if err != nil {
				return nil, fmt.Errorf("failed to check provisioning status: %w", err)
			}
			if needs {
				filtered = append(filtered, action)
			} else {
				logger.Debug().
					Str("pack", a.PackName).
					Str("script", a.ScriptPath).
					Msg("Skipping already provisioned script")
			}

		case *types.BrewAction:
			// Check if needs provisioning
			sentinelName := fmt.Sprintf("homebrew-%s.sentinel", a.PackName)
			needs, err := dataStore.NeedsProvisioning(a.PackName, sentinelName, a.Checksum)
			if err != nil {
				return nil, fmt.Errorf("failed to check provisioning status: %w", err)
			}
			if needs {
				filtered = append(filtered, action)
			} else {
				logger.Debug().
					Str("pack", a.PackName).
					Str("brewfile", a.BrewfilePath).
					Msg("Skipping already provisioned Brewfile")
			}

		default:
			// Non-provisioning actions are always included
			filtered = append(filtered, action)
		}
	}

	return filtered, nil
}
