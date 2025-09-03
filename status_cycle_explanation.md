# Status Implementation Stub Explanation

## The Circular Dependency Issue

The status implementation is stubbed because of a circular dependency that would occur if it were fully implemented:

```
handlers → operations (for HandlerCategory type)
operations → handlers (would be needed for status checking)
```

## Why Status Checking Needs Handlers

The proper status checking logic needs to:

1. **Determine handler type** to know how to check status:
   - Configuration handlers (symlink, path, shell): Check for intermediate links
   - Code execution handlers (install, homebrew): Check for sentinels

2. **Call handler-specific logic**:
   - Each handler knows how to verify its own deployment state
   - For example, symlink checks if links exist, homebrew checks sentinels

## The Current Stub

```go
func getHandlerStatus(match rules.RuleMatch, pack types.Pack, dataStore datastore.DataStore, fs types.FS, pathsInstance paths.Paths) (Status, error) {
    // This is a simplified implementation
    // In a real implementation, this would check datastore state, symlinks, etc.
    return Status{
        State:   StatusStatePending,
        Message: "Not deployed",
    }, nil
}
```

## What the Real Implementation Would Need

```go
// This would create a circular import!
import "github.com/arthur-debert/dodot/pkg/handlers"

func getHandlerStatus(...) (Status, error) {
    // Need to know handler category
    category := handlers.HandlerRegistry.GetHandlerCategory(match.HandlerName)
    
    switch category {
    case operations.CategoryConfiguration:
        // Check for intermediate links
        linkPath := pathsInstance.PackHandlerDir(pack.Name, match.HandlerName)
        exists, err := fs.Exists(filepath.Join(linkPath, match.Path))
        if exists {
            return Status{State: StatusStateReady}, nil
        }
        
    case operations.CategoryCodeExecution:
        // Check sentinels
        // Would need handler-specific sentinel generation logic
        sentinel := generateSentinel(match) // This needs handler knowledge!
        exists, err := dataStore.HasSentinel(pack.Name, match.HandlerName, sentinel)
        if exists {
            return Status{State: StatusStateReady}, nil
        }
    }
    
    return Status{State: StatusStatePending}, nil
}
```

## Solutions

1. **Move status checking to a neutral package** that both can import
2. **Use interfaces** to break the dependency
3. **Move handler category logic** to a shared location
4. **Reverse the dependency** - have handlers provide status checking

This is a classic architectural issue where domain logic (status checking) needs knowledge from multiple layers that have a hierarchical relationship.