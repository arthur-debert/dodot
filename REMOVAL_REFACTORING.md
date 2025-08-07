# REMOVAL Refactoring: Eliminate Operation Layer (#466)

## ⚠️ CRITICAL: THIS IS ABOUT REMOVAL, NOT ADDITION

The previous refactoring attempt (PRs #470-#474) **FAILED** because it added new code alongside old code instead of **REMOVING** the intermediate Operation layer.

## Current Problem

We now have **TWO parallel execution systems** instead of the requested single simplified system:

1. **Old System (Still Active)**: Actions → ConvertActionsToOperations → Operations → SynthfsExecutor
2. **New System (Added)**: Actions → DirectExecutor → synthfs

This **increased complexity** instead of reducing it. We have more code paths, not fewer.

## What This Refactoring Will Do

**REMOVE** the Operation intermediate layer entirely:

### Files to be DELETED:
- `pkg/core/operations.go` (~600 lines)
- `pkg/types/operation.go` (Operation type definitions)
- `pkg/synthfs/synthfs_executor.go` (SynthfsExecutor)
- All `ConvertActionsToOperations*` functions

### Single Execution Path (Final State):
```
Actions → DirectExecutor → synthfs operations
```

### Commands to be MIGRATED:
- `init` command: Remove Operation conversion, use DirectExecutor
- `fill` command: Remove Operation conversion, use DirectExecutor

## Success Criteria

- ✅ **Zero Operation types remain** in codebase
- ✅ **Single execution pathway** only
- ✅ **~1200 lines of code removed**
- ✅ All tests pass
- ✅ **Simpler architecture** achieved

## Branch Strategy

**Master Branch**: `refactor/remove-operation-layer-master`
**Feature Pattern**: `refactor/remove-operation-layer-master/[feature]`

**ALL feature branches MUST target the master branch, NOT main.**

## This Is REMOVAL, Not Addition

If any implementation adds new execution paths, APIs, or keeps old code "for compatibility", it **violates the core requirement**.

The goal is **architectural simplification through elimination of layers**.