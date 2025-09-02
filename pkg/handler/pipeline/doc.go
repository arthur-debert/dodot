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
// # Rule Matching
//
// Rules are evaluated in the order they appear in the configuration.
// The first matching rule wins. Exclusion patterns (!) are always
// checked first to ensure files are properly excluded.
//
// Handler execution order is controlled by the handler's RunMode:
//   - Provisioning handlers run before linking handlers
//   - Within each mode, handlers run in the order matches were found
//
// # Configuration
//
// Rules can be defined in the global config or pack-specific .dodot.toml files:
//
//	[[rules]]
//	pattern = "install.sh"
//	handler = "install"
//
//	[[rules]]
//	pattern = "*.sh"
//	handler = "shell"
//	options = { placement = "aliases" }
//
//	[[rules]]
//	pattern = "*"
//	handler = "symlink"
//
// Pack-specific rules are prepended to global rules to ensure they
// match first.
package pipeline
