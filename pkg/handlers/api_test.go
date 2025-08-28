package handlers

import (
	"testing"

	"github.com/stretchr/testify/assert"
)

func TestHandlerRegistry_IsConfigurationHandler(t *testing.T) {
	tests := []struct {
		name        string
		handlerName string
		expected    bool
	}{
		{
			name:        "symlink is configuration handler",
			handlerName: "symlink",
			expected:    true,
		},
		{
			name:        "shell is configuration handler",
			handlerName: "shell",
			expected:    true,
		},
		{
			name:        "path is configuration handler",
			handlerName: "path",
			expected:    true,
		},
		{
			name:        "homebrew is not configuration handler",
			handlerName: "homebrew",
			expected:    false,
		},
		{
			name:        "install is not configuration handler",
			handlerName: "install",
			expected:    false,
		},
		{
			name:        "unknown handler is not configuration handler",
			handlerName: "unknown",
			expected:    false,
		},
		{
			name:        "empty string is not configuration handler",
			handlerName: "",
			expected:    false,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result := HandlerRegistry.IsConfigurationHandler(tt.handlerName)
			assert.Equal(t, tt.expected, result)
		})
	}
}

func TestHandlerRegistry_IsCodeExecutionHandler(t *testing.T) {
	tests := []struct {
		name        string
		handlerName string
		expected    bool
	}{
		{
			name:        "homebrew is code execution handler",
			handlerName: "homebrew",
			expected:    true,
		},
		{
			name:        "install is code execution handler",
			handlerName: "install",
			expected:    true,
		},
		{
			name:        "symlink is not code execution handler",
			handlerName: "symlink",
			expected:    false,
		},
		{
			name:        "shell is not code execution handler",
			handlerName: "shell",
			expected:    false,
		},
		{
			name:        "path is not code execution handler",
			handlerName: "path",
			expected:    false,
		},
		{
			name:        "unknown handler is not code execution handler",
			handlerName: "unknown",
			expected:    false,
		},
		{
			name:        "empty string is not code execution handler",
			handlerName: "",
			expected:    false,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result := HandlerRegistry.IsCodeExecutionHandler(tt.handlerName)
			assert.Equal(t, tt.expected, result)
		})
	}
}

func TestHandlerRegistry_GetHandlerCategory(t *testing.T) {
	tests := []struct {
		name        string
		handlerName string
		expected    HandlerCategory
	}{
		{
			name:        "symlink returns configuration category",
			handlerName: "symlink",
			expected:    CategoryConfiguration,
		},
		{
			name:        "shell returns configuration category",
			handlerName: "shell",
			expected:    CategoryConfiguration,
		},
		{
			name:        "path returns configuration category",
			handlerName: "path",
			expected:    CategoryConfiguration,
		},
		{
			name:        "homebrew returns code execution category",
			handlerName: "homebrew",
			expected:    CategoryCodeExecution,
		},
		{
			name:        "install returns code execution category",
			handlerName: "install",
			expected:    CategoryCodeExecution,
		},
		{
			name:        "unknown handler defaults to configuration category",
			handlerName: "unknown",
			expected:    CategoryConfiguration,
		},
		{
			name:        "empty string defaults to configuration category",
			handlerName: "",
			expected:    CategoryConfiguration,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			result := HandlerRegistry.GetHandlerCategory(tt.handlerName)
			assert.Equal(t, tt.expected, result)
		})
	}
}

func TestHandlerRegistry_GetConfigurationHandlers(t *testing.T) {
	result := HandlerRegistry.GetConfigurationHandlers()

	expected := []string{"symlink", "shell", "path"}
	assert.Equal(t, expected, result)
	assert.Len(t, result, 3)

	// Verify all returned handlers are actually configuration handlers
	for _, handlerName := range result {
		assert.True(t, HandlerRegistry.IsConfigurationHandler(handlerName),
			"Handler %s should be identified as configuration handler", handlerName)
		assert.False(t, HandlerRegistry.IsCodeExecutionHandler(handlerName),
			"Handler %s should not be identified as code execution handler", handlerName)
	}
}

func TestHandlerRegistry_GetCodeExecutionHandlers(t *testing.T) {
	result := HandlerRegistry.GetCodeExecutionHandlers()

	expected := []string{"homebrew", "install"}
	assert.Equal(t, expected, result)
	assert.Len(t, result, 2)

	// Verify all returned handlers are actually code execution handlers
	for _, handlerName := range result {
		assert.True(t, HandlerRegistry.IsCodeExecutionHandler(handlerName),
			"Handler %s should be identified as code execution handler", handlerName)
		assert.False(t, HandlerRegistry.IsConfigurationHandler(handlerName),
			"Handler %s should not be identified as configuration handler", handlerName)
	}
}

func TestHandlerRegistry_RequiresExecutionOrdering(t *testing.T) {
	result := HandlerRegistry.RequiresExecutionOrdering()

	// Should return true - code execution handlers run before configuration handlers
	assert.True(t, result)
}

// Test that handler categories are mutually exclusive
func TestHandlerRegistry_MutualExclusivity(t *testing.T) {
	// Get all known handlers from both categories
	configHandlers := HandlerRegistry.GetConfigurationHandlers()
	codeExecHandlers := HandlerRegistry.GetCodeExecutionHandlers()

	// Create a map to check for overlaps
	handlerSet := make(map[string]bool)

	// Add configuration handlers
	for _, handler := range configHandlers {
		handlerSet[handler] = true
	}

	// Check that no code execution handler is already in the set
	for _, handler := range codeExecHandlers {
		assert.False(t, handlerSet[handler],
			"Handler %s appears in both configuration and code execution categories", handler)
	}
}

// Test consistency between different API methods
func TestHandlerRegistry_Consistency(t *testing.T) {
	allKnownHandlers := []string{"symlink", "shell", "path", "homebrew", "install"}

	for _, handler := range allKnownHandlers {
		// Test that IsConfigurationHandler and GetHandlerCategory are consistent
		isConfig := HandlerRegistry.IsConfigurationHandler(handler)
		category := HandlerRegistry.GetHandlerCategory(handler)

		if isConfig {
			assert.Equal(t, CategoryConfiguration, category,
				"Handler %s: IsConfigurationHandler and GetHandlerCategory should be consistent", handler)
		}

		// Test that IsCodeExecutionHandler and GetHandlerCategory are consistent
		isCodeExec := HandlerRegistry.IsCodeExecutionHandler(handler)

		if isCodeExec {
			assert.Equal(t, CategoryCodeExecution, category,
				"Handler %s: IsCodeExecutionHandler and GetHandlerCategory should be consistent", handler)
		}

		// Test that a handler cannot be both categories
		assert.False(t, isConfig && isCodeExec,
			"Handler %s cannot be both configuration and code execution", handler)

		// Test that a handler must be at least one category (for known handlers)
		assert.True(t, isConfig || isCodeExec,
			"Handler %s must be either configuration or code execution", handler)
	}
}

// Test that GetConfigurationHandlers returns handlers that are all identified correctly
func TestHandlerRegistry_ConfigurationHandlersComplete(t *testing.T) {
	configHandlers := HandlerRegistry.GetConfigurationHandlers()

	for _, handler := range configHandlers {
		// Each should be identified as configuration
		assert.True(t, HandlerRegistry.IsConfigurationHandler(handler))
		// Each should have configuration category
		assert.Equal(t, CategoryConfiguration, HandlerRegistry.GetHandlerCategory(handler))
		// Each should NOT be identified as code execution
		assert.False(t, HandlerRegistry.IsCodeExecutionHandler(handler))
	}
}

// Test that GetCodeExecutionHandlers returns handlers that are all identified correctly
func TestHandlerRegistry_CodeExecutionHandlersComplete(t *testing.T) {
	codeExecHandlers := HandlerRegistry.GetCodeExecutionHandlers()

	for _, handler := range codeExecHandlers {
		// Each should be identified as code execution
		assert.True(t, HandlerRegistry.IsCodeExecutionHandler(handler))
		// Each should have code execution category
		assert.Equal(t, CategoryCodeExecution, HandlerRegistry.GetHandlerCategory(handler))
		// Each should NOT be identified as configuration
		assert.False(t, HandlerRegistry.IsConfigurationHandler(handler))
	}
}

// Test edge cases and potential future scenarios
func TestHandlerRegistry_EdgeCases(t *testing.T) {
	edgeCases := []string{
		"",            // empty string
		"unknown",     // unknown handler
		"SYMLINK",     // case sensitivity
		"symlink ",    // whitespace
		" symlink",    // leading whitespace
		"sym-link",    // similar but different
		"nonexistent", // clearly non-existent
	}

	for _, testCase := range edgeCases {
		t.Run("edge_case_"+testCase, func(t *testing.T) {
			// Unknown handlers should default to configuration (safe default)
			category := HandlerRegistry.GetHandlerCategory(testCase)
			assert.Equal(t, CategoryConfiguration, category,
				"Unknown handler %q should default to configuration category", testCase)

			// Unknown handlers should not be identified as either category
			isConfig := HandlerRegistry.IsConfigurationHandler(testCase)
			isCodeExec := HandlerRegistry.IsCodeExecutionHandler(testCase)

			// For edge cases, both should be false (unknown handlers)
			assert.False(t, isConfig, "Edge case %q should not be identified as configuration", testCase)
			assert.False(t, isCodeExec, "Edge case %q should not be identified as code execution", testCase)
		})
	}
}
