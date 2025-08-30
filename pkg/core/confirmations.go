package core

import (
	"fmt"
	"sort"

	"github.com/arthur-debert/dodot/pkg/types"
)

// ConfirmationCollector aggregates confirmation requests from multiple sources
type ConfirmationCollector struct {
	confirmations []types.ConfirmationRequest
	seenIDs       map[string]bool
}

// NewConfirmationCollector creates a new confirmation collector
func NewConfirmationCollector() *ConfirmationCollector {
	return &ConfirmationCollector{
		confirmations: make([]types.ConfirmationRequest, 0),
		seenIDs:       make(map[string]bool),
	}
}

// Add adds a confirmation request to the collector
// Returns error if the confirmation ID is already used (prevents duplicates)
func (cc *ConfirmationCollector) Add(confirmation types.ConfirmationRequest) error {
	if cc.seenIDs[confirmation.ID] {
		return fmt.Errorf("duplicate confirmation ID: %s", confirmation.ID)
	}

	cc.confirmations = append(cc.confirmations, confirmation)
	cc.seenIDs[confirmation.ID] = true
	return nil
}

// AddMultiple adds multiple confirmation requests
func (cc *ConfirmationCollector) AddMultiple(confirmations []types.ConfirmationRequest) error {
	for _, confirmation := range confirmations {
		if err := cc.Add(confirmation); err != nil {
			return err
		}
	}
	return nil
}

// GetAll returns all collected confirmations, sorted by pack then handler
func (cc *ConfirmationCollector) GetAll() []types.ConfirmationRequest {
	// Sort by pack, then handler, then operation for consistent ordering
	sorted := make([]types.ConfirmationRequest, len(cc.confirmations))
	copy(sorted, cc.confirmations)

	sort.Slice(sorted, func(i, j int) bool {
		if sorted[i].Pack != sorted[j].Pack {
			return sorted[i].Pack < sorted[j].Pack
		}
		if sorted[i].Handler != sorted[j].Handler {
			return sorted[i].Handler < sorted[j].Handler
		}
		return sorted[i].Operation < sorted[j].Operation
	})

	return sorted
}

// HasConfirmations returns true if any confirmations have been collected
func (cc *ConfirmationCollector) HasConfirmations() bool {
	return len(cc.confirmations) > 0
}

// Count returns the number of confirmations collected
func (cc *ConfirmationCollector) Count() int {
	return len(cc.confirmations)
}

// ConfirmationDialog handles the display and collection of user responses.
// This interface stays in the core package as it defines the contract that
// orchestration functions expect from UI implementations.
// Concrete implementations (like ConsoleDialog) belong in the UI layer.
type ConfirmationDialog interface {
	// PresentConfirmations shows confirmations to the user and collects responses
	PresentConfirmations(confirmations []types.ConfirmationRequest) ([]types.ConfirmationResponse, error)
}

// CollectAndProcessConfirmations is a utility function that collects confirmations
// and prompts the user, returning the confirmation context
func CollectAndProcessConfirmations(confirmations []types.ConfirmationRequest, dialog ConfirmationDialog) (*types.ConfirmationContext, error) {
	if len(confirmations) == 0 {
		// No confirmations needed
		return nil, nil
	}

	responses, err := dialog.PresentConfirmations(confirmations)
	if err != nil {
		return nil, fmt.Errorf("failed to collect confirmation responses: %w", err)
	}

	return types.NewConfirmationContext(responses), nil
}
