# Refactoring Execution Sequence

## Overview
This document provides the detailed step-by-step sequence for refactoring the execution flow. Each step includes specific files to create/modify and dependencies.

## Phase 1: Foundation (Week 1)

### Step 1.1: Create Pure Execution Types (Day 1)
**Goal**: Extract business logic from types into pure data structures

1. Create new files:
   ```
   pkg/execution/context/manager.go      # Business logic from ExecutionContext
   pkg/execution/results/aggregator.go   # Business logic from PackExecutionResult
   ```

2. Update existing types to remove methods:
   - `pkg/types/execution_context.go` - Remove AddPackResult(), Complete()
   - Keep only data fields and simple getters

3. Update callers to use new manager/aggregator instead of methods

**Dependencies**: None
**Can be done in parallel**: Yes
**Tests to update**: All tests using ExecutionContext methods

### Step 1.2: Create Rules Package (Day 2-3)
**Goal**: Separate rule matching from handler execution

1. Create new package structure:
   ```
   pkg/rules/
   ├── scanner.go      # Move from pkg/handlers/pipeline/scanner.go
   ├── matcher.go      # Move from pkg/handlers/pipeline/integration.go
   ├── types.go        # Move RuleMatch from pkg/handlers/pipeline/
   └── config.go       # Rule configuration loading
   ```

2. Update imports in:
   - `pkg/handlers/pipeline/execute.go` to use rules package
   - All handler implementations

**Dependencies**: Step 1.1 should be complete
**Can be done in parallel**: No
**Tests to move**: scanner_test.go, integration tests

### Step 1.3: Fix Handler Dependencies (Day 4-5)
**Goal**: Resolve the operations/handlers circular dependency

1. Move `HandlerCategory` type:
   - From: `pkg/handlers/api.go`
   - To: `pkg/operations/types.go` (or create `pkg/handlers/types.go`)

2. Update all imports of HandlerCategory

3. Consider moving operations under handlers:
   ```
   pkg/handlers/
   ├── operations/     # Move from pkg/operations/
   ├── registry.go
   └── lib/
   ```

**Dependencies**: Steps 1.1 and 1.2 complete
**Can be done in parallel**: No
**Tests**: Ensure no import cycles

## Phase 2: Pack Reorganization (Week 2)

### Step 2.1: Create Execution Package Structure (Day 1)
**Goal**: Set up the new execution package with dispatcher

1. Create initial structure:
   ```
   pkg/execution/
   ├── dispatcher.go    # Move logic from pkg/dispatcher/dispatcher.go
   ├── types.go         # Common execution types
   └── errors.go        # Execution-specific errors
   ```

2. Copy dispatcher logic, updating:
   - Package name to `execution`
   - Imports to use new paths
   - Keep CLI imports minimal

**Dependencies**: Phase 1 complete
**Can be done in parallel**: Yes
**Tests**: Copy dispatcher tests

### Step 2.2: Reorganize Pack Execution (Day 2-3)
**Goal**: Rename pipeline to execution, consolidate commands

1. Create new structure:
   ```
   pkg/packs/
   ├── execution/
   │   ├── executor.go    # From pipeline/execute.go
   │   ├── context.go     # Execution context for packs
   │   └── types.go       # Pack execution types
   ├── commands/          # Consolidate all commands here
   └── operations/        # From current pkg/packs/commands/
   ```

2. Move files:
   - `pkg/packs/pipeline/execute.go` → `pkg/packs/execution/executor.go`
   - `pkg/packs/pipeline/commands/*` → `pkg/packs/commands/`
   - `pkg/packs/commands/*` → `pkg/packs/operations/`

3. Update imports in moved files

**Dependencies**: Step 2.1 complete
**Can be done in parallel**: No
**Tests**: Move all associated tests

### Step 2.3: Move Core Utilities (Day 4-5)
**Goal**: Eliminate the core package

1. Move pack discovery:
   ```
   pkg/packs/discovery/
   ├── discover.go     # From pkg/core/getpacks.go
   ├── candidates.go   # Pack candidate logic
   └── select.go       # Pack selection logic
   ```

2. Move other utilities:
   - Output generators → `pkg/ui/output/`
   - File utilities → appropriate packages

3. Delete `pkg/core/` directory

**Dependencies**: Step 2.2 complete
**Can be done in parallel**: Partially
**Tests**: Move core tests to new locations

## Phase 3: Handler Reorganization (Week 3)

### Step 3.1: Create Handler Execution Structure (Day 1-2)
**Goal**: Set up clear handler execution pipeline

1. Create structure:
   ```
   pkg/execution/pipeline/
   ├── execute.go      # From pkg/handlers/pipeline/execute.go
   ├── filter.go       # Handler filtering logic
   ├── order.go        # Execution ordering
   └── types.go        # Pipeline-specific types
   ```

2. Move handler execution logic, updating:
   - Imports to use `pkg/rules/` for matching
   - References to new execution context

**Dependencies**: Phase 2 complete
**Can be done in parallel**: No
**Tests**: Move handler pipeline tests

### Step 3.2: Update Integration Points (Day 3-4)
**Goal**: Connect all the pieces

1. Update `pkg/execution/dispatcher.go`:
   - Use new pack execution paths
   - Use new handler execution paths

2. Update pack commands to use new handler execution:
   - `pkg/packs/commands/on.go`
   - `pkg/packs/commands/off.go`
   - etc.

3. Ensure all tests pass

**Dependencies**: Step 3.1 complete
**Can be done in parallel**: No
**Tests**: Integration tests critical here

### Step 3.3: Final Handler Cleanup (Day 5)
**Goal**: Remove old handler pipeline package

1. Delete `pkg/handlers/pipeline/` directory
2. Update any remaining imports
3. Run full test suite

**Dependencies**: Step 3.2 complete
**Can be done in parallel**: No
**Tests**: Full regression test

## Phase 4: Cleanup and Documentation (Week 4)

### Step 4.1: Remove Old Packages (Day 1)
1. Delete:
   - `pkg/dispatcher/` (now in execution)
   - `pkg/core/` (distributed to other packages)
   - Any empty directories

2. Run `go mod tidy`

### Step 4.2: Update All Imports (Day 2-3)
1. Global search and replace for old import paths
2. Fix any compilation errors
3. Run all tests

### Step 4.3: Documentation Update (Day 4-5)
1. Update package documentation
2. Create architecture diagram
3. Update README with new structure
4. Document migration for any external users

## Rollback Points

Each phase can be rolled back independently:

1. **Phase 1 Rollback**: Revert type changes, restore methods
2. **Phase 2 Rollback**: Restore dispatcher, move files back
3. **Phase 3 Rollback**: Restore handler pipeline
4. **Phase 4 Rollback**: Restore deleted packages from git

## Testing Strategy

### Unit Tests
- Run after each step
- Update test imports immediately
- Keep test coverage above current levels

### Integration Tests
- Run after each phase
- Critical after Phase 2 and 3
- Create new integration tests for new structure

### Performance Tests
- Benchmark before starting
- Benchmark after Phase 3
- Ensure no performance regression

## Risk Mitigation

1. **Branch Strategy**:
   - Create feature branch: `refactor/execution-flow`
   - Create phase branches: `refactor/phase-1`, etc.
   - Merge to main only after full phase completion

2. **Continuous Integration**:
   - Ensure CI passes after each commit
   - Don't merge broken builds

3. **Code Review**:
   - Review each phase before moving to next
   - Get team buy-in on major changes

## Success Criteria

After each phase:
1. All tests pass
2. No new linting errors
3. No import cycles
4. Documentation updated
5. Code coverage maintained or improved

## Communication Plan

- Daily updates during refactoring
- Phase completion announcements
- Final presentation of new architecture