# Refactoring: Remove Operation Type (#466)

This document outlines the refactoring strategy for removing the Operation intermediate type and implementing direct Action-to-execution flow.

## Background

Issue #466 identifies that the current architecture has an unnecessary abstraction layer:

**Current Flow**: Actions → Operations → SynthfsExecutor → synthfs operations  
**Target Flow**: Actions → DirectExecutor → synthfs operations

The POC (PR #469) successfully validated this approach, demonstrating:
- Elimination of ~400+ lines of translation code
- Improved performance (no intermediate allocations)
- Simplified error handling with fail-fast validation
- Full compatibility with all power-up types

## Branching Strategy

**Root Branch**: `refac/remove-operation-type`
- This branch serves as the integration point for all refactoring work
- All feature branches must be created from and merged back to this branch
- NO direct merges to main until the complete refactoring is ready

**Feature Branch Pattern**: `refac/remove-operation-type/[feature-name]`
- Example: `refac/remove-operation-type/update-core-pipeline`
- Example: `refac/remove-operation-type/migrate-powerups`
- Example: `refac/remove-operation-type/update-commands`

**PR Strategy**:
- Feature branches create PRs targeting `refac/remove-operation-type` 
- Each PR should be focused on a specific component or concern
- The root branch maintains a single placeholder PR explaining the strategy

## Implementation Plan

### Phase 1: Core Infrastructure
- [ ] Integrate POC DirectExecutor into core pipeline
- [ ] Update ExecutionContext to work with Actions directly
- [ ] Migrate core pipeline functions (GetActions, Execute)

### Phase 2: PowerUp Migration  
- [ ] Update all PowerUps to generate Actions instead of Operations
- [ ] Remove Operation-based interfaces from PowerUp contracts
- [ ] Update PowerUp tests to validate Action generation

### Phase 3: Command Integration
- [ ] Update command layer to work with new execution flow
- [ ] Migrate deploy, install, status commands  
- [ ] Update command-level error handling and reporting

### Phase 4: Cleanup & Documentation
- [ ] Remove Operation type and related code
- [ ] Update documentation and examples
- [ ] Performance benchmarking and validation

## Testing Strategy

Each phase must maintain:
- ✅ All existing tests continue to pass
- ✅ No regression in functionality  
- ✅ Performance improvements are measurable
- ✅ Error handling maintains or improves current behavior

## Rollback Plan

If issues are discovered:
1. Feature branches can be reverted individually
2. Root branch can be abandoned if fundamental issues arise
3. POC branch demonstrates the approach works, providing confidence

## Success Criteria

- [ ] Complete removal of Operation type from codebase
- [ ] All tests pass (current: 1052 tests)
- [ ] Performance improvements demonstrated
- [ ] Code complexity reduction verified (target: -400+ lines)
- [ ] Documentation updated to reflect new architecture

---

**Related Issues**: #466  
**POC Validation**: PR #469  
**Root Branch**: `refac/remove-operation-type`