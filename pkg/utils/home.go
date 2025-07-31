package utils

import (
	"fmt"
	"os"

	"github.com/arthur-debert/dodot/pkg/errors"
)

// GetHomeDirectory returns the user's home directory.
// It first tries os.UserHomeDir(), then falls back to the HOME environment variable.
// If both fail, it returns an error rather than using dangerous defaults.
//
// Deprecated: Use github.com/arthur-debert/dodot/pkg/paths.GetHomeDirectory instead.
// This function will be removed in a future version.
func GetHomeDirectory() (string, error) {
	// Try os.UserHomeDir first (most reliable)
	homeDir, err := os.UserHomeDir()
	if err == nil && homeDir != "" {
		return homeDir, nil
	}

	// Fall back to HOME environment variable
	homeDir = os.Getenv("HOME")
	if homeDir != "" {
		return homeDir, nil
	}

	// If both methods fail, return an error
	return "", errors.New(errors.ErrFileAccess, "unable to determine home directory: neither os.UserHomeDir() nor HOME environment variable are available")
}

// GetHomeDirectoryWithDefault returns the user's home directory or a default value.
// This should only be used in contexts where a default is acceptable.
//
// Deprecated: Use github.com/arthur-debert/dodot/pkg/paths.GetHomeDirectoryWithDefault instead.
// This function will be removed in a future version.
func GetHomeDirectoryWithDefault(defaultDir string) string {
	homeDir, err := GetHomeDirectory()
	if err != nil {
		return defaultDir
	}
	return homeDir
}

// ExpandHome expands the ~ character to the user's home directory.
// Returns an error if home directory cannot be determined.
//
// Deprecated: Use github.com/arthur-debert/dodot/pkg/paths.ExpandHome instead.
// This function will be removed in a future version.
func ExpandHome(path string) (string, error) {
	if path == "~" {
		return GetHomeDirectory()
	}

	if len(path) > 1 && path[0] == '~' && path[1] == '/' {
		homeDir, err := GetHomeDirectory()
		if err != nil {
			return "", fmt.Errorf("cannot expand ~: %w", err)
		}
		return homeDir + path[1:], nil
	}

	return path, nil
}
