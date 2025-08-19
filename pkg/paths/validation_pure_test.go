package paths

import (
	"path/filepath"
	"strings"
	"testing"

	"github.com/stretchr/testify/assert"
	"github.com/stretchr/testify/require"
)

func TestValidatePath(t *testing.T) {
	tests := []struct {
		name        string
		path        string
		wantErr     bool
		errContains string
	}{
		{
			name:        "empty path",
			path:        "",
			wantErr:     true,
			errContains: "path cannot be empty",
		},
		{
			name:    "valid path",
			path:    "/home/user/file.txt",
			wantErr: false,
		},
		{
			name:        "path with null bytes",
			path:        "/home/user\x00/file.txt",
			wantErr:     true,
			errContains: "null bytes",
		},
		{
			name:        "excessively long path",
			path:        "/" + strings.Repeat("a", 4097),
			wantErr:     true,
			errContains: "exceeds maximum length",
		},
		{
			name:    "path at max length",
			path:    "/" + strings.Repeat("a", 4095),
			wantErr: false,
		},
		{
			name:    "relative path",
			path:    "relative/path/file.txt",
			wantErr: false,
		},
		{
			name:    "path with special chars",
			path:    "/home/user-name_123/file.txt",
			wantErr: false,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			err := ValidatePath(tt.path)
			if tt.wantErr {
				require.Error(t, err)
				if tt.errContains != "" {
					assert.Contains(t, err.Error(), tt.errContains)
				}
			} else {
				require.NoError(t, err)
			}
		})
	}
}

func TestIsAbsolutePath(t *testing.T) {
	tests := []struct {
		name    string
		path    string
		wantAbs bool
	}{
		{
			name:    "unix absolute path",
			path:    "/home/user",
			wantAbs: true,
		},
		{
			name:    "unix relative path",
			path:    "home/user",
			wantAbs: false,
		},
		{
			name:    "empty path",
			path:    "",
			wantAbs: false,
		},
		{
			name:    "dot path",
			path:    ".",
			wantAbs: false,
		},
		{
			name:    "double dot path",
			path:    "..",
			wantAbs: false,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got := IsAbsolutePath(tt.path)
			assert.Equal(t, tt.wantAbs, got)
		})
	}
}

func TestPathDepth(t *testing.T) {
	tests := []struct {
		name      string
		path      string
		wantDepth int
	}{
		{
			name:      "root path",
			path:      "/",
			wantDepth: 0,
		},
		{
			name:      "single level",
			path:      "/home",
			wantDepth: 0,
		},
		{
			name:      "two levels",
			path:      "/home/user",
			wantDepth: 1,
		},
		{
			name:      "three levels",
			path:      "/home/user/docs",
			wantDepth: 2,
		},
		{
			name:      "relative single level",
			path:      "folder",
			wantDepth: 0,
		},
		{
			name:      "relative two levels",
			path:      "folder/subfolder",
			wantDepth: 1,
		},
		{
			name:      "current directory",
			path:      ".",
			wantDepth: 0,
		},
		{
			name:      "parent directory",
			path:      "..",
			wantDepth: 0,
		},
		{
			name:      "path with trailing slash",
			path:      "/home/user/",
			wantDepth: 1,
		},
		{
			name:      "path with multiple slashes",
			path:      "/home//user///docs",
			wantDepth: 2,
		},
		{
			name:      "empty path",
			path:      "",
			wantDepth: 0, // Sanitizes to "."
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got := PathDepth(tt.path)
			assert.Equal(t, tt.wantDepth, got, "PathDepth(%q)", tt.path)
		})
	}
}

func TestCommonPrefix(t *testing.T) {
	tests := []struct {
		name       string
		paths      []string
		wantPrefix string
	}{
		{
			name:       "no paths",
			paths:      []string{},
			wantPrefix: "",
		},
		{
			name:       "single path",
			paths:      []string{"/home/user/docs"},
			wantPrefix: "/home/user/docs",
		},
		{
			name:       "identical paths",
			paths:      []string{"/home/user", "/home/user"},
			wantPrefix: "/home/user",
		},
		{
			name:       "common parent",
			paths:      []string{"/home/user/docs", "/home/user/pics"},
			wantPrefix: "/home/user",
		},
		{
			name:       "common grandparent",
			paths:      []string{"/home/user/docs", "/home/admin/docs"},
			wantPrefix: "/home",
		},
		{
			name:       "root common",
			paths:      []string{"/home/user", "/etc/config"},
			wantPrefix: "/",
		},
		{
			name:       "no common prefix",
			paths:      []string{"home/user", "etc/config"},
			wantPrefix: "",
		},
		{
			name:       "multiple paths",
			paths:      []string{"/usr/local/bin", "/usr/local/lib", "/usr/local/share"},
			wantPrefix: "/usr/local",
		},
		{
			name:       "relative paths with common prefix",
			paths:      []string{"home/user/docs", "home/user/pics", "home/user/music"},
			wantPrefix: "home/user",
		},
		{
			name:       "mixed absolute and relative",
			paths:      []string{"/home/user", "home/user"},
			wantPrefix: "",
		},
		{
			name:       "paths with trailing slashes",
			paths:      []string{"/home/user/", "/home/admin/"},
			wantPrefix: "/home",
		},
		{
			name:       "single component paths",
			paths:      []string{"/home", "/user"},
			wantPrefix: "/",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got := CommonPrefix(tt.paths...)
			assert.Equal(t, tt.wantPrefix, got)
		})
	}
}

func TestIsHiddenPath(t *testing.T) {
	tests := []struct {
		name       string
		path       string
		wantHidden bool
	}{
		{
			name:       "hidden file",
			path:       ".gitignore",
			wantHidden: true,
		},
		{
			name:       "hidden directory",
			path:       ".git",
			wantHidden: true,
		},
		{
			name:       "hidden file in directory",
			path:       "/home/user/.bashrc",
			wantHidden: true,
		},
		{
			name:       "visible file",
			path:       "README.md",
			wantHidden: false,
		},
		{
			name:       "visible file in hidden directory",
			path:       ".config/settings.toml",
			wantHidden: false, // Only checks basename
		},
		{
			name:       "empty path",
			path:       "",
			wantHidden: true, // filepath.Base("") returns "."
		},
		{
			name:       "current directory",
			path:       ".",
			wantHidden: true,
		},
		{
			name:       "parent directory",
			path:       "..",
			wantHidden: true,
		},
		{
			name:       "file starting with dot in middle",
			path:       "file.txt",
			wantHidden: false,
		},
		{
			name:       "absolute path to hidden file",
			path:       "/home/user/.ssh/config",
			wantHidden: false, // "config" is not hidden
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got := IsHiddenPath(tt.path)
			assert.Equal(t, tt.wantHidden, got)
		})
	}
}

func TestValidatePathSecurity(t *testing.T) {
	tests := []struct {
		name        string
		path        string
		wantErr     bool
		errContains string
	}{
		{
			name:    "safe path",
			path:    "/home/user/documents/file.txt",
			wantErr: false,
		},
		{
			name:        "empty path",
			path:        "",
			wantErr:     true,
			errContains: "path cannot be empty",
		},
		{
			name:        "path traversal with ../",
			path:        "/home/user/../admin/secrets",
			wantErr:     true,
			errContains: "parent directory references",
		},
		{
			name:        "right-to-left override",
			path:        "/home/user/\u202efile.txt",
			wantErr:     true,
			errContains: "suspicious Unicode characters",
		},
		{
			name:        "zero-width space",
			path:        "/home/user/\u200bfile.txt",
			wantErr:     true,
			errContains: "suspicious Unicode characters",
		},
		{
			name:        "soft hyphen",
			path:        "/home/user/\u00adfile.txt",
			wantErr:     true,
			errContains: "suspicious Unicode characters",
		},
		{
			name:        "null bytes",
			path:        "/home/user\x00/file.txt",
			wantErr:     true,
			errContains: "null bytes",
		},
		{
			name:    "unicode normal characters",
			path:    "/home/user/文档/file.txt",
			wantErr: false,
		},
		{
			name:    "path with spaces",
			path:    "/home/user/my documents/file.txt",
			wantErr: false,
		},
		{
			name:        "multiple suspicious characters",
			path:        "/home/\u202euser\u200b/file\u00ad.txt",
			wantErr:     true,
			errContains: "suspicious Unicode characters",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			err := ValidatePathSecurity(tt.path)
			if tt.wantErr {
				require.Error(t, err)
				if tt.errContains != "" {
					assert.Contains(t, err.Error(), tt.errContains)
				}
			} else {
				require.NoError(t, err)
			}
		})
	}
}

func TestCommonPrefix_EdgeCases(t *testing.T) {
	// Test behavior with normalized paths
	tests := []struct {
		name       string
		paths      []string
		wantPrefix string
	}{
		{
			name:       "paths needing normalization",
			paths:      []string{"/home/./user/../user/docs", "/home/user/pics"},
			wantPrefix: "/home/user",
		},
		{
			name:       "all paths are parent references",
			paths:      []string{"..", "..", ".."},
			wantPrefix: "..",
		},
		{
			name:       "mix of . and regular paths",
			paths:      []string{"./home/user", "./home/admin"},
			wantPrefix: "home",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got := CommonPrefix(tt.paths...)
			assert.Equal(t, tt.wantPrefix, got)
		})
	}
}

// TestMustValidatePath tests the panic behavior
func TestMustValidatePath(t *testing.T) {
	t.Run("valid path does not panic", func(t *testing.T) {
		assert.NotPanics(t, func() {
			MustValidatePath("/home/user/file.txt")
		})
	})

	t.Run("invalid path panics", func(t *testing.T) {
		assert.Panics(t, func() {
			MustValidatePath("")
		})
	})

	t.Run("path with null bytes panics", func(t *testing.T) {
		assert.Panics(t, func() {
			MustValidatePath("/home/user\x00/file.txt")
		})
	})
}

// TestSanitizePath tests path sanitization
func TestSanitizePath(t *testing.T) {
	tests := []struct {
		name     string
		input    string
		expected string
	}{
		{
			name:     "clean path unchanged",
			input:    "/home/user/file.txt",
			expected: "/home/user/file.txt",
		},
		{
			name:     "remove trailing slash",
			input:    "/home/user/",
			expected: "/home/user",
		},
		{
			name:     "resolve current directory",
			input:    "/home/./user",
			expected: "/home/user",
		},
		{
			name:     "resolve parent directory",
			input:    "/home/user/../admin",
			expected: "/home/admin",
		},
		{
			name:     "multiple slashes",
			input:    "/home//user///file.txt",
			expected: "/home/user/file.txt",
		},
		{
			name:     "empty path becomes dot",
			input:    "",
			expected: ".",
		},
		{
			name:     "dot path unchanged",
			input:    ".",
			expected: ".",
		},
		{
			name:     "complex path",
			input:    "/home/user/./docs/../pics/",
			expected: "/home/user/pics",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got := SanitizePath(tt.input)
			assert.Equal(t, tt.expected, got)
		})
	}
}

// TestJoinPaths tests safe path joining
func TestJoinPaths(t *testing.T) {
	tests := []struct {
		name        string
		elements    []string
		wantPath    string
		wantErr     bool
		errContains string
	}{
		{
			name:     "simple join",
			elements: []string{"/home", "user", "file.txt"},
			wantPath: filepath.Join("/home", "user", "file.txt"),
			wantErr:  false,
		},
		{
			name:     "empty elements",
			elements: []string{},
			wantPath: "",
			wantErr:  false,
		},
		{
			name:     "single element",
			elements: []string{"/home"},
			wantPath: "/home",
			wantErr:  false,
		},
		{
			name:        "null byte in element",
			elements:    []string{"/home", "user\x00", "file.txt"},
			wantErr:     true,
			errContains: "null bytes",
		},
		{
			name:     "elements with dots",
			elements: []string{"/home", ".", "user", "..", "admin"},
			wantPath: "/home/admin",
			wantErr:  false,
		},
		{
			name:     "trailing slashes",
			elements: []string{"/home/", "user/", "file.txt"},
			wantPath: "/home/user/file.txt",
			wantErr:  false,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got, err := JoinPaths(tt.elements...)
			if tt.wantErr {
				require.Error(t, err)
				if tt.errContains != "" {
					assert.Contains(t, err.Error(), tt.errContains)
				}
			} else {
				require.NoError(t, err)
				assert.Equal(t, tt.wantPath, got)
			}
		})
	}
}
