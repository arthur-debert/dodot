# Pack-Related Structure and Dependencies Analysis

## Current Package Structure

### 1. Core Pack-Related Packages

#### `/pkg/types/`
- **pack.go**: Defines the core `Pack` struct and basic methods
- **Purpose**: Core type definitions without dependencies on higher-level packages
- **Imports**: Only standard library and basic types

#### `/pkg/packs/`
- **Purpose**: Pack discovery, selection, and configuration handling
- **Key files**:
  - `discovery.go`: Finding pack directories
  - `selection.go`: Selecting specific packs from discovered ones
  - `config.go`: Pack configuration handling
  - `ignore.go`: Pack ignore file handling
  - `normalize.go`: Normalizing pack names
- **Imports**: 
  - `pkg/types` (for Pack type)
  - `pkg/filesystem`
  - `pkg/config`
  - `pkg/errors`
  - `pkg/logging`

#### `/pkg/packcommands/`
- **Purpose**: Higher-level pack operations that require dependencies on other packages
- **Key files**:
  - `pack.go`: Wrapper around types.Pack to add higher-level methods
  - `status.go`: Pack status checking operations
  - `provisioning.go`: Pack provisioning operations
  - `file_operations.go`: File-related pack operations
  - `adopt_file.go`: File adoption operations
  - `ignore_file.go`: Ignore file operations
  - `status_command.go`: Status command implementation
- **Imports**:
  - `pkg/types`
  - `pkg/datastore`
  - `pkg/config`
  - `pkg/handlerpipeline`
  - `pkg/handlers`
  - `pkg/paths`
  - `pkg/ui/display`
  - `pkg/utils`
  - `pkg/core` (only in status_command.go)
  - `pkg/filesystem` (only in status_command.go)
  - **Does NOT import**: `pkg/packs` or `pkg/packpipeline`

#### `/pkg/packpipeline/`
- **Purpose**: Orchestration for executing commands across multiple packs
- **Key files**:
  - `types.go`: Command interface and result types
  - `execute.go`: Main pipeline execution logic
- **Structure**:
  - `commands/` subdirectory for command implementations
- **Imports**:
  - `pkg/types`
  - `pkg/core` (for DiscoverAndSelectPacksFS)
  - `pkg/filesystem`
  - `pkg/logging`
  - **Does NOT import**: `pkg/packs` directly

#### `/pkg/packpipeline/commands/`
- **Purpose**: Individual command implementations for the pipeline
- **Commands**:
  - `on.go`: Enable pack command
  - `off.go`: Disable pack command
  - `status.go`: Status check command
  - `init.go`: Initialize pack command
  - `fill.go`: Fill pack command
  - `adopt.go`: Adopt files command
  - `addignore.go`: Add to ignore file command
- **Imports**:
  - `pkg/packcommands` (for higher-level pack operations)
  - `pkg/packpipeline` (for types)
  - `pkg/types`
  - `pkg/datastore`
  - `pkg/filesystem`
  - `pkg/handlerpipeline`
  - `pkg/handlers`
  - `pkg/paths`
  - `pkg/shell`
  - **Does NOT import**: `pkg/packs`

#### `/pkg/core/`
- **getpacks.go**: Helper functions for pack discovery and selection
- **Purpose**: Bridge between low-level pack discovery and higher-level operations
- **Imports**:
  - `pkg/packs` (for discovery functions)
  - `pkg/types`
  - `pkg/filesystem`
  - `pkg/errors`

#### `/pkg/dispatcher/`
- **dispatcher.go**: Central command dispatcher
- **Purpose**: Entry point from CLI, routes to appropriate command implementations
- **Imports**:
  - `pkg/packcommands`
  - `pkg/packpipeline`
  - `pkg/packpipeline/commands`
  - Various other packages

## Dependency Flow

```
CLI Layer (cmd/dodot)
    ↓
dispatcher
    ↓
packpipeline + packpipeline/commands
    ↓
packcommands (for pack-specific operations)
    ↓
core (for pack discovery via getpacks.go)
    ↓
packs (low-level discovery and selection)
    ↓
types (core Pack struct)
```

## Key Observations

1. **No Circular Dependencies**: The current structure avoids circular imports by having a clear hierarchy
2. **Clear Separation of Concerns**:
   - `types`: Core data structures
   - `packs`: Low-level pack discovery and selection
   - `packcommands`: Higher-level pack operations
   - `packpipeline`: Command orchestration across multiple packs
   - `core`: Bridge functions and helpers
   - `dispatcher`: CLI routing

3. **Dependency Direction**:
   - Higher-level packages import lower-level ones
   - `packcommands` does NOT import `packpipeline` (avoiding circular dependency)
   - `packpipeline/commands` imports `packcommands` for pack operations
   - Only `core` and `packs` packages directly handle pack discovery

## Potential Risks for Circular Imports

1. **If packcommands starts importing packpipeline**: This would create a circular dependency since packpipeline/commands already imports packcommands
2. **If packs starts importing packcommands or packpipeline**: This would break the clean hierarchy
3. **If types starts importing any higher-level package**: This would break the foundation

## Safe Reorganization Guidelines

1. **Keep types package minimal**: Only core data structures, no business logic
2. **Keep packs focused on discovery**: Don't add dependencies on higher-level packages
3. **Use packcommands for pack-specific operations**: Operations that work on a single pack
4. **Use packpipeline for multi-pack orchestration**: Commands that need to work across multiple packs
5. **Use core as a bridge**: When higher-level packages need low-level functionality
6. **Keep dispatcher as the top-level router**: All CLI commands go through here

## Current Import Relationships Summary

- `types` → (no pack-related imports)
- `packs` → `types`
- `packcommands` → `types` (not `packs` or `packpipeline`)
- `packpipeline` → `types`, `core` (not `packs` directly)
- `packpipeline/commands` → `packcommands`, `packpipeline`, `types`
- `core` → `packs`, `types`
- `dispatcher` → `packcommands`, `packpipeline`, `packpipeline/commands`