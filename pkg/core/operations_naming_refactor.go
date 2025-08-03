package core

// This file contains naming improvements to clarify the distinction between
// converting actions to operations (planning) vs executing operations (doing).
//
// PROPOSED REFACTORING:
//
// Current naming that's confusing:
// - GetFileOperations() -> sounds like fetching/retrieving, not converting
// - GetFileOperationsWithContext() -> same issue
//
// Proposed new names:
// - ConvertActionsToOperations() -> clearly indicates conversion/transformation
// - ConvertActionsToOperationsWithContext() -> same with context
//
// Alternative names considered:
// - GenerateOperationsFromActions()
// - PlanOperationsFromActions()
// - TransformActionsToOperations()
//
// The word "Convert" is already used for single actions (ConvertAction)
// so using it for multiple actions maintains consistency.
//
// For execution side, the naming is already clear:
// - ExecuteOperations() -> clearly indicates execution
// - NewCombinedExecutor() -> creates an executor
// - NewCommandExecutor() -> creates a command executor
//
// IMPLEMENTATION PLAN:
// 1. Add new functions with clear names as aliases
// 2. Mark old functions as deprecated
// 3. Update all callers to use new names
// 4. Remove old functions in next major version

// ConvertActionsToOperations converts actions into file system operations
// This is the planning phase - no actual file system changes are made.
func ConvertActionsToOperations(actions []types.Action) ([]types.Operation, error) {
	return GetFileOperations(actions)
}

// ConvertActionsToOperationsWithContext converts actions into file system operations with execution context
// This is the planning phase - no actual file system changes are made.
func ConvertActionsToOperationsWithContext(actions []types.Action, ctx *ExecutionContext) ([]types.Operation, error) {
	return GetFileOperationsWithContext(actions, ctx)
}