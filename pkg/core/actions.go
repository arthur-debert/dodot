package core

import (
	"fmt"

	"github.com/arthur-debert/dodot/pkg/handlers"
	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/types"
)

// GetActions takes trigger matches grouped by handler and calls the appropriate handler methods
func GetActions(matches []types.TriggerMatch) ([]types.Action, error) {
	logger := logging.GetLogger("core.actions")

	// Group matches by handler
	handlerGroups := groupMatchesByHandler(matches)

	var allActions []types.Action

	for handlerName, handlerMatches := range handlerGroups {
		logger.Debug().
			Str("handler", handlerName).
			Int("matches", len(handlerMatches)).
			Msg("Processing matches for handler")

		// Get V2 handler directly
		handler := handlers.GetHandler(handlerName)
		if handler == nil {
			logger.Warn().
				Str("handler", handlerName).
				Msg("No V2 handler found, skipping")
			continue
		}

		// Process based on handler type
		switch h := handler.(type) {
		case types.LinkingHandler:
			linkingActions, err := h.ProcessLinking(handlerMatches)
			if err != nil {
				return nil, fmt.Errorf("handler %s failed to process linking: %w", handlerName, err)
			}
			// Convert LinkingAction to Action
			for _, action := range linkingActions {
				allActions = append(allActions, action)
			}

		case types.ProvisioningHandler:
			provisioningActions, err := h.ProcessProvisioning(handlerMatches)
			if err != nil {
				return nil, fmt.Errorf("handler %s failed to process provisioning: %w", handlerName, err)
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
		Msg("Generated V2 actions from trigger matches")

	return allActions, nil
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

// FilterActionsByRunMode filters V2 actions based on their type
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
