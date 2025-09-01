# Pack-Related Command Result Analysis

## Overview
This analysis examines how each pack-related command returns results and how they are rendered.

## Current Patterns

### Command Layer (pkg/commands/)

1. **status** - Returns `*types.DisplayResult`
   - Direct display of pack status
   - No additional processing needed

2. **on** - Returns `*OnResult` (custom type)
   - Contains LinkResult and ProvisionResult (*types.ExecutionContext)
   - Additional fields: TotalDeployed, DryRun, Errors
   - CLI converts to status display

3. **off** - Returns `*OffResult` (custom type)
   - Contains array of PackRemovalResult
   - Additional fields: TotalCleared, DryRun, Errors
   - CLI converts to status display

4. **adopt** - Returns `*types.AdoptResult`
   - Contains PackName and AdoptedFiles array
   - CLI converts to status display

5. **fill** - Returns `*types.FillResult`
   - Contains PackName and FilesCreated array
   - CLI converts to status display

6. **add-ignore** - Returns `*types.AddIgnoreResult`
   - Contains PackName, IgnoreFilePath, Created, AlreadyExisted
   - CLI converts to status display

### CLI Layer (cmd/dodot/root.go)

All commands except `status` follow this pattern:
1. Execute the command
2. Call `StatusPacks()` to get current state
3. Wrap in `types.CommandResult` with a message
4. Render the result

The message is generated using `types.FormatCommandMessage()` for on/off commands, or custom messages for others.

## Inconsistencies

### 1. **Result Type Inconsistency**
- `status` returns `DisplayResult` directly
- `on` and `off` return custom result types with execution details
- `adopt`, `fill`, and `add-ignore` return specific result types in `types` package

### 2. **Rendering Pattern**
- All commands convert to `DisplayResult` via status command
- This creates unnecessary coupling and extra status queries
- The original command results are mostly discarded

### 3. **Error Handling**
- `on` and `off` have Errors arrays in their results
- Other commands rely on error return values only
- Inconsistent error aggregation

### 4. **Information Loss**
- Command-specific information (e.g., what files were adopted, what was filled) is lost
- Everything is normalized to pack status display
- Users don't see the specific changes made

## Recommendations

### 1. **Standardize Result Types**
All pack commands should return `*types.CommandResult` containing:
- Message (optional)
- DisplayResult showing current state
- Command-specific details (if needed)

### 2. **Move Status Logic to Command Layer**
Each command should:
- Perform its operation
- Generate its own DisplayResult
- Return complete CommandResult

### 3. **Preserve Command-Specific Information**
- Add command-specific fields to DisplayFile or DisplayPack
- Show what changed, not just final state
- Example: adopt should show which files were moved

### 4. **Consistent Error Handling**
- All commands should aggregate errors consistently
- Consider adding Errors field to CommandResult
- Show warnings/errors in the display

### 5. **Eliminate Double Status Queries**
- Commands should build DisplayResult as part of their operation
- No need to query status separately after command execution
- More efficient and accurate