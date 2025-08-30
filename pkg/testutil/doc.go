// Package testutil provides utilities for testing dodot components.
//
// This package follows the testing guidelines defined in docs/testing/guide.txxt
// and provides the infrastructure described in docs/testing/infrastructure-utils-setup.txxt.
//
// Key components:
//   - TestEnvironment: Core test orchestrator with isolation and cleanup
//   - MemoryFS: In-memory filesystem implementation for fast, isolated tests
//   - MockDataStore: Mock state management without filesystem operations
//   - PackBuilder: Declarative pack setup builder
//
// Usage guidelines:
//   - 95% of tests should use EnvMemoryOnly for speed and isolation
//   - Only pkg/datastore tests should use real filesystem operations
//   - All test data should be defined inline, not in external files
//   - Each test should be completely isolated with no shared state
package testutil
