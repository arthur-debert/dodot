package operations

import (
	"fmt"
	"path/filepath"

	"github.com/arthur-debert/dodot/pkg/types"
)

// ActionAdapter bridges the new operation system with the existing action system.
// This is a temporary compatibility layer that will be removed in phase 3.
// The adapter demonstrates how operations map to the current action types.
type ActionAdapter struct {
	executor *Executor
}

// NewActionAdapter creates a new adapter.
func NewActionAdapter(executor *Executor) *ActionAdapter {
	return &ActionAdapter{executor: executor}
}

// OperationsToActions converts operations to the legacy action types.
// This allows us to test the operation system with existing commands.
func (a *ActionAdapter) OperationsToActions(operations []Operation) ([]types.Action, error) {
	var actions []types.Action

	for _, op := range operations {
		action, err := a.operationToAction(op)
		if err != nil {
			return nil, err
		}
		if action != nil {
			actions = append(actions, action)
		}
	}

	return actions, nil
}

// operationToAction converts a single operation to an action.
// This mapping shows how the 4 operations cover all current action types.
//
// ADAPTER COMPLEXITY NOTE:
// The symlink handler requires special handling because it generates two operations
// (CreateDataLink + CreateUserLink) that map to a single LinkAction. This is handled
// by returning nil for CreateDataLink and composing the full action at CreateUserLink.
// This approach works but will need careful review as more complex handlers are migrated.
// This adapter is intentionally temporary and will be removed in Phase 3.
func (a *ActionAdapter) operationToAction(op Operation) (types.Action, error) {
	switch op.Type {
	case CreateDataLink:
		// CreateDataLink operations map to different actions based on handler
		switch op.Handler {
		case HandlerSymlink:
			// SPECIAL CASE: Symlink requires both CreateDataLink and CreateUserLink
			// to form a complete LinkAction. We skip processing here and handle
			// the complete action when we see CreateUserLink.
			// TODO: Consider tracking operation pairs for better error handling
			return nil, nil // Skip, will be handled with CreateUserLink

		case HandlerPath:
			return &types.AddToPathAction{
				PackName: op.Pack,
				DirPath:  op.Source,
			}, nil

		case HandlerShell, HandlerShellProfile:
			return &types.AddToShellProfileAction{
				PackName:   op.Pack,
				ScriptPath: op.Source,
			}, nil

		default:
			return nil, fmt.Errorf("unknown handler for CreateDataLink: %s", op.Handler)
		}

	case CreateUserLink:
		// SPECIAL CASE CONTINUED: This assumes a previous CreateDataLink operation
		// for the symlink handler. In the real implementation, the adapter would
		// need to track the datastore path from the CreateDataLink operation.
		// This complexity is acceptable for a temporary adapter but highlights
		// why the operation-based approach is cleaner - handlers can generate
		// exactly the operations they need without this mapping complexity.
		return &types.LinkAction{
			PackName:   op.Pack,
			SourceFile: filepath.Base(op.Source), // Simplified for adapter
			TargetFile: op.Target,
		}, nil

	case RunCommand:
		// RunCommand operations map to provisioning actions
		switch op.Handler {
		case HandlerInstall:
			return &types.RunScriptAction{
				PackName:     op.Pack,
				ScriptPath:   extractScriptPath(op.Command),
				SentinelName: op.Sentinel,
				Checksum:     op.Metadata["checksum"].(string),
			}, nil

		case HandlerHomebrew:
			return &types.BrewAction{
				PackName:     op.Pack,
				BrewfilePath: op.Metadata["brewfile"].(string),
				Checksum:     op.Metadata["checksum"].(string),
			}, nil

		default:
			return nil, fmt.Errorf("unknown handler for RunCommand: %s", op.Handler)
		}

	case CheckSentinel:
		// CheckSentinel doesn't map to actions - it's a query operation
		return nil, nil

	default:
		return nil, fmt.Errorf("unknown operation type: %v", op.Type)
	}
}

// ActionsToOperations converts legacy actions to operations.
// This is used when existing commands generate actions that need to be executed.
func (a *ActionAdapter) ActionsToOperations(actions []types.Action) ([]Operation, error) {
	var operations []Operation

	for _, action := range actions {
		ops, err := a.actionToOperations(action)
		if err != nil {
			return nil, err
		}
		operations = append(operations, ops...)
	}

	return operations, nil
}

// actionToOperations converts a single action to operations.
// Note how each action type maps to 1-2 operations.
func (a *ActionAdapter) actionToOperations(action types.Action) ([]Operation, error) {
	switch act := action.(type) {
	case *types.LinkAction:
		// LinkAction requires two operations:
		// 1. Create link in datastore
		// 2. Create user-visible link
		return []Operation{
			{
				Type:    CreateDataLink,
				Pack:    act.PackName,
				Handler: HandlerSymlink,
				Source:  act.SourceFile,
			},
			{
				Type:    CreateUserLink,
				Pack:    act.PackName,
				Handler: HandlerSymlink,
				Source:  act.SourceFile, // Will be resolved to datastore path
				Target:  act.TargetFile,
			},
		}, nil

	case *types.AddToPathAction:
		// Path action only needs datastore link
		// Shell init will handle PATH management
		return []Operation{
			{
				Type:    CreateDataLink,
				Pack:    act.PackName,
				Handler: HandlerPath,
				Source:  act.DirPath,
			},
		}, nil

	case *types.AddToShellProfileAction:
		// Shell profile action only needs datastore link
		// Shell init will handle sourcing
		return []Operation{
			{
				Type:    CreateDataLink,
				Pack:    act.PackName,
				Handler: HandlerShellProfile,
				Source:  act.ScriptPath,
			},
		}, nil

	case *types.RunScriptAction:
		// Script execution maps directly to RunCommand
		return []Operation{
			{
				Type:     RunCommand,
				Pack:     act.PackName,
				Handler:  HandlerInstall,
				Command:  act.ScriptPath,
				Sentinel: act.SentinelName,
				Metadata: map[string]interface{}{
					"checksum": act.Checksum,
				},
			},
		}, nil

	case *types.BrewAction:
		// Homebrew execution maps to RunCommand
		return []Operation{
			{
				Type:     RunCommand,
				Pack:     act.PackName,
				Handler:  HandlerHomebrew,
				Command:  fmt.Sprintf("brew bundle --file=%s", act.BrewfilePath),
				Sentinel: fmt.Sprintf("brewfile-%s", act.Checksum),
				Metadata: map[string]interface{}{
					"brewfile": act.BrewfilePath,
					"checksum": act.Checksum,
				},
			},
		}, nil

	default:
		return nil, fmt.Errorf("unknown action type: %T", action)
	}
}

// extractScriptPath is a helper to parse script path from command.
// In real implementation, this would be more robust.
func extractScriptPath(command string) string {
	// Simple implementation - just return the command
	// Real implementation would parse properly
	return command
}
