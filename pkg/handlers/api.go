package handlers

import "github.com/arthur-debert/dodot/pkg/operations"

// HandlerRegistry provides a minimal API for handler categorization
// This replaces the need for RunMode throughout the codebase
var HandlerRegistry = struct {
	// IsConfigurationHandler returns true if the handler manages configuration
	IsConfigurationHandler func(handlerName string) bool

	// IsCodeExecutionHandler returns true if the handler executes arbitrary code
	IsCodeExecutionHandler func(handlerName string) bool

	// GetHandlerCategory returns the category for a handler
	GetHandlerCategory func(handlerName string) operations.HandlerCategory

	// GetConfigurationHandlers returns all configuration handler names
	GetConfigurationHandlers func() []string

	// GetCodeExecutionHandlers returns all code execution handler names
	GetCodeExecutionHandlers func() []string

	// RequiresExecutionOrdering returns true if code execution handlers should run first
	RequiresExecutionOrdering func() bool
}{
	IsConfigurationHandler: func(handlerName string) bool {
		switch handlerName {
		case "symlink", "shell", "path":
			return true
		default:
			return false
		}
	},

	IsCodeExecutionHandler: func(handlerName string) bool {
		switch handlerName {
		case "homebrew", "install":
			return true
		default:
			return false
		}
	},

	GetHandlerCategory: func(handlerName string) operations.HandlerCategory {
		switch handlerName {
		case "symlink", "shell", "path":
			return operations.CategoryConfiguration
		case "homebrew", "install":
			return operations.CategoryCodeExecution
		default:
			return operations.CategoryConfiguration // Safe default
		}
	},

	GetConfigurationHandlers: func() []string {
		return []string{"symlink", "shell", "path"}
	},

	GetCodeExecutionHandlers: func() []string {
		return []string{"homebrew", "install"}
	},

	RequiresExecutionOrdering: func() bool {
		// Code execution handlers should run before configuration handlers
		return true
	},
}
