package shell

import (
	_ "embed"

	"github.com/arthur-debert/dodot/pkg/handlers"
	"github.com/arthur-debert/dodot/pkg/operations"
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

// ToOperations converts file inputs to shell operations.
// Shell scripts require only CreateDataLink - shell initialization handles sourcing.
func (h *Handler) ToOperations(files []operations.FileInput) ([]operations.Operation, error) {
	var ops []operations.Operation

	for _, file := range files {
		// Shell scripts only need to be linked in the datastore
		// The shell initialization script will source them automatically
		ops = append(ops, operations.Operation{
			Type:    operations.CreateDataLink,
			Pack:    file.PackName,
			Handler: ShellHandlerName,
			Source:  file.SourcePath,
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
