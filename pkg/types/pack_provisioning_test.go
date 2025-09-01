package types

import (
	"errors"
	"testing"
)

// MockDataStore implements DataStore interface for testing
type MockDataStore struct {
	handlerStates    map[string]map[string]bool     // pack -> handler -> hasState
	packHandlers     map[string][]string            // pack -> list of handlers
	handlerSentinels map[string]map[string][]string // pack -> handler -> sentinels
	errorOnCall      bool
}

func NewMockDataStore() *MockDataStore {
	return &MockDataStore{
		handlerStates:    make(map[string]map[string]bool),
		packHandlers:     make(map[string][]string),
		handlerSentinels: make(map[string]map[string][]string),
	}
}

func (m *MockDataStore) CreateDataLink(pack, handlerName, sourceFile string) (string, error) {
	return "", nil
}

func (m *MockDataStore) CreateUserLink(datastorePath, userPath string) error {
	return nil
}

func (m *MockDataStore) RunAndRecord(pack, handlerName, command, sentinel string) error {
	return nil
}

func (m *MockDataStore) HasSentinel(pack, handlerName, sentinel string) (bool, error) {
	return false, nil
}

func (m *MockDataStore) RemoveState(pack, handlerName string) error {
	return nil
}

func (m *MockDataStore) HasHandlerState(pack, handlerName string) (bool, error) {
	if m.errorOnCall {
		return false, errors.New("pack not found")
	}
	if packStates, ok := m.handlerStates[pack]; ok {
		if hasState, ok := packStates[handlerName]; ok {
			return hasState, nil
		}
	}
	return false, nil
}

func (m *MockDataStore) ListPackHandlers(pack string) ([]string, error) {
	if m.errorOnCall {
		return nil, errors.New("pack not found")
	}
	if handlers, ok := m.packHandlers[pack]; ok {
		return handlers, nil
	}
	return []string{}, nil
}

func (m *MockDataStore) ListHandlerSentinels(pack, handlerName string) ([]string, error) {
	if m.errorOnCall {
		return nil, errors.New("pack not found")
	}
	if packSentinels, ok := m.handlerSentinels[pack]; ok {
		if sentinels, ok := packSentinels[handlerName]; ok {
			return sentinels, nil
		}
	}
	return []string{}, nil
}

// Helper methods for test setup
func (m *MockDataStore) SetHandlerState(pack, handler string, hasState bool) {
	if m.handlerStates[pack] == nil {
		m.handlerStates[pack] = make(map[string]bool)
	}
	m.handlerStates[pack][handler] = hasState
}

func (m *MockDataStore) SetPackHandlers(pack string, handlers []string) {
	m.packHandlers[pack] = handlers
	// Also ensure state entries exist
	if m.handlerStates[pack] == nil {
		m.handlerStates[pack] = make(map[string]bool)
	}
	for _, h := range handlers {
		if _, exists := m.handlerStates[pack][h]; !exists {
			m.handlerStates[pack][h] = false
		}
	}
}

func TestPackIsHandlerProvisioned(t *testing.T) {
	tests := []struct {
		name        string
		pack        *Pack
		handler     string
		setupFunc   func(*MockDataStore)
		expected    bool
		expectError bool
	}{
		{
			name:    "provisioned handler returns true",
			pack:    &Pack{Name: "vim"},
			handler: "homebrew",
			setupFunc: func(ds *MockDataStore) {
				ds.SetHandlerState("vim", "homebrew", true)
			},
			expected: true,
		},
		{
			name:    "unprovisioned handler returns false",
			pack:    &Pack{Name: "vim"},
			handler: "install",
			setupFunc: func(ds *MockDataStore) {
				ds.SetHandlerState("vim", "install", false)
			},
			expected: false,
		},
		{
			name:        "non-existent handler returns false",
			pack:        &Pack{Name: "vim"},
			handler:     "nonexistent",
			setupFunc:   func(ds *MockDataStore) {},
			expected:    false,
			expectError: false,
		},
		{
			name:    "error from datastore propagates",
			pack:    &Pack{Name: "vim"},
			handler: "homebrew",
			setupFunc: func(ds *MockDataStore) {
				ds.errorOnCall = true
			},
			expected:    false,
			expectError: true,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			// Setup
			ds := NewMockDataStore()
			if tt.setupFunc != nil {
				tt.setupFunc(ds)
			}

			// Test
			result, err := tt.pack.IsHandlerProvisioned(ds, tt.handler)

			// Verify
			if tt.expectError && err == nil {
				t.Error("expected error but got none")
			}
			if !tt.expectError && err != nil {
				t.Errorf("unexpected error: %v", err)
			}
			if result != tt.expected {
				t.Errorf("expected %v but got %v", tt.expected, result)
			}
		})
	}
}

func TestPackGetProvisionedHandlers(t *testing.T) {
	tests := []struct {
		name        string
		pack        *Pack
		setupFunc   func(*MockDataStore)
		expected    []string
		expectError bool
	}{
		{
			name: "returns only handlers with state",
			pack: &Pack{Name: "vim"},
			setupFunc: func(ds *MockDataStore) {
				ds.SetPackHandlers("vim", []string{"symlink", "homebrew", "install", "shell"})
				ds.SetHandlerState("vim", "symlink", true)
				ds.SetHandlerState("vim", "homebrew", true)
				ds.SetHandlerState("vim", "install", false) // no state
				ds.SetHandlerState("vim", "shell", true)
			},
			expected: []string{"symlink", "homebrew", "shell"},
		},
		{
			name: "pack with no provisioned handlers",
			pack: &Pack{Name: "tmux"},
			setupFunc: func(ds *MockDataStore) {
				ds.SetPackHandlers("tmux", []string{"symlink", "shell"})
				ds.SetHandlerState("tmux", "symlink", false)
				ds.SetHandlerState("tmux", "shell", false)
			},
			expected: []string{},
		},
		{
			name:      "non-existent pack returns empty list",
			pack:      &Pack{Name: "nonexistent"},
			setupFunc: func(ds *MockDataStore) {},
			expected:  []string{},
		},
		{
			name: "error from ListPackHandlers propagates",
			pack: &Pack{Name: "vim"},
			setupFunc: func(ds *MockDataStore) {
				ds.errorOnCall = true
			},
			expected:    nil,
			expectError: true,
		},
		{
			name: "mixed state handlers",
			pack: &Pack{Name: "git"},
			setupFunc: func(ds *MockDataStore) {
				ds.SetPackHandlers("git", []string{"symlink", "shell", "path", "homebrew"})
				ds.SetHandlerState("git", "symlink", true)
				ds.SetHandlerState("git", "shell", false)
				ds.SetHandlerState("git", "path", true)
				ds.SetHandlerState("git", "homebrew", false)
			},
			expected: []string{"symlink", "path"},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			// Setup
			ds := NewMockDataStore()
			if tt.setupFunc != nil {
				tt.setupFunc(ds)
			}

			// Test
			result, err := tt.pack.GetProvisionedHandlers(ds)

			// Verify
			if tt.expectError && err == nil {
				t.Error("expected error but got none")
			}
			if !tt.expectError && err != nil {
				t.Errorf("unexpected error: %v", err)
			}

			if tt.expected == nil && result != nil {
				t.Errorf("expected nil result but got %v", result)
				return
			}

			// Sort results for consistent comparison
			if len(result) != len(tt.expected) {
				t.Errorf("expected %d handlers but got %d: %v", len(tt.expected), len(result), result)
				return
			}

			// Create a map for easier comparison
			resultMap := make(map[string]bool)
			for _, h := range result {
				resultMap[h] = true
			}

			for _, expected := range tt.expected {
				if !resultMap[expected] {
					t.Errorf("expected handler %s not found in result %v", expected, result)
				}
			}
		})
	}
}
