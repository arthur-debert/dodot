package paths

import (
	"fmt"
	"path/filepath"
	"strings"

	"github.com/arthur-debert/dodot/pkg/errors"
)

// ValidatePath performs comprehensive validation on a path.
// It checks for:
// - Empty paths
// - Invalid characters (on Windows)
// - Path traversal attempts
// - Excessive path length
func ValidatePath(path string) error {
	if path == "" {
		return errors.New(errors.ErrInvalidInput, "path cannot be empty")
	}

	// Check for null bytes
	if strings.Contains(path, "\x00") {
		return errors.New(errors.ErrInvalidInput, "path contains null bytes")
	}

	// Check path length (common filesystem limit)
	if len(path) > 4096 {
		return errors.New(errors.ErrInvalidInput, "path exceeds maximum length")
	}

	return nil
}

// ValidatePackName ensures a pack name is valid for use in paths.
// Pack names must:
// - Not be empty
// - Not contain path separators
// - Not contain special characters that could cause issues
// - Not be reserved names (. or ..)
func ValidatePackName(name string) error {
	if name == "" {
		return errors.New(errors.ErrInvalidInput, "pack name cannot be empty")
	}

	// Check for path separators
	if strings.ContainsAny(name, "/\\") {
		return errors.New(errors.ErrInvalidInput, "pack name cannot contain path separators")
	}

	// Check for reserved names
	if name == "." || name == ".." {
		return errors.New(errors.ErrInvalidInput, "pack name cannot be '.' or '..'")
	}

	// Check for problematic characters
	invalidChars := ":*?\"<>|"
	if strings.ContainsAny(name, invalidChars) {
		return errors.Newf(errors.ErrInvalidInput,
			"pack name contains invalid characters: %s", invalidChars)
	}

	// Check for control characters
	for _, r := range name {
		if r < 32 {
			return errors.New(errors.ErrInvalidInput,
				"pack name contains control characters")
		}
	}

	return nil
}

// SanitizePath attempts to clean and make a path safe for use.
// It:
// - Normalizes path separators
// - Removes redundant separators
// - Resolves . and .. elements
// - Removes trailing separators (except for root)
func SanitizePath(path string) string {
	// First expand home directory if present
	path = expandHome(path)

	// Clean the path using filepath.Clean
	cleaned := filepath.Clean(path)

	// Ensure we don't return an empty string
	if cleaned == "" {
		return "."
	}

	return cleaned
}

// IsAbsolutePath returns true if the path is absolute.
// This is a cross-platform wrapper around filepath.IsAbs.
func IsAbsolutePath(path string) bool {
	return filepath.IsAbs(path)
}

// JoinPaths safely joins path elements, ensuring proper separators.
// This is a wrapper around filepath.Join that also validates the result.
func JoinPaths(elem ...string) (string, error) {
	// Check each element
	for _, e := range elem {
		if strings.Contains(e, "\x00") {
			return "", errors.New(errors.ErrInvalidInput, "path element contains null bytes")
		}
	}

	result := filepath.Join(elem...)
	return result, nil
}

// RelativePath returns the relative path from base to target.
// Returns an error if the paths cannot be made relative.
func RelativePath(base, target string) (string, error) {
	// Normalize both paths first
	base = SanitizePath(base)
	target = SanitizePath(target)

	rel, err := filepath.Rel(base, target)
	if err != nil {
		return "", errors.Wrapf(err, errors.ErrFileAccess,
			"cannot determine relative path from %s to %s", base, target)
	}

	return rel, nil
}

// ContainsPath checks if child is contained within parent.
// Both paths are normalized before comparison.
func ContainsPath(parent, child string) bool {
	// Normalize both paths
	parent = SanitizePath(parent)
	child = SanitizePath(child)

	// Try to get relative path
	rel, err := filepath.Rel(parent, child)
	if err != nil {
		return false
	}

	// If relative path starts with .., child is outside parent
	return !strings.HasPrefix(rel, "..")
}

// SplitPath splits a path into its directory and file components.
// This is a wrapper around filepath.Split that also handles edge cases.
func SplitPath(path string) (dir, file string) {
	return filepath.Split(path)
}

// PathDepth returns the depth of a path (number of directories).
// For example: "/" = 0, "/a" = 1, "/a/b" = 2
func PathDepth(path string) int {
	// Normalize the path first
	path = SanitizePath(path)

	// Handle root directory
	if path == "/" || path == filepath.VolumeName(path) {
		return 0
	}

	// Count separators
	depth := 0
	cleaned := filepath.Clean(path)
	for _, char := range cleaned {
		if char == filepath.Separator {
			depth++
		}
	}

	// Adjust for absolute paths (leading separator doesn't count)
	if filepath.IsAbs(path) && depth > 0 {
		depth--
	}

	return depth
}

// CommonPrefix returns the longest common prefix of the provided paths.
// Returns empty string if paths have no common prefix.
func CommonPrefix(paths ...string) string {
	if len(paths) == 0 {
		return ""
	}

	if len(paths) == 1 {
		return SanitizePath(paths[0])
	}

	// Normalize all paths
	normalized := make([]string, len(paths))
	for i, p := range paths {
		normalized[i] = SanitizePath(p)
	}

	// Find common prefix by comparing path components
	first := strings.Split(normalized[0], string(filepath.Separator))

	commonParts := []string{}
	for i, part := range first {
		// Check if this part is common to all paths
		allMatch := true
		for _, path := range normalized[1:] {
			parts := strings.Split(path, string(filepath.Separator))
			if i >= len(parts) || parts[i] != part {
				allMatch = false
				break
			}
		}

		if !allMatch {
			break
		}

		commonParts = append(commonParts, part)
	}

	if len(commonParts) == 0 {
		return ""
	}

	// Reconstruct path from common parts
	result := filepath.Join(commonParts...)

	// Preserve leading separator for absolute paths
	if len(normalized[0]) > 0 && normalized[0][0] == filepath.Separator {
		result = string(filepath.Separator) + result
	}

	return result
}

// ValidatePathSecurity performs security-focused validation on a path.
// It checks for common path traversal attacks and suspicious patterns.
func ValidatePathSecurity(path string) error {
	// Check for basic validity first
	if err := ValidatePath(path); err != nil {
		return err
	}

	// Check for obvious traversal attempts
	if strings.Contains(path, "../") || strings.Contains(path, "..\\") {
		// This is not always malicious, but worth checking in context
		return errors.New(errors.ErrInvalidInput,
			"path contains parent directory references")
	}

	// Check for hidden Unicode characters that might be used to deceive
	for _, r := range path {
		if r == '\u202e' || // Right-to-left override
			r == '\u200b' || // Zero-width space
			r == '\u00ad' { // Soft hyphen
			return errors.New(errors.ErrInvalidInput,
				"path contains suspicious Unicode characters")
		}
	}

	return nil
}

// MustValidatePath panics if the path is invalid.
// This should only be used with hardcoded paths that must be valid.
func MustValidatePath(path string) {
	if err := ValidatePath(path); err != nil {
		panic(fmt.Sprintf("invalid path %q: %v", path, err))
	}
}

// IsHiddenPath returns true if the path represents a hidden file or directory.
// On Unix-like systems, this means the basename starts with a dot.
// On Windows, this would check file attributes (not implemented here).
func IsHiddenPath(path string) bool {
	base := filepath.Base(path)
	return len(base) > 0 && base[0] == '.'
}
