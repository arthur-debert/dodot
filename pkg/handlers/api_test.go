package handlers

import (
	"testing"
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
			if result != tt.expected {
				t.Errorf("IsConfigurationHandler(%q) = %v, want %v", tt.handlerName, result, tt.expected)
			}
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
			if result != tt.expected {
				t.Errorf("IsCodeExecutionHandler(%q) = %v, want %v", tt.handlerName, result, tt.expected)
			}
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
			if result != tt.expected {
				t.Errorf("GetHandlerCategory(%q) = %v, want %v", tt.handlerName, result, tt.expected)
			}
		})
	}
}

func TestHandlerRegistry_GetConfigurationHandlers(t *testing.T) {
	result := HandlerRegistry.GetConfigurationHandlers()

	expected := []string{"symlink", "shell", "path"}
	if len(result) != len(expected) {
		t.Errorf("GetConfigurationHandlers() returned %d handlers, want %d", len(result), len(expected))
	}

	for i, handler := range expected {
		if i >= len(result) {
			t.Errorf("GetConfigurationHandlers() missing handler at index %d: want %q", i, handler)
			continue
		}
		if result[i] != handler {
			t.Errorf("GetConfigurationHandlers()[%d] = %q, want %q", i, result[i], handler)
		}
	}

	// Verify all returned handlers are actually configuration handlers
	for _, handlerName := range result {
		if !HandlerRegistry.IsConfigurationHandler(handlerName) {
			t.Errorf("Handler %q should be identified as configuration handler", handlerName)
		}
		if HandlerRegistry.IsCodeExecutionHandler(handlerName) {
			t.Errorf("Handler %q should not be identified as code execution handler", handlerName)
		}
	}
}

func TestHandlerRegistry_GetCodeExecutionHandlers(t *testing.T) {
	result := HandlerRegistry.GetCodeExecutionHandlers()

	expected := []string{"homebrew", "install"}
	if len(result) != len(expected) {
		t.Errorf("GetCodeExecutionHandlers() returned %d handlers, want %d", len(result), len(expected))
	}

	for i, handler := range expected {
		if i >= len(result) {
			t.Errorf("GetCodeExecutionHandlers() missing handler at index %d: want %q", i, handler)
			continue
		}
		if result[i] != handler {
			t.Errorf("GetCodeExecutionHandlers()[%d] = %q, want %q", i, result[i], handler)
		}
	}

	// Verify all returned handlers are actually code execution handlers
	for _, handlerName := range result {
		if !HandlerRegistry.IsCodeExecutionHandler(handlerName) {
			t.Errorf("Handler %q should be identified as code execution handler", handlerName)
		}
		if HandlerRegistry.IsConfigurationHandler(handlerName) {
			t.Errorf("Handler %q should not be identified as configuration handler", handlerName)
		}
	}
}

func TestHandlerRegistry_RequiresExecutionOrdering(t *testing.T) {
	result := HandlerRegistry.RequiresExecutionOrdering()

	// Should return true - code execution handlers run before configuration handlers
	if !result {
		t.Error("RequiresExecutionOrdering() = false, want true")
	}
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
		if handlerSet[handler] {
			t.Errorf("Handler %q appears in both configuration and code execution categories", handler)
		}
	}
}

// Test consistency between different API methods
func TestHandlerRegistry_Consistency(t *testing.T) {
	allKnownHandlers := []string{"symlink", "shell", "path", "homebrew", "install"}

	for _, handler := range allKnownHandlers {
		// Test that IsConfigurationHandler and GetHandlerCategory are consistent
		isConfig := HandlerRegistry.IsConfigurationHandler(handler)
		category := HandlerRegistry.GetHandlerCategory(handler)

		if isConfig && category != CategoryConfiguration {
			t.Errorf("Handler %q: IsConfigurationHandler returns true but GetHandlerCategory returns %v", handler, category)
		}

		// Test that IsCodeExecutionHandler and GetHandlerCategory are consistent
		isCodeExec := HandlerRegistry.IsCodeExecutionHandler(handler)

		if isCodeExec && category != CategoryCodeExecution {
			t.Errorf("Handler %q: IsCodeExecutionHandler returns true but GetHandlerCategory returns %v", handler, category)
		}

		// Test that a handler cannot be both categories
		if isConfig && isCodeExec {
			t.Errorf("Handler %q cannot be both configuration and code execution", handler)
		}

		// Test that a handler must be at least one category (for known handlers)
		if !isConfig && !isCodeExec {
			t.Errorf("Handler %q must be either configuration or code execution", handler)
		}
	}
}

// Test that GetConfigurationHandlers returns handlers that are all identified correctly
func TestHandlerRegistry_ConfigurationHandlersComplete(t *testing.T) {
	configHandlers := HandlerRegistry.GetConfigurationHandlers()

	for _, handler := range configHandlers {
		// Each should be identified as configuration
		if !HandlerRegistry.IsConfigurationHandler(handler) {
			t.Errorf("GetConfigurationHandlers() returned %q which is not identified as configuration handler", handler)
		}
		// Each should have configuration category
		if HandlerRegistry.GetHandlerCategory(handler) != CategoryConfiguration {
			t.Errorf("GetConfigurationHandlers() returned %q which has category %v", handler, HandlerRegistry.GetHandlerCategory(handler))
		}
		// Each should NOT be identified as code execution
		if HandlerRegistry.IsCodeExecutionHandler(handler) {
			t.Errorf("GetConfigurationHandlers() returned %q which is identified as code execution handler", handler)
		}
	}
}

// Test that GetCodeExecutionHandlers returns handlers that are all identified correctly
func TestHandlerRegistry_CodeExecutionHandlersComplete(t *testing.T) {
	codeExecHandlers := HandlerRegistry.GetCodeExecutionHandlers()

	for _, handler := range codeExecHandlers {
		// Each should be identified as code execution
		if !HandlerRegistry.IsCodeExecutionHandler(handler) {
			t.Errorf("GetCodeExecutionHandlers() returned %q which is not identified as code execution handler", handler)
		}
		// Each should have code execution category
		if HandlerRegistry.GetHandlerCategory(handler) != CategoryCodeExecution {
			t.Errorf("GetCodeExecutionHandlers() returned %q which has category %v", handler, HandlerRegistry.GetHandlerCategory(handler))
		}
		// Each should NOT be identified as configuration
		if HandlerRegistry.IsConfigurationHandler(handler) {
			t.Errorf("GetCodeExecutionHandlers() returned %q which is identified as configuration handler", handler)
		}
	}
}

// Test edge cases and potential future scenarios
func TestHandlerRegistry_EdgeCases(t *testing.T) {
	edgeCases := []struct {
		name     string
		input    string
		category HandlerCategory
		isConfig bool
		isCode   bool
	}{
		{
			name:     "empty string",
			input:    "",
			category: CategoryConfiguration, // default
			isConfig: false,
			isCode:   false,
		},
		{
			name:     "unknown handler",
			input:    "unknown",
			category: CategoryConfiguration, // default
			isConfig: false,
			isCode:   false,
		},
		{
			name:     "uppercase variation",
			input:    "SYMLINK",
			category: CategoryConfiguration, // default
			isConfig: false,                 // case sensitive
			isCode:   false,
		},
		{
			name:     "whitespace suffix",
			input:    "symlink ",
			category: CategoryConfiguration, // default
			isConfig: false,                 // exact match
			isCode:   false,
		},
		{
			name:     "whitespace prefix",
			input:    " symlink",
			category: CategoryConfiguration, // default
			isConfig: false,                 // exact match
			isCode:   false,
		},
		{
			name:     "similar but different",
			input:    "sym-link",
			category: CategoryConfiguration, // default
			isConfig: false,
			isCode:   false,
		},
		{
			name:     "clearly non-existent",
			input:    "nonexistent",
			category: CategoryConfiguration, // default
			isConfig: false,
			isCode:   false,
		},
	}

	for _, tc := range edgeCases {
		t.Run(tc.name, func(t *testing.T) {
			// Test GetHandlerCategory
			category := HandlerRegistry.GetHandlerCategory(tc.input)
			if category != tc.category {
				t.Errorf("GetHandlerCategory(%q) = %v, want %v", tc.input, category, tc.category)
			}

			// Test IsConfigurationHandler
			isConfig := HandlerRegistry.IsConfigurationHandler(tc.input)
			if isConfig != tc.isConfig {
				t.Errorf("IsConfigurationHandler(%q) = %v, want %v", tc.input, isConfig, tc.isConfig)
			}

			// Test IsCodeExecutionHandler
			isCode := HandlerRegistry.IsCodeExecutionHandler(tc.input)
			if isCode != tc.isCode {
				t.Errorf("IsCodeExecutionHandler(%q) = %v, want %v", tc.input, isCode, tc.isCode)
			}
		})
	}
}
