package packs

import "strings"

// NormalizePackName removes trailing slashes from a pack name.
// This handles cases where shell completion adds a trailing slash to directory names.
func NormalizePackName(name string) string {
	return strings.TrimRight(name, "/")
}

// NormalizePackNames removes trailing slashes from all pack names in the slice.
func NormalizePackNames(names []string) []string {
	normalized := make([]string, len(names))
	for i, name := range names {
		normalized[i] = NormalizePackName(name)
	}
	return normalized
}
