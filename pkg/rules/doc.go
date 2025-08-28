// Package rules provides a simplified pattern-based file matching system for dodot.
//
// This package replaces the complex matcher/trigger/handler system with a simple
// rule-based approach where patterns directly map to handlers.
//
// # Pattern Conventions
//
// Rules use glob patterns with special conventions:
//
//   - `install.sh` - Exact filename match
//   - `*.sh` - Glob pattern matching
//   - `bin/` - Directory matching (trailing slash)
//   - `**/config/*` - Path pattern matching
//   - `*` - Catchall pattern
//   - `!*.tmp` - Exclusion pattern (leading !)
//
// # Rule Priority
//
// Rules are evaluated in priority order (higher values first). The first
// matching rule wins. Exclusion rules should have the highest priority to
// ensure files are properly excluded before other rules are evaluated.
//
// # Configuration
//
// Rules can be defined in the global config or pack-specific .dodot.toml files:
//
//	[[rules]]
//	pattern = "install.sh"
//	handler = "install"
//	priority = 90
//
//	[[rules]]
//	pattern = "*.sh"
//	handler = "shell"
//	priority = 80
//	options = { placement = "aliases" }
//
//	[[rules]]
//	pattern = "*"
//	handler = "symlink"
//	priority = 0
//
// Pack-specific rules automatically receive a priority boost to ensure they
// override global rules.
package rules
