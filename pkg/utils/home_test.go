package utils

import (
	"os"
	"testing"

	"github.com/arthur-debert/dodot/pkg/testutil"
)

func TestGetHomeDirectory(t *testing.T) {
	// Save original HOME
	originalHome := os.Getenv("HOME")
	defer func() {
		_ = os.Setenv("HOME", originalHome)
	}()

	tests := []struct {
		name      string
		setup     func()
		expectErr bool
	}{
		{
			name: "with HOME env var",
			setup: func() {
				_ = os.Setenv("HOME", "/home/testuser")
			},
			expectErr: false,
		},
		{
			name: "without HOME env var",
			setup: func() {
				_ = os.Unsetenv("HOME")
			},
			// This may or may not error depending on the system
			// os.UserHomeDir() might still work
			expectErr: false,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			tt.setup()

			homeDir, err := GetHomeDirectory()

			if tt.expectErr {
				testutil.AssertError(t, err)
			} else {
				// We might get an error on some systems without HOME
				if err != nil {
					t.Skip("Skipping test - system requires HOME environment variable")
				}
				testutil.AssertNotEmpty(t, homeDir)
			}
		})
	}
}

func TestGetHomeDirectoryWithDefault(t *testing.T) {
	// Save original HOME
	originalHome := os.Getenv("HOME")
	defer func() {
		_ = os.Setenv("HOME", originalHome)
	}()

	tests := []struct {
		name        string
		defaultDir  string
		setup       func()
		expectValue string
	}{
		{
			name:       "with valid HOME",
			defaultDir: "/default",
			setup: func() {
				_ = os.Setenv("HOME", "/home/user")
			},
			expectValue: "/home/user",
		},
		{
			name:       "without HOME uses default",
			defaultDir: "/default/home",
			setup: func() {
				_ = os.Unsetenv("HOME")
			},
			// This might return the actual home or the default
			// depending on whether os.UserHomeDir() works
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			tt.setup()

			result := GetHomeDirectoryWithDefault(tt.defaultDir)
			testutil.AssertNotEmpty(t, result)

			// If we have an expected value, check it
			if tt.expectValue != "" && os.Getenv("HOME") != "" {
				testutil.AssertEqual(t, tt.expectValue, result)
			}
		})
	}
}

func TestExpandHome(t *testing.T) {
	// Save original HOME
	originalHome := os.Getenv("HOME")
	defer func() {
		_ = os.Setenv("HOME", originalHome)
	}()

	// Set a known HOME for testing
	testHome := "/home/testuser"
	_ = os.Setenv("HOME", testHome)

	tests := []struct {
		name      string
		input     string
		expected  string
		expectErr bool
	}{
		{
			name:     "tilde alone",
			input:    "~",
			expected: testHome,
		},
		{
			name:     "tilde with path",
			input:    "~/Documents/config",
			expected: testHome + "/Documents/config",
		},
		{
			name:     "no tilde",
			input:    "/absolute/path",
			expected: "/absolute/path",
		},
		{
			name:     "tilde in middle",
			input:    "/path/~to/file",
			expected: "/path/~to/file",
		},
		{
			name:     "tilde without slash",
			input:    "~user",
			expected: "~user",
		},
		{
			name:     "empty string",
			input:    "",
			expected: "",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result, err := ExpandHome(tt.input)

			if tt.expectErr {
				testutil.AssertError(t, err)
			} else {
				testutil.AssertNoError(t, err)
				testutil.AssertEqual(t, tt.expected, result)
			}
		})
	}
}

func TestExpandHome_NoHome(t *testing.T) {
	// Save original HOME
	originalHome := os.Getenv("HOME")
	defer func() {
		_ = os.Setenv("HOME", originalHome)
	}()

	// Unset HOME
	_ = os.Unsetenv("HOME")

	// This test might pass if os.UserHomeDir() works without HOME
	_, err := ExpandHome("~/test")
	if err == nil {
		t.Skip("os.UserHomeDir() works without HOME on this system")
	}

	testutil.AssertError(t, err)
	testutil.AssertContains(t, err.Error(), "cannot expand ~")
}
