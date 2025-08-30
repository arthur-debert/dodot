// Package confirmations provides UI implementations for confirmation dialogs.
package confirmations

import (
	"fmt"
	"sort"
	"strings"

	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/arthur-debert/dodot/pkg/ui/format"
)

// ConsoleDialog implements ConfirmationDialog for console interaction
type ConsoleDialog struct{}

// NewConsoleDialog creates a new console confirmation dialog
func NewConsoleDialog() *ConsoleDialog {
	return &ConsoleDialog{}
}

// PresentConfirmations shows confirmations to the user via console and collects responses
func (d *ConsoleDialog) PresentConfirmations(confirmations []types.ConfirmationRequest) ([]types.ConfirmationResponse, error) {
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
			emoji := format.HandlerEmoji(conf.Handler)
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
