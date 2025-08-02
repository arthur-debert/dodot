// Package operations provides functionality for generating, managing, and
// resolving filesystem operations in the dodot system.
//
// This package handles the conversion of high-level actions into concrete
// filesystem operations, along with conflict detection and resolution.
// The operations package is responsible for:
//
//   - Operation generation from actions
//   - Conflict detection and resolution between operations
//   - Operation deduplication and optimization
//   - Utility functions for operation management
//
// The package implements the operation layer of the dodot pipeline,
// sitting between action generation and filesystem execution.
package operations
