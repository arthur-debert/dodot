package core

import (
	"fmt"
	"sort"
	"strings"

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

// ConfirmationDialog handles the display and collection of user responses
type ConfirmationDialog interface {
	// PresentConfirmations shows confirmations to the user and collects responses
	PresentConfirmations(confirmations []types.ConfirmationRequest) ([]types.ConfirmationResponse, error)
}

// ConsoleConfirmationDialog implements ConfirmationDialog for console interaction
type ConsoleConfirmationDialog struct{}

// NewConsoleConfirmationDialog creates a new console confirmation dialog
func NewConsoleConfirmationDialog() *ConsoleConfirmationDialog {
	return &ConsoleConfirmationDialog{}
}

// PresentConfirmations shows confirmations to the user via console and collects responses
func (d *ConsoleConfirmationDialog) PresentConfirmations(confirmations []types.ConfirmationRequest) ([]types.ConfirmationResponse, error) {
	if len(confirmations) == 0 {
		return []types.ConfirmationResponse{}, nil
	}

	fmt.Println("\nThe following operations require confirmation:")
	fmt.Println()

	// Group confirmations by pack for better display
	packGroups := make(map[string][]types.ConfirmationRequest)
	for _, conf := range confirmations {
		packGroups[conf.Pack] = append(packGroups[conf.Pack], conf)
	}

	// Sort pack names for consistent display
	var packNames []string
	for pack := range packGroups {
		packNames = append(packNames, pack)
	}
	sort.Strings(packNames)

	// Display grouped confirmations
	for _, pack := range packNames {
		fmt.Printf("📦 Pack: %s\n", pack)
		for _, conf := range packGroups[pack] {
			emoji := getHandlerEmoji(conf.Handler)
			fmt.Printf("└── %s %s (%s)\n", emoji, conf.Handler, conf.Title)
			if len(conf.Items) > 0 {
				if len(conf.Items) <= 3 {
					fmt.Printf("    └── %s\n", strings.Join(conf.Items, ", "))
				} else {
					fmt.Printf("    └── %s and %d more\n", strings.Join(conf.Items[:3], ", "), len(conf.Items)-3)
				}
			}
			if conf.Description != "" {
				fmt.Printf("    └── %s\n", conf.Description)
			}
		}
		fmt.Println()
	}

	// Ask for overall confirmation first
	fmt.Print("Continue with these operations? [y/N]: ")
	var overallResponse string
	_, err := fmt.Scanln(&overallResponse)
	if err != nil && err.Error() != "unexpected newline" {
		return nil, fmt.Errorf("failed to read user input: %w", err)
	}

	overallResponse = strings.ToLower(strings.TrimSpace(overallResponse))
	if overallResponse != "y" && overallResponse != "yes" {
		// User declined overall, return all as not approved
		responses := make([]types.ConfirmationResponse, len(confirmations))
		for i, conf := range confirmations {
			responses[i] = types.ConfirmationResponse{
				ID:       conf.ID,
				Approved: false,
			}
		}
		return responses, nil
	}

	fmt.Println()
	fmt.Println("Detailed confirmations:")

	// Collect individual confirmations
	responses := make([]types.ConfirmationResponse, 0, len(confirmations))
	for i, conf := range confirmations {
		defaultMarker := "[y/N]"
		if conf.Default {
			defaultMarker = "[Y/n]"
		}

		fmt.Printf("%d. %s %s: ", i+1, conf.Description, defaultMarker)

		var response string
		_, err := fmt.Scanln(&response)
		if err != nil && err.Error() != "unexpected newline" {
			return nil, fmt.Errorf("failed to read user input for confirmation %s: %w", conf.ID, err)
		}

		response = strings.ToLower(strings.TrimSpace(response))

		var approved bool
		if response == "" {
			// Use default
			approved = conf.Default
		} else {
			approved = response == "y" || response == "yes"
		}

		responses = append(responses, types.ConfirmationResponse{
			ID:       conf.ID,
			Approved: approved,
		})
	}

	return responses, nil
}

// getHandlerEmoji returns an emoji for the given handler type
func getHandlerEmoji(handler string) string {
	switch strings.ToLower(handler) {
	case "homebrew":
		return "🍺"
	case "symlink":
		return "🔗"
	case "provision":
		return "🔧"
	case "shell_profile":
		return "🐚"
	case "path":
		return "📁"
	default:
		return "⚙️"
	}
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
