package types

// HandlerType represents the fundamental nature of a handler's operations
type HandlerType string

const (
	// HandlerTypeConfiguration indicates handlers that manage configuration files/links
	// These are safe to run repeatedly without side effects
	HandlerTypeConfiguration HandlerType = "configuration"

	// HandlerTypeCodeExecution indicates handlers that execute arbitrary code/scripts
	// These require user consent for repeated execution
	HandlerTypeCodeExecution HandlerType = "code_execution"
)
