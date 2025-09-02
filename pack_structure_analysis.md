# Execution Flow Refactoring Plan

## Executive Summary

The current codebase has execution logic scattered across multiple packages with confusing naming conventions and mixed responsibilities. This document proposes a comprehensive refactoring to create a clearer, more maintainable architecture.

## Current Problems

### 1. Scattered Execution Logic
- **Dispatcher**: Redundant routing layer between CLI and pack pipeline
- **Core**: Thin package with mostly utility functions
- **Pack Pipeline**: Mixed with command implementations  
- **Handler Pipeline**: Combines rule matching with execution
- **Operations**: Low-level execution without clear boundaries
- **Types**: Contains business logic that should be in executors

### 2. Confusing Naming
- Two "pipeline" packages (pack and handler) with different purposes
- Both `pkg/packs/commands/` and `pkg/packs/pipeline/commands/`
- "Core" package that isn't really core to anything

### 3. Mixed Responsibilities
- ExecutionContext and PackExecutionResult have business logic methods
- Handler pipeline does both rule matching AND execution
- Pack commands mixed between pipeline and operations

## Proposed Architecture

### High-Level Flow
```
CLI Commands
    ↓
Execution Dispatcher (merged from pkg/dispatcher)
    ↓
Pack Execution Engine
    ↓
Handler Execution Engine
    ↓
Operations Executor
    ↓
DataStore
```

### Package Structure

```
pkg/
├── packs/
│   ├── discovery/        # Finding and selecting packs
│   ├── config/          # Pack configuration (.dodot.toml)
│   ├── execution/       # Pack command execution
│   ├── commands/        # Command implementations (on, off, status, etc.)
│   └── operations/      # Reusable pack operations
│
├── rules/               # Rule matching system (from handlers/pipeline)
│   ├── scanner.go      # File scanning
│   ├── matcher.go      # Rule matching
│   └── config.go       # Rule configuration
│
├── handlers/           # Handler registry and implementations
│   ├── registry.go     # Handler registration
│   ├── categories.go   # Handler categorization
│   └── lib/           # Individual handlers
│
├── execution/          # Execution orchestration
│   ├── dispatcher.go   # Command routing (from pkg/dispatcher)
│   ├── pipeline/       # Handler execution pipeline
│   ├── commands/       # Handler-specific commands
│   └── context.go      # Execution context management
│
├── operations/         # Low-level operations (unchanged)
│   ├── executor.go
│   └── types.go
│
└── types/             # Pure data types (no business logic)
    ├── pack.go
    ├── handler.go
    └── result.go
```

## Refactoring Steps

### Phase 1: Foundation (Week 1)
1. **Extract business logic from types**
   - Move ExecutionContext methods to execution/context.go
   - Move PackExecutionResult methods to execution/results.go
   - Keep types as pure data structures

2. **Create rules package**
   - Move scanning/matching from handlers/pipeline
   - Separate rule matching from execution

3. **Fix HandlerCategory dependency**
   - Move HandlerCategory type to where it belongs
   - Resolve operations/handlers coupling

### Phase 2: Pack Reorganization (Week 2)
1. **Merge dispatcher into execution**
   - Move dispatcher logic to `pkg/execution/dispatcher.go`
   - Keep CLI thin - only user interaction
   - Direct CLI → Execution (with dispatch) → Pack execution flow

2. **Reorganize pack execution**
   - Rename pipeline → execution
   - Consolidate command implementations
   - Move pack operations to operations/

3. **Move core utilities**
   - Pack discovery → packs/discovery/
   - Delete unnecessary core package

### Phase 3: Handler Reorganization (Week 3)
1. **Create execution package**
   - Move handler execution from handlers/pipeline
   - Create clear execution pipeline

2. **Separate concerns**
   - Rules: What files match
   - Handlers: What operations to perform
   - Execution: How to orchestrate
   - Operations: Actual work

### Phase 4: Cleanup (Week 4)
1. **Remove old packages**
   - Delete pkg/dispatcher (after merging into execution)
   - Delete pkg/core
   - Delete old pipeline packages

2. **Update imports**
   - Fix all import paths
   - Run tests

3. **Documentation**
   - Update package docs
   - Create architecture diagram

## Benefits

### 1. Clearer Architecture
- Single responsibility per package
- Clear data flow
- No redundant layers

### 2. Better Testability
- Isolated components
- Pure functions for business logic
- Clear boundaries

### 3. Easier Maintenance
- Logical code organization
- Consistent patterns
- Less cognitive overhead

### 4. No Import Cycles
- Clear dependency hierarchy
- Foundation → Service → Domain → Application layers
- No circular dependencies

## Migration Strategy

### Compatibility
- Create new packages alongside old
- Maintain backward compatibility during transition
- Gradual migration of features

### Testing
- Full test coverage before changes
- Test each phase independently
- Integration tests for entire flow

### Rollback Plan
- Each phase is reversible
- Git branches for each phase
- Feature flags if needed

## Success Metrics
1. Reduced code duplication
2. Clearer import graph
3. Faster development of new features
4. Easier onboarding for new developers
5. Better test coverage

## Timeline
- Phase 1: 1 week
- Phase 2: 1 week  
- Phase 3: 1 week
- Phase 4: 1 week
- Total: 4 weeks

## Risks and Mitigations
1. **Breaking changes**: Use compatibility layers
2. **Import cycles**: Check dependencies before moving
3. **Test failures**: Comprehensive test suite
4. **Performance**: Benchmark before/after

This refactoring will significantly improve code clarity and maintainability while preserving all existing functionality.