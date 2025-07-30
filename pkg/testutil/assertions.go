package testutil

import (
	"fmt"
	"reflect"
	"sort"
	"strings"
	"testing"
)

// AssertEqual checks if two values are equal using deep equality
func AssertEqual(t *testing.T, expected, actual interface{}, msgAndArgs ...interface{}) {
	t.Helper()

	if !reflect.DeepEqual(expected, actual) {
		msg := formatMessage(msgAndArgs...)
		t.Errorf("%sExpected: %+v\nActual: %+v", msg, expected, actual)
	}
}

// AssertNotEqual checks if two values are not equal
func AssertNotEqual(t *testing.T, expected, actual interface{}, msgAndArgs ...interface{}) {
	t.Helper()

	if reflect.DeepEqual(expected, actual) {
		msg := formatMessage(msgAndArgs...)
		t.Errorf("%sValues should not be equal: %+v", msg, actual)
	}
}

// AssertNil checks if a value is nil
func AssertNil(t *testing.T, value interface{}, msgAndArgs ...interface{}) {
	t.Helper()

	if !isNil(value) {
		msg := formatMessage(msgAndArgs...)
		t.Errorf("%sExpected nil, got: %+v", msg, value)
	}
}

// AssertNotNil checks if a value is not nil
func AssertNotNil(t *testing.T, value interface{}, msgAndArgs ...interface{}) {
	t.Helper()

	if isNil(value) {
		msg := formatMessage(msgAndArgs...)
		t.Errorf("%sExpected non-nil value", msg)
	}
}

// AssertTrue checks if a value is true
func AssertTrue(t *testing.T, value bool, msgAndArgs ...interface{}) {
	t.Helper()

	if !value {
		msg := formatMessage(msgAndArgs...)
		t.Errorf("%sExpected true, got false", msg)
	}
}

// AssertFalse checks if a value is false
func AssertFalse(t *testing.T, value bool, msgAndArgs ...interface{}) {
	t.Helper()

	if value {
		msg := formatMessage(msgAndArgs...)
		t.Errorf("%sExpected false, got true", msg)
	}
}

// AssertContains checks if a string contains a substring
func AssertContains(t *testing.T, str, substr string, msgAndArgs ...interface{}) {
	t.Helper()

	if !strings.Contains(str, substr) {
		msg := formatMessage(msgAndArgs...)
		t.Errorf("%sString %q does not contain %q", msg, str, substr)
	}
}

// AssertNotContains checks if a string does not contain a substring
func AssertNotContains(t *testing.T, str, substr string, msgAndArgs ...interface{}) {
	t.Helper()

	if strings.Contains(str, substr) {
		msg := formatMessage(msgAndArgs...)
		t.Errorf("%sString %q should not contain %q", msg, str, substr)
	}
}

// AssertSliceEqual checks if two slices are equal (ignoring order)
func AssertSliceEqual(t *testing.T, expected, actual []string, msgAndArgs ...interface{}) {
	t.Helper()

	if len(expected) != len(actual) {
		msg := formatMessage(msgAndArgs...)
		t.Errorf("%sSlice length mismatch. Expected: %d, Actual: %d\nExpected: %v\nActual: %v",
			msg, len(expected), len(actual), expected, actual)
		return
	}

	// Sort both slices for comparison
	expectedSorted := make([]string, len(expected))
	actualSorted := make([]string, len(actual))
	copy(expectedSorted, expected)
	copy(actualSorted, actual)
	sort.Strings(expectedSorted)
	sort.Strings(actualSorted)

	for i := range expectedSorted {
		if expectedSorted[i] != actualSorted[i] {
			msg := formatMessage(msgAndArgs...)
			t.Errorf("%sSlice content mismatch at index %d\nExpected: %v\nActual: %v",
				msg, i, expected, actual)
			return
		}
	}
}

// AssertMapEqual checks if two string maps are equal
func AssertMapEqual(t *testing.T, expected, actual map[string]string, msgAndArgs ...interface{}) {
	t.Helper()

	if len(expected) != len(actual) {
		msg := formatMessage(msgAndArgs...)
		t.Errorf("%sMap size mismatch. Expected: %d, Actual: %d", msg, len(expected), len(actual))
		return
	}

	for k, v := range expected {
		actualV, exists := actual[k]
		if !exists {
			msg := formatMessage(msgAndArgs...)
			t.Errorf("%sMap missing key %q", msg, k)
			continue
		}
		if v != actualV {
			msg := formatMessage(msgAndArgs...)
			t.Errorf("%sMap value mismatch for key %q. Expected: %q, Actual: %q", msg, k, v, actualV)
		}
	}

	// Check for extra keys in actual
	for k := range actual {
		if _, exists := expected[k]; !exists {
			msg := formatMessage(msgAndArgs...)
			t.Errorf("%sMap has unexpected key %q", msg, k)
		}
	}
}

// AssertError checks if an error occurred
func AssertError(t *testing.T, err error, msgAndArgs ...interface{}) {
	t.Helper()

	if err == nil {
		msg := formatMessage(msgAndArgs...)
		t.Errorf("%sExpected an error but got nil", msg)
	}
}

// AssertNoError checks if no error occurred
func AssertNoError(t *testing.T, err error, msgAndArgs ...interface{}) {
	t.Helper()

	if err != nil {
		msg := formatMessage(msgAndArgs...)
		t.Errorf("%sUnexpected error: %v", msg, err)
	}
}

// AssertPanic checks if a function panics
func AssertPanic(t *testing.T, fn func(), msgAndArgs ...interface{}) {
	t.Helper()

	defer func() {
		if r := recover(); r == nil {
			msg := formatMessage(msgAndArgs...)
			t.Errorf("%sExpected panic but function completed normally", msg)
		}
	}()

	fn()
}

// AssertNoPanic checks if a function does not panic
func AssertNoPanic(t *testing.T, fn func(), msgAndArgs ...interface{}) {
	t.Helper()

	defer func() {
		if r := recover(); r != nil {
			msg := formatMessage(msgAndArgs...)
			t.Errorf("%sUnexpected panic: %v", msg, r)
		}
	}()

	fn()
}

// Helper functions

func isNil(value interface{}) bool {
	if value == nil {
		return true
	}

	v := reflect.ValueOf(value)
	switch v.Kind() {
	case reflect.Chan, reflect.Func, reflect.Interface, reflect.Map, reflect.Ptr, reflect.Slice:
		return v.IsNil()
	}

	return false
}

func formatMessage(msgAndArgs ...interface{}) string {
	if len(msgAndArgs) == 0 {
		return ""
	}

	if len(msgAndArgs) == 1 {
		if msg, ok := msgAndArgs[0].(string); ok {
			return msg + "\n"
		}
		return fmt.Sprint(msgAndArgs[0]) + "\n"
	}

	// Check if first arg is a format string with format verbs
	if format, ok := msgAndArgs[0].(string); ok && len(msgAndArgs) > 1 {
		// Simple check for format verbs
		if strings.Contains(format, "%") {
			return fmt.Sprintf(format, msgAndArgs[1:]...) + "\n"
		}
	}

	// Otherwise, just concatenate with spaces
	parts := make([]string, len(msgAndArgs))
	for i, arg := range msgAndArgs {
		parts[i] = fmt.Sprint(arg)
	}
	return strings.Join(parts, " ") + "\n"
}

// AssertNotEmpty checks that a string is not empty
func AssertNotEmpty(t *testing.T, value string, msgAndArgs ...interface{}) {
	t.Helper()
	if value == "" {
		msg := formatMessage(msgAndArgs...)
		t.Errorf("%sExpected non-empty string", msg)
	}
}

// AssertFileExists checks that a file exists.
func AssertFileExists(t *testing.T, path string, msgAndArgs ...interface{}) {
	t.Helper()
	if !FileExists(t, path) {
		msg := formatMessage(msgAndArgs...)
		t.Errorf("%sFile does not exist: %s", msg, path)
	}
}

// AssertDirExists checks that a directory exists.
func AssertDirExists(t *testing.T, path string, msgAndArgs ...interface{}) {
	t.Helper()
	if !DirExists(t, path) {
		msg := formatMessage(msgAndArgs...)
		t.Errorf("%sDirectory does not exist: %s", msg, path)
	}
}