// Test Type: Unit Test
// Description: Tests for the paths package - path validation functions

package paths_test

import (
	"strings"
	"testing"

	"github.com/arthur-debert/dodot/pkg/errors"
	"github.com/arthur-debert/dodot/pkg/paths"
	"github.com/stretchr/testify/assert"
)

func TestValidatePath(t *testing.T) {
	tests := []struct {
		name        string
		path        string
		expectError bool
		errorCode   errors.ErrorCode
	}{
		{
			name:        "valid_path",
			path:        "/home/user/dotfiles",
			expectError: false,
		},
		{
			name:        "empty_path",
			path:        "",
			expectError: true,
			errorCode:   errors.ErrInvalidInput,
		},
		{
			name:        "path_with_null_bytes",
			path:        "/path/with\x00null",
			expectError: true,
			errorCode:   errors.ErrInvalidInput,
		},
		{
			name:        "excessive_length_path",
			path:        "/" + strings.Repeat("a", 4096),
			expectError: true,
			errorCode:   errors.ErrInvalidInput,
		},
		{
			name:        "relative_path",
			path:        "relative/path",
			expectError: false,
		},
		{
			name:        "path_with_spaces",
			path:        "/path with spaces/file.txt",
			expectError: false,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			err := paths.ValidatePath(tt.path)

			if tt.expectError {
				assert.Error(t, err)
				if terr, ok := err.(*errors.DodotError); ok {
					assert.Equal(t, tt.errorCode, terr.Code)
				}
			} else {
				assert.NoError(t, err)
			}
		})
	}
}

func TestValidatePackName(t *testing.T) {
	tests := []struct {
		name        string
		packName    string
		expectError bool
		errorCode   errors.ErrorCode
	}{
		{
			name:        "valid_pack_name",
			packName:    "vim",
			expectError: false,
		},
		{
			name:        "empty_pack_name",
			packName:    "",
			expectError: true,
			errorCode:   errors.ErrInvalidInput,
		},
		{
			name:        "pack_name_with_slash",
			packName:    "vim/config",
			expectError: true,
			errorCode:   errors.ErrInvalidInput,
		},
		{
			name:        "pack_name_with_backslash",
			packName:    "vim\\config",
			expectError: true,
			errorCode:   errors.ErrInvalidInput,
		},
		{
			name:        "dot_reserved_name",
			packName:    ".",
			expectError: true,
			errorCode:   errors.ErrInvalidInput,
		},
		{
			name:        "dotdot_reserved_name",
			packName:    "..",
			expectError: true,
			errorCode:   errors.ErrInvalidInput,
		},
		{
			name:        "pack_name_with_colon",
			packName:    "vim:config",
			expectError: true,
			errorCode:   errors.ErrInvalidInput,
		},
		{
			name:        "pack_name_with_asterisk",
			packName:    "vim*",
			expectError: true,
			errorCode:   errors.ErrInvalidInput,
		},
		{
			name:        "pack_name_with_control_char",
			packName:    "vim\x01",
			expectError: true,
			errorCode:   errors.ErrInvalidInput,
		},
		{
			name:        "numeric_pack_name",
			packName:    "123valid",
			expectError: false,
		},
		{
			name:        "pack_name_with_dash",
			packName:    "my-pack",
			expectError: false,
		},
		{
			name:        "pack_name_with_underscore",
			packName:    "my_pack",
			expectError: false,
		},
		{
			name:        "pack_name_with_dot",
			packName:    "my.pack",
			expectError: false,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			err := paths.ValidatePackName(tt.packName)

			if tt.expectError {
				assert.Error(t, err)
				if terr, ok := err.(*errors.DodotError); ok {
					assert.Equal(t, tt.errorCode, terr.Code)
				}
			} else {
				assert.NoError(t, err)
			}
		})
	}
}

func TestSanitizePath(t *testing.T) {
	tests := []struct {
		name     string
		path     string
		expected string
	}{
		{
			name:     "already_clean",
			path:     "/home/user/dotfiles",
			expected: "/home/user/dotfiles",
		},
		{
			name:     "trailing_slash",
			path:     "/home/user/dotfiles/",
			expected: "/home/user/dotfiles",
		},
		{
			name:     "multiple_slashes",
			path:     "/home//user///dotfiles",
			expected: "/home/user/dotfiles",
		},
		{
			name:     "dot_segments",
			path:     "/home/./user/../user/dotfiles",
			expected: "/home/user/dotfiles",
		},
		{
			name:     "empty_path",
			path:     "",
			expected: ".",
		},
		{
			name:     "relative_path",
			path:     "vim/config",
			expected: "vim/config",
		},
		{
			name:     "root_path",
			path:     "/",
			expected: "/",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result := paths.SanitizePath(tt.path)
			assert.Equal(t, tt.expected, result)
		})
	}
}

func TestIsAbsolutePath(t *testing.T) {
	tests := []struct {
		name     string
		path     string
		expected bool
	}{
		{
			name:     "absolute_unix_path",
			path:     "/home/user/dotfiles",
			expected: true,
		},
		{
			name:     "relative_path",
			path:     "home/user/dotfiles",
			expected: false,
		},
		{
			name:     "empty_path",
			path:     "",
			expected: false,
		},
		{
			name:     "dot_path",
			path:     ".",
			expected: false,
		},
		{
			name:     "root_path",
			path:     "/",
			expected: true,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result := paths.IsAbsolutePath(tt.path)
			assert.Equal(t, tt.expected, result)
		})
	}
}

func TestJoinPaths(t *testing.T) {
	tests := []struct {
		name        string
		elements    []string
		expected    string
		expectError bool
	}{
		{
			name:     "simple_join",
			elements: []string{"/home", "user", "dotfiles"},
			expected: "/home/user/dotfiles",
		},
		{
			name:     "join_with_trailing_slash",
			elements: []string{"/home/", "user", "dotfiles"},
			expected: "/home/user/dotfiles",
		},
		{
			name:     "empty_elements",
			elements: []string{},
			expected: "",
		},
		{
			name:     "single_element",
			elements: []string{"/home"},
			expected: "/home",
		},
		{
			name:     "relative_join",
			elements: []string{"vim", "config"},
			expected: "vim/config",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result, err := paths.JoinPaths(tt.elements...)

			if tt.expectError {
				assert.Error(t, err)
			} else {
				assert.NoError(t, err)
				assert.Equal(t, tt.expected, result)
			}
		})
	}
}

func TestContainsPath(t *testing.T) {
	tests := []struct {
		name     string
		parent   string
		child    string
		expected bool
	}{
		{
			name:     "child_is_contained",
			parent:   "/home/user/dotfiles",
			child:    "/home/user/dotfiles/vim/vimrc",
			expected: true,
		},
		{
			name:     "child_not_contained",
			parent:   "/home/user/dotfiles",
			child:    "/home/other/files",
			expected: false,
		},
		{
			name:     "same_path",
			parent:   "/home/user/dotfiles",
			child:    "/home/user/dotfiles",
			expected: true,
		},
		{
			name:     "parent_shorter_than_child_prefix",
			parent:   "/home",
			child:    "/home/user/dotfiles",
			expected: true,
		},
		{
			name:     "similar_prefix_but_not_contained",
			parent:   "/home/user/dotfiles",
			child:    "/home/user/dotfiles2",
			expected: false,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result := paths.ContainsPath(tt.parent, tt.child)
			assert.Equal(t, tt.expected, result)
		})
	}
}

func TestSplitPath(t *testing.T) {
	tests := []struct {
		name         string
		path         string
		expectedDir  string
		expectedFile string
	}{
		{
			name:         "standard_path",
			path:         "/home/user/file.txt",
			expectedDir:  "/home/user",
			expectedFile: "file.txt",
		},
		{
			name:         "path_with_trailing_slash",
			path:         "/home/user/",
			expectedDir:  "/home/user",
			expectedFile: "",
		},
		{
			name:         "filename_only",
			path:         "file.txt",
			expectedDir:  "",
			expectedFile: "file.txt",
		},
		{
			name:         "root_path",
			path:         "/",
			expectedDir:  "/",
			expectedFile: "",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			dir, file := paths.SplitPath(tt.path)
			assert.Equal(t, tt.expectedDir, dir)
			assert.Equal(t, tt.expectedFile, file)
		})
	}
}

func TestIsHiddenPath(t *testing.T) {
	tests := []struct {
		name     string
		path     string
		expected bool
	}{
		{
			name:     "hidden_file",
			path:     ".vimrc",
			expected: true,
		},
		{
			name:     "normal_file",
			path:     "vimrc",
			expected: false,
		},
		{
			name:     "hidden_in_path",
			path:     "/home/.config/vim/vimrc",
			expected: true,
		},
		{
			name:     "dot_directory",
			path:     ".",
			expected: false,
		},
		{
			name:     "dotdot_directory",
			path:     "..",
			expected: false,
		},
		{
			name:     "empty_path",
			path:     "",
			expected: false,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result := paths.IsHiddenPath(tt.path)
			assert.Equal(t, tt.expected, result)
		})
	}
}

func TestPathDepth(t *testing.T) {
	tests := []struct {
		name     string
		path     string
		expected int
	}{
		{
			name:     "root_path",
			path:     "/",
			expected: 0,
		},
		{
			name:     "single_level",
			path:     "/home",
			expected: 1,
		},
		{
			name:     "multiple_levels",
			path:     "/home/user/dotfiles/vim",
			expected: 4,
		},
		{
			name:     "relative_path",
			path:     "vim/config",
			expected: 2,
		},
		{
			name:     "empty_path",
			path:     "",
			expected: 0,
		},
		{
			name:     "trailing_slash",
			path:     "/home/user/",
			expected: 2,
		},
		{
			name:     "dot_path",
			path:     ".",
			expected: 1,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result := paths.PathDepth(tt.path)
			assert.Equal(t, tt.expected, result)
		})
	}
}

func TestCommonPrefix(t *testing.T) {
	tests := []struct {
		name     string
		paths    []string
		expected string
	}{
		{
			name: "common_prefix",
			paths: []string{
				"/home/user/dotfiles/vim",
				"/home/user/dotfiles/bash",
				"/home/user/dotfiles/git",
			},
			expected: "/home/user/dotfiles",
		},
		{
			name: "no_common_prefix",
			paths: []string{
				"/home/user",
				"/var/log",
				"/etc/config",
			},
			expected: "",
		},
		{
			name:     "empty_paths",
			paths:    []string{},
			expected: "",
		},
		{
			name:     "single_path",
			paths:    []string{"/home/user/dotfiles"},
			expected: "/home/user/dotfiles",
		},
		{
			name: "root_common",
			paths: []string{
				"/home",
				"/usr",
				"/var",
			},
			expected: "",
		},
		{
			name: "one_path_is_prefix",
			paths: []string{
				"/home/user",
				"/home/user/dotfiles",
				"/home/user/documents",
			},
			expected: "/home/user",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result := paths.CommonPrefix(tt.paths...)
			assert.Equal(t, tt.expected, result)
		})
	}
}

func TestValidatePathSecurity(t *testing.T) {
	tests := []struct {
		name        string
		path        string
		expectError bool
		errorCode   errors.ErrorCode
	}{
		{
			name:        "safe_path",
			path:        "/home/user/dotfiles",
			expectError: false,
		},
		{
			name:        "path_traversal",
			path:        "../../../etc/passwd",
			expectError: true,
			errorCode:   errors.ErrInvalidInput,
		},
		{
			name:        "unicode_rtl_override",
			path:        "file\u202Etxt.sh",
			expectError: true,
			errorCode:   errors.ErrInvalidInput,
		},
		{
			name:        "null_byte",
			path:        "file\x00.txt",
			expectError: true,
			errorCode:   errors.ErrInvalidInput,
		},
		{
			name:        "zero_width_space",
			path:        "file\u200B.txt",
			expectError: true,
			errorCode:   errors.ErrInvalidInput,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			err := paths.ValidatePathSecurity(tt.path)

			if tt.expectError {
				assert.Error(t, err)
				if terr, ok := err.(*errors.DodotError); ok {
					assert.Equal(t, tt.errorCode, terr.Code)
				}
			} else {
				assert.NoError(t, err)
			}
		})
	}
}

func TestMustValidatePath(t *testing.T) {
	t.Run("valid_path_no_panic", func(t *testing.T) {
		assert.NotPanics(t, func() {
			paths.MustValidatePath("/home/user/dotfiles")
		})
	})

	t.Run("invalid_path_panics", func(t *testing.T) {
		assert.Panics(t, func() {
			paths.MustValidatePath("")
		})
	})
}

func TestRelativePath(t *testing.T) {
	tests := []struct {
		name        string
		base        string
		target      string
		expected    string
		expectError bool
	}{
		{
			name:     "simple_relative",
			base:     "/home/user",
			target:   "/home/user/dotfiles/vim",
			expected: "dotfiles/vim",
		},
		{
			name:     "same_path",
			base:     "/home/user/dotfiles",
			target:   "/home/user/dotfiles",
			expected: ".",
		},
		{
			name:     "parent_directory",
			base:     "/home/user/dotfiles/vim",
			target:   "/home/user",
			expected: "../..",
		},
		{
			name:     "sibling_directory",
			base:     "/home/user/dotfiles",
			target:   "/home/user/documents",
			expected: "../documents",
		},
		{
			name:        "different_roots",
			base:        "/home/user",
			target:      "C:/Users/user",
			expected:    "",
			expectError: true,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result, err := paths.RelativePath(tt.base, tt.target)

			if tt.expectError {
				assert.Error(t, err)
			} else {
				assert.NoError(t, err)
				assert.Equal(t, tt.expected, result)
			}
		})
	}
}
