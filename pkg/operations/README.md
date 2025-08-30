# Operations Package - Phase 1 Implementation

This package implements the simplified handler architecture proposed in issue #669.

## Overview

The operations package introduces a new architecture where:
- Handlers are simple data transformers (50-100 lines instead of 200-300+)
- DataStore has only 4 operations instead of 20+ methods
- Complex orchestration is centralized in the operation executor

## Phase 1 Status ✅ COMPLETED

### Implemented:
- Core operation types and interfaces
- Operation executor with dry-run support
- Adapter to bridge with existing action system
- DataStore adapter using existing methods
- Simplified path handler as proof of concept (185→40 lines, 78% reduction)
- Feature flag system (DODOT_USE_OPERATIONS=true)
- Integration with link/provision commands
- Clear/uninstall support for simplified handlers
- Comprehensive tests for all components

### Demonstrated Results:
- Path handler reduced from 185 to 40 lines (78% reduction)
- All tests passing
- Commands work with feature flag enabled
- Clear functionality operational

## Usage

To test the new system:

```bash
# Enable the operations system
export DODOT_USE_OPERATIONS=true

# Run commands normally - path handler will use new system
dodot link mypack
```

## Architecture

```
Commands
    ↓
Handlers (simple transformers)
    ↓
Operations (4 types)
    ↓
Executor (orchestration)
    ↓
SimpleDataStore (4 methods)
```

## Testing

Run tests with:
```bash
go test ./pkg/operations/...
go test ./pkg/handlers/path/simplified_test.go
```

## Phase 2 Status 🚧 IN PROGRESS

### Objectives:
- Migrate all remaining handlers to simplified architecture
- Each handler should be reduced to ~50-100 lines (data transformation only)
- Maintain backward compatibility through adapters
- Demonstrate consistent 70-80% code reduction across all handlers

### Migration Progress:
1. ✅ **symlink** - 315→113 lines (64% reduction) - Two-operation pattern working
2. ✅ **shell** - 150→56 lines (63% reduction) - Single CreateDataLink pattern
3. ✅ **install** - 241→83 lines (66% reduction) - RunCommand + CheckSentinel pattern
4. 🚧 **homebrew** - Complex provisioning with external tool integration

### Success Criteria:
- All handlers work with DODOT_USE_OPERATIONS=true
- Each handler is <100 lines of code
- All existing tests pass
- Clear functionality works for all handlers
- Integration tests demonstrate end-to-end functionality

## Phase 3 (Future)

- Replace DataStore with SimpleDataStore interface (4 methods only)
- Remove all adapters and legacy code
- Implement generic state management
- Full architectural simplification complete