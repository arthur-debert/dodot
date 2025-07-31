# Path and Filesystem Refactoring

## Overview
This PR introduces a comprehensive refactoring of dodot's path handling and filesystem operations to improve safety, testability, and maintainability.

## Problems Addressed
1. **Direct filesystem access**: PowerUps bypass the action system for file reading
2. **Scattered path logic**: Path handling spread across multiple packages
3. **Missing XDG compliance**: Manual XDG handling instead of proper library
4. **No operation execution**: Pipeline generates but doesn't execute operations
5. **Testing risks**: Tests could potentially write to user's home directory

## Implementation Plan
This PR creates the foundation for a series of improvements tracked in issues #63-#71:

### Phase 1: Action System Enhancement (#63, #64)
- Add file reading support to action/operation system
- Fix PowerUps to use actions exclusively

### Phase 2: Path Centralization (#65-#69)
- Create centralized `pkg/paths` package
- Integrate `github.com/adrg/xdg` for proper XDG support
- Migrate all path operations to central API
- Add comprehensive testing with mocks

### Phase 3: Safe Execution (#70, #71)
- Enable synthfs execution for safe PowerUps
- Implement SymlinkPowerUp execution with safety measures

## Key Design Decisions
1. **Phased approach**: Path refactoring before execution to ensure safety
2. **XDG compliance**: Using established library rather than manual implementation
3. **Test isolation**: Mock paths to prevent test pollution
4. **Double-symlink**: Safe deployment approach for user files

## Next Steps
The issues have been created in order of execution. Each issue contains detailed context and acceptance criteria for implementation.

## Related Issues
- Fixes #45 (Path handling is scattered)
- Addresses filesystem safety concerns
- Enables proper testing isolation
