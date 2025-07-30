package errors

import (
	"errors"
	"fmt"
	"testing"
)

func TestNew(t *testing.T) {
	err := New(ErrNotFound, "file not found")

	if err.Code != ErrNotFound {
		t.Errorf("New() error code = %v, want %v", err.Code, ErrNotFound)
	}

	if err.Message != "file not found" {
		t.Errorf("New() error message = %v, want %v", err.Message, "file not found")
	}

	if err.Details == nil {
		t.Error("New() error details should be initialized")
	}

	expectedStr := "[NOT_FOUND] file not found"
	if err.Error() != expectedStr {
		t.Errorf("Error() = %v, want %v", err.Error(), expectedStr)
	}
}

func TestNewf(t *testing.T) {
	err := Newf(ErrInvalidInput, "invalid value: %s", "test")

	expectedMsg := "invalid value: test"
	if err.Message != expectedMsg {
		t.Errorf("Newf() error message = %v, want %v", err.Message, expectedMsg)
	}
}

func TestWrap(t *testing.T) {
	baseErr := errors.New("base error")

	t.Run("wrap non-nil error", func(t *testing.T) {
		err := Wrap(baseErr, ErrInternal, "internal error")

		if err.Code != ErrInternal {
			t.Errorf("Wrap() error code = %v, want %v", err.Code, ErrInternal)
		}

		if err.Wrapped != baseErr {
			t.Error("Wrap() should preserve the wrapped error")
		}

		expectedStr := "[INTERNAL] internal error: base error"
		if err.Error() != expectedStr {
			t.Errorf("Error() = %v, want %v", err.Error(), expectedStr)
		}
	})

	t.Run("wrap nil error", func(t *testing.T) {
		err := Wrap(nil, ErrInternal, "internal error")
		if err != nil {
			t.Error("Wrap(nil) should return nil")
		}
	})
}

func TestWrapf(t *testing.T) {
	baseErr := errors.New("base error")
	err := Wrapf(baseErr, ErrFileAccess, "cannot access %s", "/path/to/file")

	expectedMsg := "cannot access /path/to/file"
	if err.Message != expectedMsg {
		t.Errorf("Wrapf() error message = %v, want %v", err.Message, expectedMsg)
	}
}

func TestWithDetail(t *testing.T) {
	err := New(ErrNotFound, "not found").
		WithDetail("path", "/test/path").
		WithDetail("type", "file")

	if err.Details["path"] != "/test/path" {
		t.Errorf("WithDetail() path = %v, want %v", err.Details["path"], "/test/path")
	}

	if err.Details["type"] != "file" {
		t.Errorf("WithDetail() type = %v, want %v", err.Details["type"], "file")
	}
}

func TestWithDetails(t *testing.T) {
	details := map[string]interface{}{
		"path": "/test/path",
		"mode": 0644,
		"size": 1024,
	}

	err := New(ErrFileCreate, "cannot create file").WithDetails(details)

	for k, v := range details {
		if err.Details[k] != v {
			t.Errorf("WithDetails() %s = %v, want %v", k, err.Details[k], v)
		}
	}
}

func TestUnwrap(t *testing.T) {
	baseErr := errors.New("base error")
	err := Wrap(baseErr, ErrInternal, "wrapped error")

	unwrapped := err.Unwrap()
	if unwrapped != baseErr {
		t.Error("Unwrap() should return the wrapped error")
	}
}

func TestIs(t *testing.T) {
	err1 := New(ErrNotFound, "error 1")
	err2 := New(ErrNotFound, "error 2")
	err3 := New(ErrInternal, "error 3")

	if !err1.Is(err2) {
		t.Error("Is() should return true for errors with same code")
	}

	if err1.Is(err3) {
		t.Error("Is() should return false for errors with different codes")
	}

	// Test with standard errors.Is
	if !errors.Is(err1, err2) {
		t.Error("errors.Is() should work with DodotError")
	}
}

func TestIsErrorCode(t *testing.T) {
	tests := []struct {
		name     string
		err      error
		code     ErrorCode
		expected bool
	}{
		{
			name:     "matching code",
			err:      New(ErrNotFound, "not found"),
			code:     ErrNotFound,
			expected: true,
		},
		{
			name:     "different code",
			err:      New(ErrNotFound, "not found"),
			code:     ErrInternal,
			expected: false,
		},
		{
			name:     "wrapped error",
			err:      Wrap(errors.New("base"), ErrFileAccess, "access denied"),
			code:     ErrFileAccess,
			expected: true,
		},
		{
			name:     "non-dodot error",
			err:      errors.New("standard error"),
			code:     ErrNotFound,
			expected: false,
		},
		{
			name:     "nil error",
			err:      nil,
			code:     ErrNotFound,
			expected: false,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if got := IsErrorCode(tt.err, tt.code); got != tt.expected {
				t.Errorf("IsErrorCode() = %v, want %v", got, tt.expected)
			}
		})
	}
}

func TestGetErrorCode(t *testing.T) {
	tests := []struct {
		name     string
		err      error
		expected ErrorCode
	}{
		{
			name:     "dodot error",
			err:      New(ErrPackNotFound, "pack not found"),
			expected: ErrPackNotFound,
		},
		{
			name:     "standard error",
			err:      errors.New("standard error"),
			expected: ErrUnknown,
		},
		{
			name:     "nil error",
			err:      nil,
			expected: ErrUnknown,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if got := GetErrorCode(tt.err); got != tt.expected {
				t.Errorf("GetErrorCode() = %v, want %v", got, tt.expected)
			}
		})
	}
}

func TestGetErrorDetails(t *testing.T) {
	t.Run("dodot error with details", func(t *testing.T) {
		err := New(ErrFileCreate, "cannot create").
			WithDetail("path", "/test").
			WithDetail("mode", 0644)

		details := GetErrorDetails(err)
		if details == nil {
			t.Fatal("GetErrorDetails() returned nil")
		}

		if details["path"] != "/test" {
			t.Errorf("GetErrorDetails() path = %v, want %v", details["path"], "/test")
		}
	})

	t.Run("standard error", func(t *testing.T) {
		err := errors.New("standard error")
		details := GetErrorDetails(err)
		if details != nil {
			t.Error("GetErrorDetails() should return nil for non-dodot errors")
		}
	})
}

// Example of how to use in tests
func TestErrorUsageExample(t *testing.T) {
	// Simulate a function that returns an error
	doSomething := func() error {
		return Newf(ErrFileNotFound, "cannot find file %s", "config.toml").
			WithDetail("cwd", "/home/user").
			WithDetail("searchPaths", []string{"/etc", "/home/user/.config"})
	}

	err := doSomething()

	// Check error code in tests
	if !IsErrorCode(err, ErrFileNotFound) {
		t.Errorf("Expected error code %v, got %v", ErrFileNotFound, GetErrorCode(err))
	}

	// Access error details
	details := GetErrorDetails(err)
	if details["cwd"] != "/home/user" {
		t.Error("Error details missing expected cwd")
	}
}

func TestErrorChaining(t *testing.T) {
	// Create a chain of errors
	err1 := errors.New("root cause")
	err2 := Wrap(err1, ErrFileAccess, "cannot read file")
	err3 := Wrap(err2, ErrConfigLoad, "failed to load config")

	// Check the chain
	if !IsErrorCode(err3, ErrConfigLoad) {
		t.Error("Top level error should have ErrConfigLoad code")
	}

	// Unwrap to get the middle error
	var dodotErr *DodotError
	if errors.As(err3.Unwrap(), &dodotErr) {
		if !IsErrorCode(dodotErr, ErrFileAccess) {
			t.Error("Middle error should have ErrFileAccess code")
		}
	}

	// Check if we can find the root cause
	if !errors.Is(err3, err1) {
		t.Error("Should be able to find root cause with errors.Is")
	}
}

func ExampleNew() {
	err := New(ErrNotFound, "configuration file not found").
		WithDetail("path", "/etc/dodot/config.toml").
		WithDetail("searchPaths", []string{"/etc/dodot", "~/.config/dodot"})

	fmt.Println(err.Error())
	// Output: [NOT_FOUND] configuration file not found
}

func ExampleWrap() {
	// Simulate a low-level error
	fsErr := errors.New("permission denied")

	// Wrap it with context
	err := Wrap(fsErr, ErrFileAccess, "cannot read configuration").
		WithDetail("path", "/etc/dodot/config.toml").
		WithDetail("user", "dodot")

	fmt.Println(err.Error())
	// Output: [FILE_ACCESS] cannot read configuration: permission denied
}
