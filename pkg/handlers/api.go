package handlers

// HandlerCategory represents the fundamental nature of a handler's operations
type HandlerCategory string

const (
	// CategoryConfiguration handlers manage configuration files/links
	// These are safe to run repeatedly without side effects
	CategoryConfiguration HandlerCategory = "configuration"

	// CategoryCodeExecution handlers run arbitrary code/scripts
	// These require user consent for repeated execution
	CategoryCodeExecution HandlerCategory = "code_execution"
)

// HandlerRegistry provides a minimal API for handler categorization
// This replaces the need for RunMode throughout the codebase
var HandlerRegistry = struct {
	// IsConfigurationHandler returns true if the handler manages configuration
	IsConfigurationHandler func(handlerName string) bool

	// IsCodeExecutionHandler returns true if the handler executes arbitrary code
	IsCodeExecutionHandler func(handlerName string) bool

	// GetHandlerCategory returns the category for a handler
	GetHandlerCategory func(handlerName string) HandlerCategory

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

	GetHandlerCategory: func(handlerName string) HandlerCategory {
		switch handlerName {
		case "symlink", "shell", "path":
			return CategoryConfiguration
		case "homebrew", "install":
			return CategoryCodeExecution
		default:
			return CategoryConfiguration // Safe default
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

// Migration helpers to ease transition from RunMode

// IsLinkingHandler checks if a handler is safe for repeated execution (configuration)
// Deprecated: Use HandlerRegistry.IsConfigurationHandler
func IsLinkingHandler(handlerName string) bool {
	return HandlerRegistry.IsConfigurationHandler(handlerName)
}

// IsProvisioningHandler checks if a handler executes arbitrary code
// Deprecated: Use HandlerRegistry.IsCodeExecutionHandler
func IsProvisioningHandler(handlerName string) bool {
	return HandlerRegistry.IsCodeExecutionHandler(handlerName)
}
