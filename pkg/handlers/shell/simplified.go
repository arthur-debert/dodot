package shell

import (
	_ "embed"

	"github.com/arthur-debert/dodot/pkg/handlers"
	"github.com/arthur-debert/dodot/pkg/operations"
	"github.com/arthur-debert/dodot/pkg/types"
)

// SimplifiedHandler implements the new simplified handler interface.
// It transforms shell profile requests into operations without performing any I/O.
type SimplifiedHandler struct {
	operations.BaseHandler
}

// NewSimplifiedHandler creates a new simplified shell handler.
func NewSimplifiedHandler() *SimplifiedHandler {
	return &SimplifiedHandler{
		BaseHandler: operations.NewBaseHandler(ShellHandlerName, handlers.CategoryConfiguration),
	}
}

// ToOperations converts rule matches to shell operations.
// Shell scripts require only CreateDataLink - shell initialization handles sourcing.
func (h *SimplifiedHandler) ToOperations(matches []types.RuleMatch) ([]operations.Operation, error) {
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
func (h *SimplifiedHandler) GetMetadata() operations.HandlerMetadata {
	return operations.HandlerMetadata{
		Description:     "Manages shell profile modifications (e.g., sourcing aliases)",
		RequiresConfirm: false, // Shell scripts don't need confirmation
		CanRunMultiple:  true,  // Can link multiple times
	}
}

// GetTemplateContent returns the template content for this handler.
func (h *SimplifiedHandler) GetTemplateContent() string {
	return aliasesTemplate
}

// Verify interface compliance
var _ operations.Handler = (*SimplifiedHandler)(nil)
