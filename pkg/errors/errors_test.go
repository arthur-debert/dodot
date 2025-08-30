// pkg/errors/errors_test.go
// TEST TYPE: Unit Test
// DEPENDENCIES: None
// PURPOSE: Test error creation, wrapping, and utility functions

package errors_test

import (
	stderrors "errors"
	"testing"

	"github.com/arthur-debert/dodot/pkg/errors"
)

func TestNew(t *testing.T) {
	tests := []struct {
		name    string
		code    errors.ErrorCode
		message string
		wantStr string
	}{
		{
			name:    "not_found_error",
			code:    errors.ErrNotFound,
			message: "file not found",
			wantStr: "[NOT_FOUND] file not found",
		},
		{
			name:    "invalid_input_error",
			code:    errors.ErrInvalidInput,
			message: "invalid configuration",
			wantStr: "[INVALID_INPUT] invalid configuration",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			err := errors.New(tt.code, tt.message)

			if err.Code != tt.code {
				t.Errorf("New() code = %v, want %v", err.Code, tt.code)
			}

			if err.Message != tt.message {
				t.Errorf("New() message = %q, want %q", err.Message, tt.message)
			}

			if err.Details == nil {
				t.Error("New() details should be initialized")
			}

			if got := err.Error(); got != tt.wantStr {
				t.Errorf("Error() = %q, want %q", got, tt.wantStr)
			}
		})
	}
}

func TestNewf(t *testing.T) {
	tests := []struct {
		name    string
		code    errors.ErrorCode
		format  string
		args    []interface{}
		wantMsg string
	}{
		{
			name:    "format_with_string",
			code:    errors.ErrInvalidInput,
			format:  "invalid value: %s",
			args:    []interface{}{"test"},
			wantMsg: "invalid value: test",
		},
		{
			name:    "format_with_multiple_args",
			code:    errors.ErrFileCreate,
			format:  "cannot create %s with mode %o",
			args:    []interface{}{"file.txt", 0644},
			wantMsg: "cannot create file.txt with mode 644",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			err := errors.Newf(tt.code, tt.format, tt.args...)

			if err.Message != tt.wantMsg {
				t.Errorf("Newf() message = %q, want %q", err.Message, tt.wantMsg)
			}
		})
	}
}

func TestWrap(t *testing.T) {
	baseErr := stderrors.New("base error")

	t.Run("wrap_non_nil_error", func(t *testing.T) {
		err := errors.Wrap(baseErr, errors.ErrInternal, "internal error")

		if err.Code != errors.ErrInternal {
			t.Errorf("Wrap() code = %v, want %v", err.Code, errors.ErrInternal)
		}

		if err.Wrapped != baseErr {
			t.Error("Wrap() should preserve wrapped error")
		}

		wantStr := "[INTERNAL] internal error: base error"
		if got := err.Error(); got != wantStr {
			t.Errorf("Error() = %q, want %q", got, wantStr)
		}
	})

	t.Run("wrap_nil_error_returns_nil", func(t *testing.T) {
		err := errors.Wrap(nil, errors.ErrInternal, "internal error")
		if err != nil {
			t.Error("Wrap(nil) should return nil")
		}
	})
}

func TestWithDetail(t *testing.T) {
	err := errors.New(errors.ErrNotFound, "not found").
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

	err := errors.New(errors.ErrFileCreate, "cannot create file").
		WithDetails(details)

	for k, v := range details {
		if err.Details[k] != v {
			t.Errorf("WithDetails() %s = %v, want %v", k, err.Details[k], v)
		}
	}
}

func TestIs(t *testing.T) {
	err1 := errors.New(errors.ErrNotFound, "error 1")
	err2 := errors.New(errors.ErrNotFound, "error 2")
	err3 := errors.New(errors.ErrInternal, "error 3")

	t.Run("same_code_is_equal", func(t *testing.T) {
		if !err1.Is(err2) {
			t.Error("Is() should return true for same code")
		}
	})

	t.Run("different_code_not_equal", func(t *testing.T) {
		if err1.Is(err3) {
			t.Error("Is() should return false for different codes")
		}
	})

	t.Run("works_with_errors_Is", func(t *testing.T) {
		if !stderrors.Is(err1, err2) {
			t.Error("errors.Is() should work with DodotError")
		}
	})
}

func TestIsErrorCode(t *testing.T) {
	tests := []struct {
		name     string
		err      error
		code     errors.ErrorCode
		expected bool
	}{
		{
			name:     "matching_code",
			err:      errors.New(errors.ErrNotFound, "not found"),
			code:     errors.ErrNotFound,
			expected: true,
		},
		{
			name:     "different_code",
			err:      errors.New(errors.ErrNotFound, "not found"),
			code:     errors.ErrInternal,
			expected: false,
		},
		{
			name:     "wrapped_error",
			err:      errors.Wrap(stderrors.New("base"), errors.ErrFileAccess, "denied"),
			code:     errors.ErrFileAccess,
			expected: true,
		},
		{
			name:     "non_dodot_error",
			err:      stderrors.New("standard error"),
			code:     errors.ErrNotFound,
			expected: false,
		},
		{
			name:     "nil_error",
			err:      nil,
			code:     errors.ErrNotFound,
			expected: false,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if got := errors.IsErrorCode(tt.err, tt.code); got != tt.expected {
				t.Errorf("IsErrorCode() = %v, want %v", got, tt.expected)
			}
		})
	}
}

func TestGetErrorCode(t *testing.T) {
	tests := []struct {
		name     string
		err      error
		expected errors.ErrorCode
	}{
		{
			name:     "dodot_error",
			err:      errors.New(errors.ErrPackNotFound, "pack not found"),
			expected: errors.ErrPackNotFound,
		},
		{
			name:     "standard_error",
			err:      stderrors.New("standard error"),
			expected: errors.ErrUnknown,
		},
		{
			name:     "nil_error",
			err:      nil,
			expected: errors.ErrUnknown,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if got := errors.GetErrorCode(tt.err); got != tt.expected {
				t.Errorf("GetErrorCode() = %v, want %v", got, tt.expected)
			}
		})
	}
}

func TestErrorChaining(t *testing.T) {
	// Create a chain of errors
	rootCause := stderrors.New("root cause")
	fileErr := errors.Wrap(rootCause, errors.ErrFileAccess, "cannot read file")
	configErr := errors.Wrap(fileErr, errors.ErrConfigLoad, "failed to load config")

	t.Run("top_level_has_correct_code", func(t *testing.T) {
		if !errors.IsErrorCode(configErr, errors.ErrConfigLoad) {
			t.Error("Top level should have ErrConfigLoad code")
		}
	})

	t.Run("can_find_middle_error", func(t *testing.T) {
		var dodotErr *errors.DodotError
		if stderrors.As(configErr.Unwrap(), &dodotErr) {
			if !errors.IsErrorCode(dodotErr, errors.ErrFileAccess) {
				t.Error("Middle error should have ErrFileAccess code")
			}
		}
	})

	t.Run("can_find_root_cause", func(t *testing.T) {
		if !stderrors.Is(configErr, rootCause) {
			t.Error("Should find root cause with errors.Is")
		}
	})
}
