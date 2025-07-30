package testutil

import (
	"crypto/sha256"
	"fmt"

	"io"
	"os"
	"path/filepath"
	"testing"
)

// TempDir creates a temporary directory for tests and returns its path.
// The directory is automatically cleaned up when the test completes.
func TempDir(t *testing.T, prefix string) string {
	t.Helper()
	return t.TempDir()
}

// CreateFile creates a file with the given content in the specified directory.
// It fails the test if the file cannot be created.
func CreateFile(t *testing.T, dir, name, content string) string {
	t.Helper()

	path := filepath.Join(dir, name)

	// Create parent directories if needed
	if err := os.MkdirAll(filepath.Dir(path), 0755); err != nil {
		t.Fatalf("Failed to create parent directories for %s: %v", path, err)
	}

	if err := os.WriteFile(path, []byte(content), 0644); err != nil {
		t.Fatalf("Failed to create file %s: %v", path, err)
	}

	return path
}

// CreateDir creates a directory in the specified parent directory.
// It fails the test if the directory cannot be created.
func CreateDir(t *testing.T, parent, name string) string {
	t.Helper()

	path := filepath.Join(parent, name)

	if err := os.MkdirAll(path, 0755); err != nil {
		t.Fatalf("Failed to create directory %s: %v", path, err)
	}

	return path
}

// CreateSymlink creates a symbolic link pointing to target.
// It fails the test if the symlink cannot be created.
func CreateSymlink(t *testing.T, target, link string) {
	t.Helper()

	// Create parent directory for the link if needed
	if err := os.MkdirAll(filepath.Dir(link), 0755); err != nil {
		t.Fatalf("Failed to create parent directory for symlink %s: %v", link, err)
	}

	if err := os.Symlink(target, link); err != nil {
		t.Fatalf("Failed to create symlink %s -> %s: %v", link, target, err)
	}
}

// FileExists checks if a file exists and is not a directory.
func FileExists(t *testing.T, path string) bool {
	t.Helper()

	info, err := os.Stat(path)
	if err != nil {
		return false
	}

	return !info.IsDir()
}

// DirExists checks if a directory exists.
func DirExists(t *testing.T, path string) bool {
	t.Helper()

	info, err := os.Stat(path)
	if err != nil {
		return false
	}

	return info.IsDir()
}

// SymlinkExists checks if a path is a symbolic link.
func SymlinkExists(t *testing.T, path string) bool {
	t.Helper()

	info, err := os.Lstat(path)
	if err != nil {
		return false
	}

	return info.Mode()&os.ModeSymlink != 0
}

// ReadFile reads the content of a file and returns it as a string.
// It fails the test if the file cannot be read.
func ReadFile(t *testing.T, path string) string {
	t.Helper()

	content, err := os.ReadFile(path)
	if err != nil {
		t.Fatalf("Failed to read file %s: %v", path, err)
	}

	return string(content)
}

// ReadSymlink reads the target of a symbolic link.
// It fails the test if the link cannot be read.
func ReadSymlink(t *testing.T, path string) string {
	t.Helper()

	target, err := os.Readlink(path)
	if err != nil {
		t.Fatalf("Failed to read symlink %s: %v", path, err)
	}

	return target
}

// AssertFileContent checks that a file exists and has the expected content.
func AssertFileContent(t *testing.T, path, expected string) {
	t.Helper()

	if !FileExists(t, path) {
		t.Fatalf("File %s does not exist", path)
	}

	actual := ReadFile(t, path)
	if actual != expected {
		t.Errorf("File %s content mismatch\nExpected: %q\nActual: %q", path, expected, actual)
	}
}

// AssertSymlink checks that a symlink exists and points to the expected target.
func AssertSymlink(t *testing.T, link, expectedTarget string) {
	t.Helper()

	if !SymlinkExists(t, link) {
		t.Fatalf("Symlink %s does not exist", link)
	}

	actualTarget := ReadSymlink(t, link)
	if actualTarget != expectedTarget {
		t.Errorf("Symlink %s target mismatch\nExpected: %s\nActual: %s", link, expectedTarget, actualTarget)
	}
}

// AssertNoFile checks that a file does not exist.
func AssertNoFile(t *testing.T, path string) {
	t.Helper()

	if _, err := os.Stat(path); !os.IsNotExist(err) {
		t.Errorf("File %s exists but should not", path)
	}
}

// Chmod changes the permissions of a file or directory.
// It fails the test if the operation fails.
func Chmod(t *testing.T, path string, mode os.FileMode) {
	t.Helper()

	if err := os.Chmod(path, mode); err != nil {
		t.Fatalf("Failed to chmod %s: %v", path, err)
	}
}

// RequireRoot skips the test if not running as root.
func RequireRoot(t *testing.T) {
	t.Helper()

	if os.Geteuid() != 0 {
		t.Skip("Test requires root privileges")
	}
}

// SkipOnWindows skips the test if running on Windows.
func SkipOnWindows(t *testing.T) {
	t.Helper()

	if os.PathSeparator == '\\' {
		t.Skip("Test not supported on Windows")
	}
}

// Setenv sets an environment variable for the duration of the test.
func Setenv(t *testing.T, key, value string) {
	t.Helper()

	original, wasSet := os.LookupEnv(key)

	if err := os.Setenv(key, value); err != nil {
		t.Fatalf("Failed to set environment variable %s: %v", key, err)
	}

	t.Cleanup(func() {
		if wasSet {
			if err := os.Setenv(key, original); err != nil {
				t.Fatalf("Failed to restore environment variable %s: %v", key, err)
			}
		} else {
			if err := os.Unsetenv(key); err != nil {
				t.Fatalf("Failed to unset environment variable %s: %v", key, err)
			}
		}
	})
}

// CalculateFileChecksum calculates SHA256 checksum of a file
func CalculateFileChecksum(filepath string) (string, error) {
	file, err := os.Open(filepath)
	if err != nil {
		return "", err
	}
	defer func() {
		_ = file.Close()
	}()

	hash := sha256.New()
	if _, err := io.Copy(hash, file); err != nil {
		return "", err
	}

	return fmt.Sprintf("%x", hash.Sum(nil)), nil
}
