package errors

import (
	"errors"
	"fmt"
)

// ErrorCode represents a unique error code for stable testing
type ErrorCode string

// Error codes for different error categories
const (
	// General errors
	ErrUnknown      ErrorCode = "UNKNOWN"
	ErrInternal     ErrorCode = "INTERNAL"
	ErrInvalidInput ErrorCode = "INVALID_INPUT"
	ErrNotFound     ErrorCode = "NOT_FOUND"
	ErrAlreadyExists ErrorCode = "ALREADY_EXISTS"
	ErrPermission   ErrorCode = "PERMISSION"
	ErrNotImplemented ErrorCode = "NOT_IMPLEMENTED"
	
	// Configuration errors
	ErrConfigLoad   ErrorCode = "CONFIG_LOAD"
	ErrConfigParse  ErrorCode = "CONFIG_PARSE"
	ErrConfigValid  ErrorCode = "CONFIG_INVALID"
	
	// Pack errors
	ErrPackNotFound ErrorCode = "PACK_NOT_FOUND"
	ErrPackInvalid  ErrorCode = "PACK_INVALID"
	ErrPackAccess   ErrorCode = "PACK_ACCESS"
	
	// Trigger errors
	ErrTriggerNotFound ErrorCode = "TRIGGER_NOT_FOUND"
	ErrTriggerInvalid  ErrorCode = "TRIGGER_INVALID"
	ErrTriggerMatch    ErrorCode = "TRIGGER_MATCH"
	
	// PowerUp errors
	ErrPowerUpNotFound ErrorCode = "POWERUP_NOT_FOUND"
	ErrPowerUpInvalid  ErrorCode = "POWERUP_INVALID"
	ErrPowerUpExecute  ErrorCode = "POWERUP_EXECUTE"
	
	// Action errors
	ErrActionInvalid   ErrorCode = "ACTION_INVALID"
	ErrActionConflict  ErrorCode = "ACTION_CONFLICT"
	ErrActionExecute   ErrorCode = "ACTION_EXECUTE"
	
	// FileSystem errors
	ErrFileNotFound   ErrorCode = "FILE_NOT_FOUND"
	ErrFileAccess     ErrorCode = "FILE_ACCESS"
	ErrFileCreate     ErrorCode = "FILE_CREATE"
	ErrFileWrite      ErrorCode = "FILE_WRITE"
	ErrSymlinkCreate  ErrorCode = "SYMLINK_CREATE"
	ErrSymlinkExists  ErrorCode = "SYMLINK_EXISTS"
	ErrDirCreate      ErrorCode = "DIR_CREATE"
)

// DodotError represents a structured error with code and details
type DodotError struct {
	Code    ErrorCode
	Message string
	Details map[string]interface{}
	Wrapped error
}

// Error implements the error interface
func (e *DodotError) Error() string {
	if e.Wrapped != nil {
		return fmt.Sprintf("[%s] %s: %v", e.Code, e.Message, e.Wrapped)
	}
	return fmt.Sprintf("[%s] %s", e.Code, e.Message)
}

// Unwrap implements the errors.Unwrap interface
func (e *DodotError) Unwrap() error {
	return e.Wrapped
}

// Is implements errors.Is interface
func (e *DodotError) Is(target error) bool {
	var targetErr *DodotError
	if errors.As(target, &targetErr) {
		return e.Code == targetErr.Code
	}
	return false
}

// New creates a new DodotError with the given code and message
func New(code ErrorCode, message string) *DodotError {
	return &DodotError{
		Code:    code,
		Message: message,
		Details: make(map[string]interface{}),
	}
}

// Newf creates a new DodotError with a formatted message
func Newf(code ErrorCode, format string, args ...interface{}) *DodotError {
	return &DodotError{
		Code:    code,
		Message: fmt.Sprintf(format, args...),
		Details: make(map[string]interface{}),
	}
}

// Wrap wraps an existing error with a DodotError
func Wrap(err error, code ErrorCode, message string) *DodotError {
	if err == nil {
		return nil
	}
	return &DodotError{
		Code:    code,
		Message: message,
		Details: make(map[string]interface{}),
		Wrapped: err,
	}
}

// Wrapf wraps an existing error with a formatted message
func Wrapf(err error, code ErrorCode, format string, args ...interface{}) *DodotError {
	if err == nil {
		return nil
	}
	return &DodotError{
		Code:    code,
		Message: fmt.Sprintf(format, args...),
		Details: make(map[string]interface{}),
		Wrapped: err,
	}
}

// WithDetail adds a detail to the error
func (e *DodotError) WithDetail(key string, value interface{}) *DodotError {
	if e.Details == nil {
		e.Details = make(map[string]interface{})
	}
	e.Details[key] = value
	return e
}

// WithDetails adds multiple details to the error
func (e *DodotError) WithDetails(details map[string]interface{}) *DodotError {
	if e.Details == nil {
		e.Details = make(map[string]interface{})
	}
	for k, v := range details {
		e.Details[k] = v
	}
	return e
}

// IsErrorCode checks if an error has a specific error code
func IsErrorCode(err error, code ErrorCode) bool {
	var dodotErr *DodotError
	if errors.As(err, &dodotErr) {
		return dodotErr.Code == code
	}
	return false
}

// GetErrorCode returns the error code from an error, or ErrUnknown if not a DodotError
func GetErrorCode(err error) ErrorCode {
	var dodotErr *DodotError
	if errors.As(err, &dodotErr) {
		return dodotErr.Code
	}
	return ErrUnknown
}

// GetErrorDetails returns the details from an error, or nil if not a DodotError
func GetErrorDetails(err error) map[string]interface{} {
	var dodotErr *DodotError
	if errors.As(err, &dodotErr) {
		return dodotErr.Details
	}
	return nil
}