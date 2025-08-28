package testutil

import (
	"fmt"
	"sync"
	
	"github.com/arthur-debert/dodot/pkg/types"
)

// MockDataStore provides a mock implementation of types.DataStore for testing
type MockDataStore struct {
	mu            sync.RWMutex
	links         map[string]string              // pack:source -> target
	paths         map[string][]string            // pack -> paths
	provisioning  map[string]map[string]string   // pack:handler -> checksum
	shellProfiles map[string]string              // pack:script -> status
	
	// Error injection
	errorOn      string
	errorToReturn error
	
	// Call tracking
	calls []string
}

// NewMockDataStore creates a new mock data store
func NewMockDataStore() *MockDataStore {
	return &MockDataStore{
		links:         make(map[string]string),
		paths:         make(map[string][]string),
		provisioning:  make(map[string]map[string]string),
		shellProfiles: make(map[string]string),
		calls:         []string{},
	}
}

// Link records a symlink in the data store
func (m *MockDataStore) Link(pack, sourceFile string) (string, error) {
	m.mu.Lock()
	defer m.mu.Unlock()
	
	m.calls = append(m.calls, fmt.Sprintf("Link(%s,%s)", pack, sourceFile))
	
	if m.errorOn == "Link" {
		return "", m.errorToReturn
	}
	
	key := fmt.Sprintf("%s:%s", pack, sourceFile)
	target := fmt.Sprintf("/home/.%s", sourceFile)
	m.links[key] = target
	
	return target, nil
}

// Unlink removes a symlink record from the data store
func (m *MockDataStore) Unlink(pack, sourceFile string) error {
	m.mu.Lock()
	defer m.mu.Unlock()
	
	m.calls = append(m.calls, fmt.Sprintf("Unlink(%s,%s)", pack, sourceFile))
	
	if m.errorOn == "Unlink" {
		return m.errorToReturn
	}
	
	key := fmt.Sprintf("%s:%s", pack, sourceFile)
	delete(m.links, key)
	
	return nil
}

// GetStatus returns the status of a file
func (m *MockDataStore) GetStatus(pack, sourceFile string) (types.Status, error) {
	m.mu.RLock()
	defer m.mu.RUnlock()
	
	m.calls = append(m.calls, fmt.Sprintf("GetStatus(%s,%s)", pack, sourceFile))
	
	if m.errorOn == "GetStatus" {
		return types.Status{}, m.errorToReturn
	}
	
	key := fmt.Sprintf("%s:%s", pack, sourceFile)
	if target, exists := m.links[key]; exists {
		return types.Status{
			Type:   types.StatusLinked,
			Target: target,
		}, nil
	}
	
	return types.Status{
		Type: types.StatusNotLinked,
	}, nil
}

// AddToPath adds a directory to PATH
func (m *MockDataStore) AddToPath(pack, dirPath string) error {
	m.mu.Lock()
	defer m.mu.Unlock()
	
	m.calls = append(m.calls, fmt.Sprintf("AddToPath(%s,%s)", pack, dirPath))
	
	if m.errorOn == "AddToPath" {
		return m.errorToReturn
	}
	
	m.paths[pack] = append(m.paths[pack], dirPath)
	return nil
}

// NeedsProvisioning checks if a pack/handler needs provisioning
func (m *MockDataStore) NeedsProvisioning(pack, sentinelName, checksum string) (bool, error) {
	m.mu.RLock()
	defer m.mu.RUnlock()
	
	m.calls = append(m.calls, fmt.Sprintf("NeedsProvisioning(%s,%s,%s)", pack, sentinelName, checksum))
	
	if m.errorOn == "NeedsProvisioning" {
		return false, m.errorToReturn
	}
	
	if packMap, exists := m.provisioning[pack]; exists {
		if storedChecksum, exists := packMap[sentinelName]; exists {
			return storedChecksum != checksum, nil
		}
	}
	
	return true, nil // Not provisioned yet
}

// RecordProvisioning records that a handler has been provisioned
func (m *MockDataStore) RecordProvisioning(pack, sentinelName, checksum string) error {
	m.mu.Lock()
	defer m.mu.Unlock()
	
	m.calls = append(m.calls, fmt.Sprintf("RecordProvisioning(%s,%s,%s)", pack, sentinelName, checksum))
	
	if m.errorOn == "RecordProvisioning" {
		return m.errorToReturn
	}
	
	if _, exists := m.provisioning[pack]; !exists {
		m.provisioning[pack] = make(map[string]string)
	}
	
	m.provisioning[pack][sentinelName] = checksum
	return nil
}

// WithError configures the mock to return an error for a specific method
func (m *MockDataStore) WithError(method string, err error) *MockDataStore {
	m.errorOn = method
	m.errorToReturn = err
	return m
}

// WithLink sets up an existing link in the mock
func (m *MockDataStore) WithLink(pack, source, target string) *MockDataStore {
	m.mu.Lock()
	defer m.mu.Unlock()
	
	key := fmt.Sprintf("%s:%s", pack, source)
	m.links[key] = target
	return m
}

// WithBrokenLink sets up a broken link (link exists but target doesn't)
func (m *MockDataStore) WithBrokenLink(pack, source, target string) *MockDataStore {
	// For mock purposes, we just record it as a regular link
	// The test would need to check if the target exists separately
	return m.WithLink(pack, source, target)
}

// WithProvisioningState sets up existing provisioning state
func (m *MockDataStore) WithProvisioningState(pack, handler string, provisioned bool) *MockDataStore {
	m.mu.Lock()
	defer m.mu.Unlock()
	
	if _, exists := m.provisioning[pack]; !exists {
		m.provisioning[pack] = make(map[string]string)
	}
	
	if provisioned {
		m.provisioning[pack][handler] = "mock-checksum"
	} else {
		delete(m.provisioning[pack], handler)
	}
	
	return m
}

// GetCalls returns all method calls made to the mock
func (m *MockDataStore) GetCalls() []string {
	m.mu.RLock()
	defer m.mu.RUnlock()
	
	calls := make([]string, len(m.calls))
	copy(calls, m.calls)
	return calls
}

// Reset clears all state and call history
func (m *MockDataStore) Reset() {
	m.mu.Lock()
	defer m.mu.Unlock()
	
	m.links = make(map[string]string)
	m.paths = make(map[string][]string)
	m.provisioning = make(map[string]map[string]string)
	m.shellProfiles = make(map[string]string)
	m.calls = []string{}
	m.errorOn = ""
	m.errorToReturn = nil
}