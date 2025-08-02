package template

import (
	"fmt"
	"os"
	"path/filepath"
	"strings"

	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/registry"
	"github.com/arthur-debert/dodot/pkg/types"
)

const (
	// TemplatePowerUpName is the unique name for the template power-up
	TemplatePowerUpName = "template"

	// TemplatePowerUpPriority is the priority for template operations
	TemplatePowerUpPriority = 70
)

// TemplatePowerUp processes template files and expands variables
type TemplatePowerUp struct {
	defaultTarget string
	variables     map[string]string
}

// NewTemplatePowerUp creates a new TemplatePowerUp
func NewTemplatePowerUp() *TemplatePowerUp {
	// Default variables
	vars := make(map[string]string)

	// Add environment variables
	vars["HOME"] = os.Getenv("HOME")
	vars["USER"] = os.Getenv("USER")
	vars["SHELL"] = os.Getenv("SHELL")

	// Add hostname
	hostname, _ := os.Hostname()
	vars["HOSTNAME"] = hostname

	return &TemplatePowerUp{
		defaultTarget: "~",
		variables:     vars,
	}
}

// Name returns the unique name of this power-up
func (p *TemplatePowerUp) Name() string {
	return TemplatePowerUpName
}

// Description returns a human-readable description
func (p *TemplatePowerUp) Description() string {
	return "Processes template files with variable substitution"
}

// RunMode returns when this power-up should run
func (p *TemplatePowerUp) RunMode() types.RunMode {
	return types.RunModeMany
}

// Process takes template files and generates processed file actions
func (p *TemplatePowerUp) Process(matches []types.TriggerMatch) ([]types.Action, error) {
	logger := logging.GetLogger("powerups.template")
	actions := make([]types.Action, 0, len(matches))

	// Get target directory from options or use default
	targetDir := p.defaultTarget
	if len(matches) > 0 && matches[0].PowerUpOptions != nil {
		if target, ok := matches[0].PowerUpOptions["target"].(string); ok {
			targetDir = target
		}
	}

	// Merge additional variables from options
	variables := make(map[string]string)
	for k, v := range p.variables {
		variables[k] = v
	}
	if len(matches) > 0 && matches[0].PowerUpOptions != nil {
		if vars, ok := matches[0].PowerUpOptions["variables"].(map[string]interface{}); ok {
			for k, v := range vars {
				if strVal, ok := v.(string); ok {
					variables[k] = strVal
				}
			}
		}
	}

	for _, match := range matches {
		// Remove .tmpl extension if present
		filename := filepath.Base(match.Path)
		filename = strings.TrimSuffix(filename, ".tmpl")

		targetPath := filepath.Join(targetDir, filename)

		// Create template processing action
		action := types.Action{
			Type:        types.ActionTypeTemplate,
			Description: fmt.Sprintf("Process template %s -> %s", match.Path, targetPath),
			Source:      match.AbsolutePath,
			Target:      targetPath,
			Pack:        match.Pack,
			PowerUpName: p.Name(),
			Priority:    TemplatePowerUpPriority,
			Metadata: map[string]interface{}{
				"trigger":   match.TriggerName,
				"variables": variables,
			},
		}

		actions = append(actions, action)

		logger.Debug().
			Str("source", match.AbsolutePath).
			Str("target", targetPath).
			Str("pack", match.Pack).
			Int("variables", len(variables)).
			Msg("generated template action")
	}

	logger.Info().
		Int("match_count", len(matches)).
		Int("action_count", len(actions)).
		Msg("processed template matches")

	return actions, nil
}

// ValidateOptions checks if the provided options are valid
func (p *TemplatePowerUp) ValidateOptions(options map[string]interface{}) error {
	if options == nil {
		return nil
	}

	// Check target option if provided
	if target, exists := options["target"]; exists {
		if _, ok := target.(string); !ok {
			return fmt.Errorf("target option must be a string, got %T", target)
		}
	}

	// Check variables option if provided
	if variables, exists := options["variables"]; exists {
		if varsMap, ok := variables.(map[string]interface{}); ok {
			// Validate that all variable values are strings
			for k, v := range varsMap {
				if _, ok := v.(string); !ok {
					return fmt.Errorf("variable %s must be a string, got %T", k, v)
				}
			}
		} else {
			return fmt.Errorf("variables option must be a map, got %T", variables)
		}
	}

	// Check for unknown options
	for key := range options {
		if key != "target" && key != "variables" {
			return fmt.Errorf("unknown option: %s", key)
		}
	}

	return nil
}

// GetTemplateContent returns the template content for this power-up
func (p *TemplatePowerUp) GetTemplateContent() string {
	return ""
}

func init() {
	// Register the factory
	err := registry.RegisterPowerUpFactory(TemplatePowerUpName, func(config map[string]interface{}) (types.PowerUp, error) {
		return NewTemplatePowerUp(), nil
	})
	if err != nil {
		panic(fmt.Sprintf("failed to register %s power-up: %v", TemplatePowerUpName, err))
	}

	// Default matchers will be registered separately to avoid import cycles
}
