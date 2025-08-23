package shell_profile

import (
	_ "embed"

	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/types"
)

// ShellProfileHandlerName is the name of the shell profile handler
const ShellProfileHandlerName = "shell_profile"

// aliasesTemplate is the template content for aliases.sh
const aliasesTemplate = `#!/usr/bin/env sh
# Shell aliases for PACK_NAME pack
#
# This file is sourced to add shell aliases during 'dodot deploy PACK_NAME'
# 
# Use standard shell alias syntax (compatible with bash/zsh/fish/etc)
# dodot handles shell compatibility automatically
#
# Safe to keep empty or remove. By keeping it, you can add
# aliases later without redeploying the pack.

# Add aliases below
# Examples:
# alias ll='ls -la'
# alias grep='grep --color=auto'
`

// ShellProfileHandlerV2 manages shell profile modifications
type ShellProfileHandlerV2 struct{}

// NewShellProfileHandlerV2 creates a new instance of the ShellProfileHandlerV2
func NewShellProfileHandlerV2() *ShellProfileHandlerV2 {
	return &ShellProfileHandlerV2{}
}

// Name returns the unique name of this handler
func (h *ShellProfileHandlerV2) Name() string {
	return ShellProfileHandlerName
}

// Description returns a human-readable description of what this handler does
func (h *ShellProfileHandlerV2) Description() string {
	return "Manages shell profile modifications (e.g., sourcing aliases)"
}

// RunMode returns when this handler should run
func (h *ShellProfileHandlerV2) RunMode() types.RunMode {
	return types.RunModeLinking
}

// ProcessLinking takes shell script files and creates AddToShellProfileAction instances
func (h *ShellProfileHandlerV2) ProcessLinking(matches []types.TriggerMatch) ([]types.LinkingAction, error) {
	logger := logging.GetLogger("handlers.shell_profile.v2")
	actions := make([]types.LinkingAction, 0, len(matches))

	for _, match := range matches {
		logger.Debug().
			Str("path", match.Path).
			Str("pack", match.Pack).
			Msg("Processing shell profile file")

		// Create AddToShellProfileAction
		action := &types.AddToShellProfileAction{
			PackName:   match.Pack,
			ScriptPath: match.AbsolutePath,
		}

		actions = append(actions, action)
	}

	logger.Info().
		Int("match_count", len(matches)).
		Int("action_count", len(actions)).
		Msg("processed shell profile matches")

	return actions, nil
}

// ValidateOptions checks if the provided options are valid
func (h *ShellProfileHandlerV2) ValidateOptions(options map[string]interface{}) error {
	return nil // No options to validate yet
}

// GetTemplateContent returns the template content for this handler
func (h *ShellProfileHandlerV2) GetTemplateContent() string {
	return aliasesTemplate
}

// Verify interface compliance
var _ types.LinkingHandler = (*ShellProfileHandlerV2)(nil)
