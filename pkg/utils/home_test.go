package utils

import (
	"os"
	"strings"
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
			// This should test the actual behavior - does our system work without HOME?
			expectErr: false, // We'll check the actual result in the test
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			tt.setup()

			homeDir, err := GetHomeDirectory()

			if tt.expectErr {
				testutil.AssertError(t, err)
			} else if tt.name == "without HOME env var" {
				// For the no-HOME test, we should test both cases:
				// 1. If the system works without HOME, ensure we get a valid path
				// 2. If the system fails without HOME, ensure we get an appropriate error
				if err != nil {
					// This is expected on some systems - ensure it's a reasonable error
					testutil.AssertContains(t, err.Error(), "home")
					t.Logf("System requires HOME environment variable (error: %v)", err)
				} else {
					// System works without HOME - ensure we get a valid directory
					testutil.AssertNotEmpty(t, homeDir)
					t.Logf("System works without HOME, returned: %s", homeDir)
				}
			} else {
				testutil.AssertNoError(t, err)
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
		if originalHome != "" {
			_ = os.Setenv("HOME", originalHome)
		} else {
			_ = os.Unsetenv("HOME")
		}
	}()

	// Unset HOME
	_ = os.Unsetenv("HOME")

	// Test expansion with no HOME environment variable
	result, err := ExpandHome("~/test")

	if err == nil {
		// System works without HOME - verify we got a reasonable result
		testutil.AssertNotEmpty(t, result)
		testutil.AssertTrue(t, !strings.HasPrefix(result, "~"),
			"Tilde should be expanded even without HOME: %s", result)
		t.Logf("System expanded ~ without HOME to: %s", result)
	} else {
		// System requires HOME - verify we get an appropriate error
		testutil.AssertContains(t, err.Error(), "cannot expand ~")
		t.Logf("System requires HOME environment variable (error: %v)", err)
	}
}
