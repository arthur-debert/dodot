# Dependency Analysis Report

## Current Import Relationships

### 1. Package Dependency Graph

```
pkg/execution (leaf - no imports)
    ↑
pkg/types (imports: config, execution)
    ↑
    ├── pkg/datastore (imports: types, paths, filesystem)
    ├── pkg/handlers (imports: operations, types)
    │   └── pkg/handlers/lib/* (imports: handlers, operations)
    ├── pkg/operations (imports: types, handlers, datastore, logging)
    └── pkg/packs (imports: types, handlers, operations, datastore, core, ...)
        └── pkg/packs/commands (imports: types, datastore, handlers/pipeline)
        └── pkg/packs/pipeline (imports: types, core, filesystem)
```

### 2. Import Statistics
- **pkg/types**: Imported by 42 files (foundational package)
- **pkg/operations**: Imported by 7 files (mainly handlers)
- **pkg/handlers**: Imported by 14 files 
- **pkg/datastore**: Imported by 11 files

### 3. Current Dependency Issues

#### A. Near-Cycle: operations ↔ handlers
- **pkg/operations** imports **pkg/handlers** (for HandlerCategory type)
- **pkg/handlers/lib/*** import **pkg/operations** (for Handler interface and BaseHandler)
- This creates a mutual dependency that makes the packages tightly coupled

#### B. Problematic Import: pkg/types → pkg/execution
- **pkg/types/execution_context.go** imports **pkg/execution** for status constants (ExecutionStatus, OperationStatus)
- This creates an upward dependency from a foundational package
- However, pkg/execution is clean (no imports), so this is not a cycle

### 4. Proposed Refactoring Impact

The proposed reorganization would likely create cycles:

1. **If operations moves under handlers/**:
   - handlers/operations would import handlers (for category)
   - handlers/lib/* would import handlers/operations
   - This is acceptable as they're in the same package tree

2. **If packs absorbs operations**:
   - packs would need to import handlers (for categories)
   - handlers would import packs (for operations)
   - **This would create a cycle!**

### 5. Packages That Must Remain at Bottom of Dependency Tree

1. **pkg/execution** - Currently has no dependencies, imported by types
2. **pkg/types** - Most widely imported (42 files)
3. **pkg/filesystem** - Low-level filesystem abstraction
4. **pkg/paths** - Path utilities

### 6. Recommendations

#### A. Fix the Near-Cycle
Move `HandlerCategory` from `pkg/handlers` to either:
- `pkg/types` (if it's a core type)
- `pkg/operations` (if it's operation-specific)

#### B. Fix pkg/types Dependencies
- Move `ExecutionContext` from `pkg/execution` to `pkg/types`
- Or create a separate `pkg/context` package

#### C. Proposed Clean Structure
```
pkg/
├── types/           # Core types, no pkg/ dependencies
├── execution/       # Execution status types, no dependencies
├── filesystem/      # FS abstraction, minimal dependencies
├── paths/           # Path utilities
├── datastore/       # Storage layer (imports: types, filesystem, paths)
├── handlers/        # Handler definitions and operations
│   ├── operations/  # Operation types (imports: types)
│   ├── lib/         # Handler implementations (imports: operations)
│   └── pipeline/    # Handler execution (imports: operations, datastore)
└── packs/           # Pack management
    ├── commands/    # Pack commands (imports: handlers, datastore)
    └── pipeline/    # Pack pipeline (imports: handlers/pipeline)
```

### 7. Critical Constraints

1. **pkg/types** must minimize imports from other pkg/ packages
2. **pkg/operations** should not import concrete handlers, only interfaces/types
3. **pkg/datastore** should remain independent of handlers/packs (currently clean)
4. **pkg/handlers** and **pkg/packs** can be peers that import each other's sub-packages
5. **No true circular dependencies** - Go won't compile with actual import cycles

### 8. Detailed Findings

#### Current Clean Dependencies:
- **pkg/execution**: No imports (pure types)
- **pkg/datastore**: Only imports types, paths, filesystem (clean separation)
- **pkg/filesystem**: Minimal dependencies

#### Current Problematic Dependencies:
1. **operations → handlers**: Imports HandlerCategory type
2. **handlers → operations**: All handler implementations import operations
3. **types → execution**: Imports status constants

### 9. Migration Path

#### Phase 1: Resolve Immediate Issues
1. **Move HandlerCategory** from pkg/handlers to pkg/operations
   - This removes the operations → handlers dependency
   - handlers/lib/* already import operations, so no new dependencies

2. **Consider moving status types** from pkg/execution to pkg/types
   - Or accept the current dependency as it's not creating a cycle
   - pkg/execution has no dependencies, so it's safe at the bottom

#### Phase 2: Reorganize Structure
1. **Option A - Move operations under handlers/**:
   ```
   pkg/handlers/
   ├── operations/     # Operation types and interfaces
   ├── lib/           # Handler implementations
   └── pipeline/      # Execution pipeline
   ```
   - Pros: Logical grouping, handlers are self-contained
   - Cons: None significant

2. **Option B - Keep operations separate**:
   - Current structure works if we fix the HandlerCategory issue
   - operations and handlers remain peers

#### Phase 3: Validate No Cycles
After any reorganization:
1. Run `go mod verify` to ensure no import cycles
2. Check that datastore remains independent
3. Verify types has minimal dependencies

## Summary

### No Actual Import Cycles Found
The analysis shows that while there are some tight couplings and near-cycles, there are **no actual import cycles** that would prevent compilation. The main issues are:

1. **Tight coupling between operations and handlers** due to shared types
2. **Types depending on execution** for status constants
3. **Complex interdependencies** between packs, handlers, and operations

### Safe Refactoring Path
The proposed refactoring can proceed safely by:

1. **First fixing the type dependencies**:
   - Move `HandlerCategory` from handlers to operations (or to a shared types package)
   - This eliminates the operations → handlers import

2. **Then reorganizing the structure**:
   - Moving operations under handlers/operations is safe and logical
   - The packs package can remain separate and import handlers/operations
   - Datastore remains independent

### Key Insight
The current architecture already avoids true circular dependencies through careful layering:
- **Bottom layer**: execution, filesystem, paths (no pkg/ dependencies)
- **Foundation layer**: types (minimal dependencies)
- **Service layer**: datastore (depends only on foundation)
- **Domain layer**: operations, handlers (mutual dependency via types)
- **Application layer**: packs (depends on all lower layers)

This layering should be preserved in any refactoring.