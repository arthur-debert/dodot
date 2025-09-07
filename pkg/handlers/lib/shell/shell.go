package shell

import (
	_ "embed"

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
		BaseHandler: operations.NewBaseHandler(ShellHandlerName, operations.CategoryConfiguration),
	}
}

// ToOperations converts file inputs to shell operations.
// Shell scripts require only CreateDataLink - shell initialization handles sourcing.
func (h *Handler) ToOperations(files []operations.FileInput, config interface{}) ([]operations.Operation, error) {
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

// CheckStatus checks if the shell config file has been linked
func (h *Handler) CheckStatus(file operations.FileInput, checker operations.StatusChecker) (operations.HandlerStatus, error) {
	// Check if the data link exists in the datastore
	exists, err := checker.HasDataLink(file.PackName, h.Name(), file.RelativePath)
	if err != nil {
		return operations.HandlerStatus{
			State:   operations.StatusStateError,
			Message: "Failed to check shell config status",
		}, err
	}

	if exists {
		// Shell config is linked and will be sourced
		return operations.HandlerStatus{
			State:   operations.StatusStateReady,
			Message: "sourced in shell",
		}, nil
	}

	// Shell config not linked
	return operations.HandlerStatus{
		State:   operations.StatusStatePending,
		Message: "not sourced in shell",
	}, nil
}

// Verify interface compliance
var _ operations.Handler = (*Handler)(nil)
