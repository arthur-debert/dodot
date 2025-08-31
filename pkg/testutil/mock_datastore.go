// Package testutil provides testing utilities
package testutil

import (
	"fmt"
	"sync"

	"github.com/arthur-debert/dodot/pkg/types"
)

// MockDataStore is a mock implementation of types.DataStore for testing
type MockDataStore struct {
	mu            sync.RWMutex
	dataLinks     map[string]string // pack:handler:source -> datastorePath
	userLinks     map[string]string // userPath -> datastorePath
	sentinels     map[string]bool   // pack:handler:sentinel -> exists
	commands      map[string]string // pack:handler:sentinel -> command
	calls         []string
	errorOn       string
	errorToReturn error
}

// NewMockDataStore creates a new mock DataStore
func NewMockDataStore() *MockDataStore {
	return &MockDataStore{
		dataLinks: make(map[string]string),
		userLinks: make(map[string]string),
		sentinels: make(map[string]bool),
		commands:  make(map[string]string),
		calls:     []string{},
	}
}

// CreateDataLink creates an intermediate symlink in the datastore
func (m *MockDataStore) CreateDataLink(pack, handlerName, sourceFile string) (string, error) {
	m.mu.Lock()
	defer m.mu.Unlock()

	m.calls = append(m.calls, fmt.Sprintf("CreateDataLink(%s,%s,%s)", pack, handlerName, sourceFile))

	if m.errorOn == "CreateDataLink" {
		return "", m.errorToReturn
	}

	key := fmt.Sprintf("%s:%s:%s", pack, handlerName, sourceFile)
	datastorePath := fmt.Sprintf("/datastore/%s/%s/%s", pack, handlerName, sourceFile)
	m.dataLinks[key] = datastorePath

	return datastorePath, nil
}

// CreateUserLink creates a user-facing symlink
func (m *MockDataStore) CreateUserLink(datastorePath, userPath string) error {
	m.mu.Lock()
	defer m.mu.Unlock()

	m.calls = append(m.calls, fmt.Sprintf("CreateUserLink(%s,%s)", datastorePath, userPath))

	if m.errorOn == "CreateUserLink" {
		return m.errorToReturn
	}

	m.userLinks[userPath] = datastorePath
	return nil
}

// RunAndRecord executes a command and records completion with a sentinel
func (m *MockDataStore) RunAndRecord(pack, handlerName, command, sentinel string) error {
	m.mu.Lock()
	defer m.mu.Unlock()

	m.calls = append(m.calls, fmt.Sprintf("RunAndRecord(%s,%s,%s,%s)", pack, handlerName, command, sentinel))

	if m.errorOn == "RunAndRecord" {
		return m.errorToReturn
	}

	key := fmt.Sprintf("%s:%s:%s", pack, handlerName, sentinel)
	m.sentinels[key] = true
	m.commands[key] = command

	return nil
}

// HasSentinel checks if an operation has been completed
func (m *MockDataStore) HasSentinel(pack, handlerName, sentinel string) (bool, error) {
	m.mu.RLock()
	defer m.mu.RUnlock()

	m.calls = append(m.calls, fmt.Sprintf("HasSentinel(%s,%s,%s)", pack, handlerName, sentinel))

	if m.errorOn == "HasSentinel" {
		return false, m.errorToReturn
	}

	key := fmt.Sprintf("%s:%s:%s", pack, handlerName, sentinel)
	return m.sentinels[key], nil
}

// RemoveState removes all state for a handler in a pack
func (m *MockDataStore) RemoveState(pack, handlerName string) error {
	m.mu.Lock()
	defer m.mu.Unlock()

	m.calls = append(m.calls, fmt.Sprintf("RemoveState(%s,%s)", pack, handlerName))

	if m.errorOn == "RemoveState" {
		return m.errorToReturn
	}

	// Remove all data links for this pack/handler
	prefix := fmt.Sprintf("%s:%s:", pack, handlerName)
	for key := range m.dataLinks {
		if len(key) >= len(prefix) && key[:len(prefix)] == prefix {
			delete(m.dataLinks, key)
		}
	}

	// Remove all sentinels for this pack/handler
	for key := range m.sentinels {
		if len(key) >= len(prefix) && key[:len(prefix)] == prefix {
			delete(m.sentinels, key)
		}
	}

	return nil
}

// Test helper methods

// WithError configures the mock to return an error for a specific method
func (m *MockDataStore) WithError(method string, err error) *MockDataStore {
	m.errorOn = method
	m.errorToReturn = err
	return m
}

// WithDataLink pre-configures a data link
func (m *MockDataStore) WithDataLink(pack, handler, source, datastorePath string) *MockDataStore {
	m.mu.Lock()
	defer m.mu.Unlock()

	key := fmt.Sprintf("%s:%s:%s", pack, handler, source)
	m.dataLinks[key] = datastorePath
	return m
}

// WithUserLink pre-configures a user link
func (m *MockDataStore) WithUserLink(userPath, datastorePath string) *MockDataStore {
	m.mu.Lock()
	defer m.mu.Unlock()

	m.userLinks[userPath] = datastorePath
	return m
}

// WithSentinel pre-configures a sentinel
func (m *MockDataStore) WithSentinel(pack, handler, sentinel string, exists bool) *MockDataStore {
	m.mu.Lock()
	defer m.mu.Unlock()

	key := fmt.Sprintf("%s:%s:%s", pack, handler, sentinel)
	m.sentinels[key] = exists
	return m
}

// GetCalls returns all recorded method calls
func (m *MockDataStore) GetCalls() []string {
	m.mu.RLock()
	defer m.mu.RUnlock()

	result := make([]string, len(m.calls))
	copy(result, m.calls)
	return result
}

// GetDataLinks returns all data links (for testing)
func (m *MockDataStore) GetDataLinks() map[string]string {
	m.mu.RLock()
	defer m.mu.RUnlock()

	result := make(map[string]string)
	for k, v := range m.dataLinks {
		result[k] = v
	}
	return result
}

// GetUserLinks returns all user links (for testing)
func (m *MockDataStore) GetUserLinks() map[string]string {
	m.mu.RLock()
	defer m.mu.RUnlock()

	result := make(map[string]string)
	for k, v := range m.userLinks {
		result[k] = v
	}
	return result
}

// GetSentinels returns all sentinels (for testing)
func (m *MockDataStore) GetSentinels() map[string]bool {
	m.mu.RLock()
	defer m.mu.RUnlock()

	result := make(map[string]bool)
	for k, v := range m.sentinels {
		result[k] = v
	}
	return result
}

// Reset clears all state
func (m *MockDataStore) Reset() {
	m.mu.Lock()
	defer m.mu.Unlock()

	m.dataLinks = make(map[string]string)
	m.userLinks = make(map[string]string)
	m.sentinels = make(map[string]bool)
	m.commands = make(map[string]string)
	m.calls = []string{}
	m.errorOn = ""
	m.errorToReturn = nil
}

// HasHandlerState checks if any state exists for a handler in a pack
func (m *MockDataStore) HasHandlerState(pack, handlerName string) (bool, error) {
	m.mu.RLock()
	defer m.mu.RUnlock()

	m.calls = append(m.calls, fmt.Sprintf("HasHandlerState(%s,%s)", pack, handlerName))

	if m.errorOn == "HasHandlerState" {
		return false, m.errorToReturn
	}

	// Check if any sentinels exist for this pack/handler combination
	prefix := fmt.Sprintf("%s:%s:", pack, handlerName)
	for key := range m.sentinels {
		if len(key) > len(prefix) && key[:len(prefix)] == prefix {
			return true, nil
		}
	}

	return false, nil
}

// ListPackHandlers returns a list of all handlers that have state for a given pack
func (m *MockDataStore) ListPackHandlers(pack string) ([]string, error) {
	m.mu.RLock()
	defer m.mu.RUnlock()

	m.calls = append(m.calls, fmt.Sprintf("ListPackHandlers(%s)", pack))

	if m.errorOn == "ListPackHandlers" {
		return nil, m.errorToReturn
	}

	handlers := make(map[string]bool)
	prefix := pack + ":"

	// Find all unique handlers for this pack
	for key := range m.sentinels {
		if len(key) > len(prefix) && key[:len(prefix)] == prefix {
			parts := splitKey(key)
			if len(parts) >= 2 {
				handlers[parts[1]] = true
			}
		}
	}

	// Convert map to slice
	result := make([]string, 0, len(handlers))
	for handler := range handlers {
		result = append(result, handler)
	}

	return result, nil
}

// ListHandlerSentinels returns all sentinel files for a specific handler in a pack
func (m *MockDataStore) ListHandlerSentinels(pack, handlerName string) ([]string, error) {
	m.mu.RLock()
	defer m.mu.RUnlock()

	m.calls = append(m.calls, fmt.Sprintf("ListHandlerSentinels(%s,%s)", pack, handlerName))

	if m.errorOn == "ListHandlerSentinels" {
		return nil, m.errorToReturn
	}

	var sentinels []string
	prefix := fmt.Sprintf("%s:%s:", pack, handlerName)

	for key := range m.sentinels {
		if len(key) > len(prefix) && key[:len(prefix)] == prefix {
			parts := splitKey(key)
			if len(parts) >= 3 {
				sentinels = append(sentinels, parts[2])
			}
		}
	}

	return sentinels, nil
}

// splitKey splits a colon-separated key into parts
func splitKey(key string) []string {
	var parts []string
	start := 0
	for i, ch := range key {
		if ch == ':' {
			parts = append(parts, key[start:i])
			start = i + 1
		}
	}
	if start < len(key) {
		parts = append(parts, key[start:])
	}
	return parts
}

// SetSentinel sets a sentinel value for testing
func (m *MockDataStore) SetSentinel(pack, handlerName, sentinel string, exists bool) {
	m.mu.Lock()
	defer m.mu.Unlock()

	key := fmt.Sprintf("%s:%s:%s", pack, handlerName, sentinel)
	if exists {
		m.sentinels[key] = true
	} else {
		delete(m.sentinels, key)
	}
}

// Verify interface compliance
var _ types.DataStore = (*MockDataStore)(nil)
