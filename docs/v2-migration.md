# V2 Migration & Architecture Context

This document provides the essential context for V2 development, separating current state from historical complexity.

## Current State (V2 Baseline)

### ✅ Completed Components

**Core Execution System**
- `Executor` (renamed from DirectExecutor) - single execution path
- `ExecutorOptions` - configuration for executor  
- Action-based processing (no more dual Operation/Action paths)
- PowerUpResult-based results tracking
- Comprehensive synthfs integration

**Display Infrastructure** 
- Rich terminal display infrastructure in place (lipgloss + pterm)
- Adaptive color theming working
- `TextRenderer` (renamed from SimpleRenderer) provides basic text output
- `ExecutionContext → DisplayResult` transformation exists

**API Quality**
- Clean, version-agnostic naming (no "Direct", "Simple" prefixes)
- Forward-looking documentation (no legacy references)
- 670 tests passing

### 🚧 What Needs Implementation

**Display Data Collection & Templates**
- Collect correct return data for rich display
- Create rich display templates using existing infrastructure  
- The rendering framework exists, needs proper data + templates

**Commands Missing Rich Output**
- `deploy` - currently uses TextRenderer, needs rich display
- `install` - currently uses TextRenderer, needs rich display  
- `status` - not yet implemented
- `list` - simple output, likely fine as-is

## Architecture Decisions (Preserved for Reference)

### Eliminated Concepts
- **Dual execution paths** - now single Executor path only
- **Operation type** - replaced with Action execution + PowerUpResult tracking
- **Complex status checking** - unified into PowerUpResult status

### Key Design Principles  
- **Bottom-up transformation**: Each layer transforms its data upward
- **PowerUpResult as atomic unit**: If any action in PowerUp fails, PowerUp fails
- **ExecutionContext as top-level container**: Organizes results by pack
- **DisplayResult for rendering**: Final transformation for display layer

### Data Flow
```
Actions → Executor → PowerUpResults → ExecutionContext → DisplayResult → Renderer
```

## Implementation Guide for Display System

### Step 1: Rich Display Data Collection
The `ExecutionContext.ToDisplayResult()` method exists but needs enhancement:
- Collect file-level metadata (HasConfig, IsIgnored, LastExecuted) 
- Implement proper status aggregation rules
- Add pack-level configuration detection

### Step 2: Rich Display Templates
Using existing lipgloss + pterm infrastructure:
- Create templates for different command outputs
- Implement status indicators with colors
- Add progress/summary information

### Step 3: Integration  
- Update deploy/install commands to use rich renderer
- Implement status command with rich output
- Ensure consistent display across all commands

## Reference Materials

### Design Documents
- `docs/design/display.txxt` - Target display specification
- `docs/design/simpler.txxt` - Overall design philosophy  
- `docs/design/execution.txxt` - Implementation phases

### Historical Context (for reference only)
- Issue #466: Eliminate dual execution paths (✅ completed)
- Issue #487: Install Command Implementation (✅ completed) 
- Issue #490: Shell Integration (✅ completed)
- Issue #488: Standard output formatting (🚧 remaining work)
- Issue #492: Legacy API naming cleanup (✅ completed)

## File Structure Notes

### Key Implementation Files
- `pkg/core/direct_executor.go` - Main execution engine  
- `pkg/types/execution_context.go` - Result organization
- `pkg/types/results.go` - Display types and transformation
- `pkg/display/simple.go` - Basic text renderer (working)

### Display Infrastructure (ready to use)
- `pkg/display/` - Display system foundation
- Rich terminal utilities available via lipgloss + pterm
- Color theming and adaptive display working

This V2 branch represents a clean state with ~90% of refactoring complete. The remaining work is focused and well-defined: implement rich display data collection and templates using the existing infrastructure.