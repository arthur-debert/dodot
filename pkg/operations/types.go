package operations

import (
	"github.com/arthur-debert/dodot/pkg/types"
)

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

// FileInput represents the minimal information handlers need about a file.
// This decouples handlers from the matching/rules system.
type FileInput struct {
	PackName     string                 // Name of the pack containing this file
	SourcePath   string                 // Absolute path to the file
	RelativePath string                 // Path relative to pack root
	Options      map[string]interface{} // Handler-specific options from rules
}

// OperationType represents the fundamental operations that dodot performs.
// This is the core insight: dodot only does 4 things, everything else is orchestration.
type OperationType int

const (
	// CreateDataLink creates a link in the datastore pointing to a source file.
	// This is used by symlink, path, and shell handlers to stage files.
	CreateDataLink OperationType = iota

	// CreateUserLink creates a user-visible symlink pointing to the datastore.
	// This is the final step for symlink handler to make files accessible.
	CreateUserLink

	// RunCommand executes a command and records completion with a sentinel.
	// This is used by install and homebrew handlers for provisioning.
	RunCommand

	// CheckSentinel queries if an operation has been completed.
	// This prevents re-running expensive operations.
	CheckSentinel
)

// Operation represents a single atomic unit of work to be performed.
// Operations are the bridge between handlers (which understand file patterns)
// and the datastore (which only knows how to perform these 4 operations).
type Operation struct {
	Type     OperationType
	Pack     string
	Handler  string
	Source   string                 // For link operations: source file path
	Target   string                 // For link operations: target path
	Command  string                 // For RunCommand: command to execute
	Sentinel string                 // For RunCommand/CheckSentinel: completion marker
	Metadata map[string]interface{} // Handler-specific data for customization
}

// OperationResult captures the outcome of executing an operation.
// This is used for status reporting and dry-run output.
type OperationResult struct {
	Operation Operation
	Success   bool
	Message   string
	Error     error
}

// HandlerMetadata provides UI/UX information about a handler.
// This separates presentation concerns from operation logic.
type HandlerMetadata struct {
	Description     string // Human-readable description
	RequiresConfirm bool   // Whether operations need user confirmation
	CanRunMultiple  bool   // Whether handler supports multiple executions
}

// Handler is the simplified interface that all handlers implement.
// The key insight: handlers are just data transformers, not orchestrators.
type Handler interface {
	// Core identification
	Name() string
	Category() HandlerCategory

	// Core responsibility: transform file inputs to operations
	// This is the heart of the simplification - handlers just declare
	// what operations they need, not how to perform them.
	// The config parameter provides merged configuration for the current pack.
	ToOperations(files []FileInput, config interface{}) ([]Operation, error)

	// Status checking: handlers know how to check their own status
	CheckStatus(file FileInput, checker StatusChecker) (HandlerStatus, error)

	// Metadata for UI/UX
	GetMetadata() HandlerMetadata

	// Optional customization points with sensible defaults.
	// Most handlers won't need to implement these.
	GetClearConfirmation(ctx ClearContext) *ConfirmationRequest
	FormatClearedItem(item ClearedItem, dryRun bool) string
	ValidateOperations(ops []Operation) error
	GetStateDirectoryName() string
}

// ConfirmationRequest represents a request for user confirmation.
// This allows handlers like homebrew to customize their confirmation flow.
type ConfirmationRequest struct {
	ID          string   // Unique identifier for the confirmation
	Title       string   // Question to ask the user
	Description string   // Additional context
	Items       []string // List of items affected (e.g., packages to uninstall)
}

// ClearedItem represents something that was removed during a clear operation
type ClearedItem struct {
	Type        string // "symlink", "brew_package", "script_output", etc.
	Path        string // What was removed/affected
	Description string // Human-readable description
}

// ClearContext provides all the resources needed for a handler to clean up
type ClearContext struct {
	Pack   types.Pack   // The pack being cleared
	FS     types.FS     // For file operations
	Paths  types.Pather // For path resolution
	DryRun bool         // Whether this is a dry run
}

// StatusState represents the state of a handler's operations
type StatusState string

const (
	StatusStatePending StatusState = "pending" // Operations not yet applied
	StatusStateReady   StatusState = "ready"   // Operations successfully applied
	StatusStateError   StatusState = "error"   // Error checking or applying operations
	StatusStateUnknown StatusState = "unknown" // Status cannot be determined
)

// HandlerStatus represents the status of a handler's operations for a file
type HandlerStatus struct {
	State   StatusState // Current state of the handler's operations
	Message string      // Human-readable status message
	Details interface{} // Optional handler-specific details
}

// StatusChecker provides an interface for handlers to check their status
// without direct filesystem access or datastore implementation knowledge
type StatusChecker interface {
	// HasDataLink checks if a data link exists in the datastore
	// Used by configuration handlers (symlink, shell, path)
	HasDataLink(packName, handlerName, relativePath string) (bool, error)

	// HasSentinel checks if a sentinel exists for tracking operation completion
	// Used by code execution handlers (install, homebrew)
	HasSentinel(packName, handlerName, sentinel string) (bool, error)

	// GetMetadata retrieves metadata for future extensibility
	GetMetadata(packName, handlerName, key string) (string, error)
}

// BaseHandler provides default implementations for optional handler methods.
// This is crucial for keeping handlers simple - they only override what they need.
type BaseHandler struct {
	name     string
	category HandlerCategory
}

// NewBaseHandler creates a new BaseHandler with the given name and category.
func NewBaseHandler(name string, category HandlerCategory) BaseHandler {
	return BaseHandler{
		name:     name,
		category: category,
	}
}

func (h *BaseHandler) Name() string              { return h.name }
func (h *BaseHandler) Category() HandlerCategory { return h.category }

// Default implementations return empty/nil to use system defaults
func (h *BaseHandler) GetClearConfirmation(ctx ClearContext) *ConfirmationRequest {
	return nil
}
func (h *BaseHandler) FormatClearedItem(item ClearedItem, dryRun bool) string { return "" }
func (h *BaseHandler) ValidateOperations(ops []Operation) error               { return nil }
func (h *BaseHandler) GetStateDirectoryName() string                          { return "" }

// CheckStatus provides a default implementation that returns unknown status
// Handlers should override this to provide specific status checking logic
func (h *BaseHandler) CheckStatus(file FileInput, checker StatusChecker) (HandlerStatus, error) {
	return HandlerStatus{
		State:   StatusStateUnknown,
		Message: "Status checking not implemented for this handler",
	}, nil
}
