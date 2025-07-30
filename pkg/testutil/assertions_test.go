package testutil

import (
	"errors"
	"testing"
)

func TestAssertEqual(t *testing.T) {
	// Test with equal values
	AssertEqual(t, 42, 42)
	AssertEqual(t, "hello", "hello")
	AssertEqual(t, []int{1, 2, 3}, []int{1, 2, 3})

	// Test with custom message
	AssertEqual(t, true, true, "boolean comparison")
}

func TestAssertNotEqual(t *testing.T) {
	// Test with different values
	AssertNotEqual(t, 42, 43)
	AssertNotEqual(t, "hello", "world")
}

func TestAssertNil(t *testing.T) {
	// Test with nil values
	AssertNil(t, nil)
	var ptr *string
	AssertNil(t, ptr)
	var slice []int
	AssertNil(t, slice)
}

func TestAssertNotNil(t *testing.T) {
	// Test with non-nil values
	AssertNotNil(t, "not nil")
	AssertNotNil(t, 42)
	AssertNotNil(t, []int{1, 2, 3})
}

func TestAssertTrue(t *testing.T) {
	// Test with true
	AssertTrue(t, true)
	x := 1
	AssertTrue(t, x == 1)
}

func TestAssertFalse(t *testing.T) {
	// Test with false
	AssertFalse(t, false)
	AssertFalse(t, 1 == 2)
}

func TestAssertContains(t *testing.T) {
	// Test with substring
	AssertContains(t, "hello world", "world")
	AssertContains(t, "testing", "test")
}

func TestAssertNotContains(t *testing.T) {
	// Test without substring
	AssertNotContains(t, "hello", "world")
	AssertNotContains(t, "testing", "fail")
}

func TestAssertSliceEqual(t *testing.T) {
	// Test with equal slices
	AssertSliceEqual(t, []string{"a", "b", "c"}, []string{"a", "b", "c"})

	// Test with different order (should still pass)
	AssertSliceEqual(t, []string{"c", "b", "a"}, []string{"a", "b", "c"})
}

func TestAssertMapEqual(t *testing.T) {
	// Test with equal maps
	map1 := map[string]string{"key1": "value1", "key2": "value2"}
	map2 := map[string]string{"key1": "value1", "key2": "value2"}
	AssertMapEqual(t, map1, map2)
}

func TestAssertError(t *testing.T) {
	// Test with error
	err := errors.New("test error")
	AssertError(t, err)
}

func TestAssertNoError(t *testing.T) {
	// Test with nil error
	AssertNoError(t, nil)
}

func TestAssertPanic(t *testing.T) {
	// Test with panicking function
	AssertPanic(t, func() {
		panic("test panic")
	})
}

func TestAssertNoPanic(t *testing.T) {
	// Test with non-panicking function
	AssertNoPanic(t, func() {
		// Does not panic
	})
}

func TestFormatMessage(t *testing.T) {
	tests := []struct {
		name     string
		args     []interface{}
		expected string
	}{
		{
			name:     "no args",
			args:     []interface{}{},
			expected: "",
		},
		{
			name:     "single string",
			args:     []interface{}{"test message"},
			expected: "test message\n",
		},
		{
			name:     "format string",
			args:     []interface{}{"value is %d", 42},
			expected: "value is 42\n",
		},
		{
			name:     "multiple args",
			args:     []interface{}{"multiple", "args"},
			expected: "multiple args\n",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got := formatMessage(tt.args...)
			if got != tt.expected {
				t.Errorf("formatMessage() = %q, want %q", got, tt.expected)
			}
		})
	}
}

func TestIsNil(t *testing.T) {
	tests := []struct {
		name     string
		value    interface{}
		expected bool
	}{
		{"nil literal", nil, true},
		{"nil pointer", (*string)(nil), true},
		{"nil slice", ([]int)(nil), true},
		{"nil map", (map[string]int)(nil), true},
		{"nil chan", (chan int)(nil), true},
		{"nil func", (func())(nil), true},
		{"non-nil string", "test", false},
		{"non-nil int", 42, false},
		{"non-nil slice", []int{1, 2, 3}, false},
		{"empty slice", []int{}, false},
		{"zero int", 0, false},
		{"empty string", "", false},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got := isNil(tt.value)
			if got != tt.expected {
				t.Errorf("isNil(%v) = %v, want %v", tt.value, got, tt.expected)
			}
		})
	}
}
