# feat: Implement two-layer koanf configuration system (Phase III+IV)

## Summary

This PR implements a comprehensive two-layer configuration system using koanf, providing a clean user-facing interface while maintaining internal flexibility. This combines Phase III (YAML externalization) and Phase IV (koanf integration) into a single implementation.

## Key Features

### 🔧 Two-Layer Configuration Architecture

1. **User-facing layer** - Clean snake_case YAML that users write:
   ```yaml
   pack:
     ignore: [.cache, .tmp]
   symlink:
     force_home: [ssh, aws]
     protected_paths: [.ssh/id_rsa, .aws/credentials]
   ```

2. **Internal layer** - System configuration with complex nested structures:
   ```yaml
   logging:
     default_level: warn
     verbosity_levels:
       0: warn
       1: info
   priorities:
     triggers:
       filename: 100
   ```

### 📥 Multi-Source Configuration Loading

Configuration sources are loaded in priority order (highest to lowest):
1. **Environment variables** (`DODOT_*`) - Override everything
2. **User config files** - Repository or user-specific settings
3. **Embedded user defaults** - User-friendly defaults  
4. **Embedded system defaults** - Core system configuration

User config file locations checked (first found wins):
- `$DOTFILES_ROOT/.dodot/config.yaml` - Repository-specific
- `$XDG_CONFIG_HOME/dodot/config.yaml` - User-specific
- `~/.config/dodot/config.yaml` - Fallback

### 🔄 Configuration Transformation

The loader automatically transforms between user-friendly and internal formats:
- `pack.ignore` → `patterns.pack_ignore`
- `symlink.force_home` → `link_paths.force_home` (array to map conversion)
- `symlink.protected_paths` → `security.protected_paths` (array to map conversion)

## Implementation Decisions

### Environment Variable Array Override Behavior

**Current Implementation**: Environment variables completely **replace** arrays rather than appending to them.

**Rationale**:
- **Predictable behavior**: `DODOT_PACK_IGNORE=".git,.cache"` results in exactly `[".git", ".cache"]`
- **CI/CD friendly**: Environments can fully control configuration without inheriting unwanted defaults
- **Security conscious**: No risk of accidentally inheriting sensitive patterns

**Example**:
```bash
# Default: [.git, .svn, .hg, node_modules, .DS_Store]
# User config adds: [.cache, .tmp]
# Result before env: [.git, .svn, .hg, node_modules, .DS_Store, .cache, .tmp]

export DODOT_PACK_IGNORE=".git,.private"
# Result after env: [.git, .private]  # Complete replacement
```

**Alternative Considered**: Appending to arrays
- Would require special syntax to differentiate append vs replace (e.g., `+=` prefix)
- More complex for users to understand
- Harder to achieve deterministic configuration in automated environments

### Nested Configuration Merge Behavior

**Current Implementation**: Deep merging with selective override at each level.

**How it works**:
1. **Maps merge recursively**: Nested maps are merged key by key
2. **Arrays are replaced**: When the same array key exists, the higher priority source replaces the entire array
3. **Scalars are overridden**: Simple values (strings, numbers, bools) are replaced

**Example**:
```yaml
# Default config
logging:
  verbosity_levels:
    0: warn
    1: info
    2: debug
  default_level: warn
  enable_color: true

# User config
logging:
  verbosity_levels:
    1: trace  # Override just level 1
  enable_color: false

# Result after merge
logging:
  verbosity_levels:
    0: warn   # Preserved from default
    1: trace  # Overridden by user
    2: debug  # Preserved from default
  default_level: warn  # Preserved (not specified in user config)
  enable_color: false  # Overridden by user
```

**Design Decisions**:
- **Selective override**: Users can change specific nested values without redefining entire structures
- **Preservation of defaults**: Unspecified values remain at their defaults
- **No array merging**: Arrays are atomic units - this prevents unexpected combinations of values

## Technical Details

### Dependencies Added
- `github.com/knadh/koanf/v2` - Core configuration library
- `github.com/knadh/koanf/providers/*` - File, environment, and confmap providers
- `github.com/knadh/koanf/parsers/yaml` - YAML parsing
- `github.com/go-viper/mapstructure/v2` - Flexible struct unmarshaling

### File Structure
```
pkg/config/
├── loader.go                 # Main configuration loader
├── embedded/
│   ├── defaults.yaml        # System defaults
│   └── user-defaults.yaml   # User-friendly defaults
├── loader_test.go           # Basic loader tests
└── loader_comprehensive_test.go # Edge case tests
```

### Struct Tags
All config structs now have `koanf:` tags for field mapping:
```go
type Security struct {
    ProtectedPaths map[string]bool `koanf:"protected_paths"`
}
```

## Migration Impact

This is a **breaking change** with no backward compatibility:
- The configuration system is completely replaced
- Users will need to migrate any custom configurations to the new format
- The benefits of the cleaner, more flexible system outweigh the migration cost

## Testing

Comprehensive test coverage includes:
- ✅ Configuration loading from all sources
- ✅ Transformation between user and internal formats  
- ✅ Post-processing and validation
- ✅ Concurrent access safety
- ✅ Edge cases (empty configs, invalid types)

Some tests were deferred for future refinement:
- Permission error handling (should gracefully skip vs error)
- Complex environment variable scenarios
- Deep merge conflict resolution

## Future Enhancements

1. **Configuration validation** - Schema validation for user configs
2. **Configuration reload** - Hot reload without restart
3. **Array append syntax** - Special env var syntax for appending vs replacing
4. **Type coercion** - Better handling of string to complex type conversions

## Related Issues

Closes #631 - Configuration centralization and externalization

## Checklist

- [x] Code follows project conventions (see docs/dev/)
- [x] Tests pass locally
- [x] Documentation updated
- [x] No backward compatibility maintained (intentional)