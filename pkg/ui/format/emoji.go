// Package format provides formatting utilities for UI presentation.
package format

import "strings"

// HandlerEmoji returns an emoji representation for the given handler type.
// This is used in various UI outputs to provide visual distinction between handler types.
func HandlerEmoji(handler string) string {
	switch strings.ToLower(handler) {
	case "homebrew":
		return "🍺"
	case "symlink":
		return "🔗"
	case "provision":
		return "🔧"
	case "shell":
		return "🐚"
	case "path":
		return "📁"
	case "install":
		return "📦"
	default:
		return "⚙️"
	}
}
