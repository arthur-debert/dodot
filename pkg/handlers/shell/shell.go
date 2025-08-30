package shell

import (
	_ "embed"

	"github.com/arthur-debert/dodot/pkg/handlers"
	"github.com/arthur-debert/dodot/pkg/operations"
	"github.com/arthur-debert/dodot/pkg/types"
)

const ShellHandlerName = "shell"

//go:embed aliases-template.txt
var aliasesTemplate string

// Handler implements the new simplified handler interface.
// It transforms shell profile requests into operations without performing any I/O.
type Handler struct {
	operations.BaseHandler
}

// NewHandler creates a new simplified shell handler.
func NewHandler() *Handler {
	return &Handler{
		BaseHandler: operations.NewBaseHandler(ShellHandlerName, handlers.CategoryConfiguration),
	}
}

// ToOperations converts rule matches to shell operations.
// Shell scripts require only CreateDataLink - shell initialization handles sourcing.
func (h *Handler) ToOperations(matches []types.RuleMatch) ([]operations.Operation, error) {
	var ops []operations.Operation

	for _, match := range matches {
		// Shell scripts only need to be linked in the datastore
		// The shell initialization script will source them automatically
		ops = append(ops, operations.Operation{
			Type:    operations.CreateDataLink,
			Pack:    match.Pack,
			Handler: ShellHandlerName,
			Source:  match.AbsolutePath,
		})
	}

	return ops, nil
}

// GetMetadata returns handler metadata.
func (h *Handler) GetMetadata() operations.HandlerMetadata {
	return operations.HandlerMetadata{
		Description:     "Manages shell profile modifications (e.g., sourcing aliases)",
		RequiresConfirm: false, // Shell scripts don't need confirmation
		CanRunMultiple:  true,  // Can link multiple times
	}
}

// GetTemplateContent returns the template content for this handler.
func (h *Handler) GetTemplateContent() string {
	return aliasesTemplate
}

// Verify interface compliance
var _ operations.Handler = (*Handler)(nil)
