package registry

import (
	"fmt"
	"sort"
	"sync"

	"github.com/arthur-debert/dodot/pkg/errors"
)

// Registry is a generic, thread-safe registry for storing and retrieving items by name
type Registry[T any] interface {
	// Register adds an item to the registry
	Register(name string, item T) error

	// Get retrieves an item from the registry
	Get(name string) (T, error)

	// Remove removes an item from the registry
	Remove(name string) error

	// List returns all registered names
	List() []string

	// Has checks if an item is registered
	Has(name string) bool

	// Clear removes all items from the registry
	Clear()

	// Count returns the number of registered items
	Count() int
}

// registry is the internal implementation of Registry
type registry[T any] struct {
	mu    sync.RWMutex
	items map[string]T
}

// New creates a new Registry instance
func New[T any]() Registry[T] {
	return &registry[T]{
		items: make(map[string]T),
	}
}

// Register adds an item to the registry
func (r *registry[T]) Register(name string, item T) error {
	if name == "" {
		return errors.New(errors.ErrInvalidInput, "registry name cannot be empty")
	}

	r.mu.Lock()
	defer r.mu.Unlock()

	if _, exists := r.items[name]; exists {
		return errors.Newf(errors.ErrAlreadyExists, "item '%s' is already registered", name)
	}

	r.items[name] = item
	return nil
}

// Get retrieves an item from the registry
func (r *registry[T]) Get(name string) (T, error) {
	r.mu.RLock()
	defer r.mu.RUnlock()

	item, exists := r.items[name]
	if !exists {
		var zero T
		return zero, errors.Newf(errors.ErrNotFound, "item '%s' not found in registry", name)
	}

	return item, nil
}

// Remove removes an item from the registry
func (r *registry[T]) Remove(name string) error {
	r.mu.Lock()
	defer r.mu.Unlock()

	if _, exists := r.items[name]; !exists {
		return errors.Newf(errors.ErrNotFound, "item '%s' not found in registry", name)
	}

	delete(r.items, name)
	return nil
}

// List returns all registered names in sorted order
func (r *registry[T]) List() []string {
	r.mu.RLock()
	defer r.mu.RUnlock()

	names := make([]string, 0, len(r.items))
	for name := range r.items {
		names = append(names, name)
	}

	sort.Strings(names)
	return names
}

// Has checks if an item is registered
func (r *registry[T]) Has(name string) bool {
	r.mu.RLock()
	defer r.mu.RUnlock()

	_, exists := r.items[name]
	return exists
}

// Clear removes all items from the registry
func (r *registry[T]) Clear() {
	r.mu.Lock()
	defer r.mu.Unlock()

	r.items = make(map[string]T)
}

// Count returns the number of registered items
func (r *registry[T]) Count() int {
	r.mu.RLock()
	defer r.mu.RUnlock()

	return len(r.items)
}

// MustRegister registers an item and panics if registration fails
// This is useful for init() functions where registration errors are programming errors
func MustRegister[T any](reg Registry[T], name string, item T) {
	if err := reg.Register(name, item); err != nil {
		panic(fmt.Sprintf("failed to register %s: %v", name, err))
	}
}

// MustGet retrieves an item and panics if not found
// This is useful when the item must exist
func MustGet[T any](reg Registry[T], name string) T {
	item, err := reg.Get(name)
	if err != nil {
		panic(fmt.Sprintf("failed to get %s: %v", name, err))
	}
	return item
}
