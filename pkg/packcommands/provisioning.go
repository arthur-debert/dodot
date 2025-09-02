package packcommands

import (
	"fmt"

	"github.com/arthur-debert/dodot/pkg/datastore"
	"github.com/arthur-debert/dodot/pkg/types"
)

// IsPackHandlerProvisioned checks if a specific handler has been provisioned for this pack.
// This is a business logic function that uses the DataStore's query capabilities.
func IsPackHandlerProvisioned(pack *types.Pack, store datastore.DataStore, handlerName string) (bool, error) {
	return store.HasHandlerState(pack.Name, handlerName)
}

// GetPackProvisionedHandlers returns a list of all handlers that have been provisioned for this pack.
// This helps identify which handlers have already been executed.
func GetPackProvisionedHandlers(pack *types.Pack, store datastore.DataStore) ([]string, error) {
	handlers, err := store.ListPackHandlers(pack.Name)
	if err != nil {
		return nil, fmt.Errorf("failed to list provisioned handlers for pack %s: %w", pack.Name, err)
	}

	// Filter to only include handlers that actually have state
	var provisionedHandlers []string
	for _, handler := range handlers {
		hasState, err := store.HasHandlerState(pack.Name, handler)
		if err != nil {
			return nil, fmt.Errorf("failed to check handler state for %s/%s: %w", pack.Name, handler, err)
		}
		if hasState {
			provisionedHandlers = append(provisionedHandlers, handler)
		}
	}

	return provisionedHandlers, nil
}
