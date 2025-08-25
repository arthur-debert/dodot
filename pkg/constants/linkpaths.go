// Package constants provides shared constants used across the dodot codebase.
// DEPRECATED: CoreUnixExceptions has been moved to pkg/config.
// This package now re-exports it for backward compatibility.
package constants

import "github.com/arthur-debert/dodot/pkg/config"

// CoreUnixExceptions defines files/dirs that always deploy to $HOME.
// DEPRECATED: Use config.GetLinkPaths().CoreUnixExceptions instead.
var CoreUnixExceptions = config.GetLinkPaths().CoreUnixExceptions
