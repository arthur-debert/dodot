# Operations Package - Phase 1 Implementation

This package implements the simplified handler architecture proposed in issue #669.

## Overview

The operations package introduces a new architecture where:
- Handlers are simple data transformers (50-100 lines instead of 200-300+)
- DataStore has only 4 operations instead of 20+ methods
- Complex orchestration is centralized in the operation executor

## Current Status (Phase 1)

✅ Implemented:
- Core operation types and interfaces
- Operation executor with dry-run support
- Adapter to bridge with existing action system
- DataStore adapter using existing methods
- Simplified path handler as proof of concept
- Feature flag system (DODOT_USE_OPERATIONS=true)

🚧 Not Yet Implemented:
- Other handlers (symlink, shell, install, homebrew)
- Full clear/uninstall support
- Shell integration changes
- Direct datastore implementation

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

## Next Steps

Phase 2: Migrate remaining handlers
Phase 3: Simplify DataStore interface and remove adapters