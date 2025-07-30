package registry

import (
	"fmt"
	"sort"
	"sync"
	"testing"

	"github.com/arthur-debert/dodot/pkg/errors"
)

// TestItem is a simple type for testing
type TestItem struct {
	ID    int
	Name  string
	Value string
}

func TestNew(t *testing.T) {
	reg := New[TestItem]()

	if reg == nil {
		t.Fatal("New() returned nil")
	}

	if reg.Count() != 0 {
		t.Errorf("New registry should be empty, got count %d", reg.Count())
	}
}

func TestRegister(t *testing.T) {
	reg := New[TestItem]()

	t.Run("register valid item", func(t *testing.T) {
		item := TestItem{ID: 1, Name: "test", Value: "value1"}
		err := reg.Register("item1", item)

		if err != nil {
			t.Fatalf("Register() error = %v, want nil", err)
		}

		if reg.Count() != 1 {
			t.Errorf("Count() = %d, want 1", reg.Count())
		}
	})

	t.Run("register with empty name", func(t *testing.T) {
		item := TestItem{ID: 2, Name: "test2", Value: "value2"}
		err := reg.Register("", item)

		if !errors.IsErrorCode(err, errors.ErrInvalidInput) {
			t.Errorf("Register() with empty name should return ErrInvalidInput, got %v", err)
		}
	})

	t.Run("register duplicate", func(t *testing.T) {
		item := TestItem{ID: 3, Name: "test3", Value: "value3"}
		err := reg.Register("item1", item)

		if !errors.IsErrorCode(err, errors.ErrAlreadyExists) {
			t.Errorf("Register() duplicate should return ErrAlreadyExists, got %v", err)
		}
	})
}

func TestGet(t *testing.T) {
	reg := New[TestItem]()
	item := TestItem{ID: 1, Name: "test", Value: "value1"}
	_ = reg.Register("item1", item)

	t.Run("get existing item", func(t *testing.T) {
		got, err := reg.Get("item1")

		if err != nil {
			t.Fatalf("Get() error = %v, want nil", err)
		}

		if got.ID != item.ID || got.Name != item.Name || got.Value != item.Value {
			t.Errorf("Get() = %+v, want %+v", got, item)
		}
	})

	t.Run("get non-existing item", func(t *testing.T) {
		_, err := reg.Get("nonexistent")

		if !errors.IsErrorCode(err, errors.ErrNotFound) {
			t.Errorf("Get() non-existing should return ErrNotFound, got %v", err)
		}
	})
}

func TestRemove(t *testing.T) {
	reg := New[TestItem]()
	item := TestItem{ID: 1, Name: "test", Value: "value1"}
	_ = reg.Register("item1", item)

	t.Run("remove existing item", func(t *testing.T) {
		err := reg.Remove("item1")

		if err != nil {
			t.Fatalf("Remove() error = %v, want nil", err)
		}

		if reg.Has("item1") {
			t.Error("Item should not exist after removal")
		}
	})

	t.Run("remove non-existing item", func(t *testing.T) {
		err := reg.Remove("nonexistent")

		if !errors.IsErrorCode(err, errors.ErrNotFound) {
			t.Errorf("Remove() non-existing should return ErrNotFound, got %v", err)
		}
	})
}

func TestList(t *testing.T) {
	reg := New[TestItem]()

	// Register items in non-alphabetical order
	items := []string{"charlie", "alpha", "bravo"}
	for i, name := range items {
		_ = reg.Register(name, TestItem{ID: i})
	}

	list := reg.List()
	expected := []string{"alpha", "bravo", "charlie"}

	if len(list) != len(expected) {
		t.Fatalf("List() returned %d items, want %d", len(list), len(expected))
	}

	for i, name := range list {
		if name != expected[i] {
			t.Errorf("List()[%d] = %s, want %s", i, name, expected[i])
		}
	}
}

func TestHas(t *testing.T) {
	reg := New[TestItem]()
	_ = reg.Register("item1", TestItem{ID: 1})

	tests := []struct {
		name     string
		itemName string
		want     bool
	}{
		{"existing item", "item1", true},
		{"non-existing item", "item2", false},
		{"empty name", "", false},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if got := reg.Has(tt.itemName); got != tt.want {
				t.Errorf("Has(%s) = %v, want %v", tt.itemName, got, tt.want)
			}
		})
	}
}

func TestClear(t *testing.T) {
	reg := New[TestItem]()

	// Register multiple items
	for i := 0; i < 5; i++ {
		_ = reg.Register(fmt.Sprintf("item%d", i), TestItem{ID: i})
	}

	if reg.Count() != 5 {
		t.Fatalf("Expected 5 items before clear, got %d", reg.Count())
	}

	reg.Clear()

	if reg.Count() != 0 {
		t.Errorf("Count() after Clear() = %d, want 0", reg.Count())
	}

	if len(reg.List()) != 0 {
		t.Errorf("List() after Clear() should be empty")
	}
}

func TestCount(t *testing.T) {
	reg := New[TestItem]()

	for i := 0; i < 3; i++ {
		if reg.Count() != i {
			t.Errorf("Count() = %d, want %d", reg.Count(), i)
		}
		_ = reg.Register(fmt.Sprintf("item%d", i), TestItem{ID: i})
	}
}

func TestConcurrency(t *testing.T) {
	reg := New[TestItem]()
	const goroutines = 10
	const itemsPerGoroutine = 100

	// Test concurrent writes
	var wg sync.WaitGroup
	wg.Add(goroutines)

	for g := 0; g < goroutines; g++ {
		go func(goroutineID int) {
			defer wg.Done()
			for i := 0; i < itemsPerGoroutine; i++ {
				name := fmt.Sprintf("g%d_item%d", goroutineID, i)
				item := TestItem{ID: goroutineID*1000 + i}
				if err := reg.Register(name, item); err != nil {
					t.Errorf("Concurrent Register() failed: %v", err)
				}
			}
		}(g)
	}

	wg.Wait()

	expectedCount := goroutines * itemsPerGoroutine
	if reg.Count() != expectedCount {
		t.Errorf("Count() after concurrent writes = %d, want %d", reg.Count(), expectedCount)
	}

	// Test concurrent reads
	wg.Add(goroutines)

	for g := 0; g < goroutines; g++ {
		go func(goroutineID int) {
			defer wg.Done()
			for i := 0; i < itemsPerGoroutine; i++ {
				name := fmt.Sprintf("g%d_item%d", goroutineID, i)
				if _, err := reg.Get(name); err != nil {
					t.Errorf("Concurrent Get() failed: %v", err)
				}
			}
		}(g)
	}

	wg.Wait()
}

func TestMustRegister(t *testing.T) {
	reg := New[TestItem]()

	t.Run("successful registration", func(t *testing.T) {
		// Should not panic
		MustRegister(reg, "item1", TestItem{ID: 1})

		if !reg.Has("item1") {
			t.Error("MustRegister() should have registered the item")
		}
	})

	t.Run("panic on duplicate", func(t *testing.T) {
		defer func() {
			if r := recover(); r == nil {
				t.Error("MustRegister() should panic on duplicate registration")
			}
		}()

		MustRegister(reg, "item1", TestItem{ID: 2})
	})
}

func TestMustGet(t *testing.T) {
	reg := New[TestItem]()
	item := TestItem{ID: 1, Name: "test"}
	_ = reg.Register("item1", item)

	t.Run("successful get", func(t *testing.T) {
		// Should not panic
		got := MustGet[TestItem](reg, "item1")

		if got.ID != item.ID {
			t.Errorf("MustGet() = %+v, want %+v", got, item)
		}
	})

	t.Run("panic on not found", func(t *testing.T) {
		defer func() {
			if r := recover(); r == nil {
				t.Error("MustGet() should panic when item not found")
			}
		}()

		MustGet[TestItem](reg, "nonexistent")
	})
}

// Plugin interface for testing
type Plugin interface {
	Name() string
	Execute() error
}

type testPlugin struct {
	name string
}

func (p *testPlugin) Name() string   { return p.name }
func (p *testPlugin) Execute() error { return nil }

// TestWithInterfaces tests registry with interface types
func TestWithInterfaces(t *testing.T) {
	reg := New[Plugin]()

	plugin1 := &testPlugin{name: "plugin1"}
	plugin2 := &testPlugin{name: "plugin2"}

	_ = reg.Register("p1", plugin1)
	_ = reg.Register("p2", plugin2)

	if reg.Count() != 2 {
		t.Errorf("Count() = %d, want 2", reg.Count())
	}

	got, err := reg.Get("p1")
	if err != nil {
		t.Fatalf("Get() error = %v", err)
	}

	if got.Name() != "plugin1" {
		t.Errorf("Get() returned wrong plugin: %s", got.Name())
	}
}

// TestWithFunctions tests registry with function types
func TestWithFunctions(t *testing.T) {
	type HandlerFunc func(string) error

	reg := New[HandlerFunc]()

	handler1 := func(s string) error { return nil }
	handler2 := func(s string) error { return fmt.Errorf("error: %s", s) }

	_ = reg.Register("handler1", handler1)
	_ = reg.Register("handler2", handler2)

	h, err := reg.Get("handler2")
	if err != nil {
		t.Fatalf("Get() error = %v", err)
	}

	if err := h("test"); err == nil || err.Error() != "error: test" {
		t.Error("Retrieved function doesn't behave as expected")
	}
}

// Benchmark tests
func BenchmarkRegister(b *testing.B) {
	reg := New[TestItem]()

	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		name := fmt.Sprintf("item%d", i)
		_ = reg.Register(name, TestItem{ID: i})
	}
}

func BenchmarkGet(b *testing.B) {
	reg := New[TestItem]()

	// Pre-populate registry
	for i := 0; i < 1000; i++ {
		_ = reg.Register(fmt.Sprintf("item%d", i), TestItem{ID: i})
	}

	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		name := fmt.Sprintf("item%d", i%1000)
		_, _ = reg.Get(name)
	}
}

func BenchmarkList(b *testing.B) {
	reg := New[TestItem]()

	// Pre-populate registry
	for i := 0; i < 100; i++ {
		_ = reg.Register(fmt.Sprintf("item%d", i), TestItem{ID: i})
	}

	b.ResetTimer()
	for i := 0; i < b.N; i++ {
		_ = reg.List()
	}
}

// Example usage
func ExampleRegistry() {
	// Create a registry for string handlers
	reg := New[func() string]()

	// Register some handlers
	_ = reg.Register("greeting", func() string { return "Hello, World!" })
	_ = reg.Register("farewell", func() string { return "Goodbye!" })

	// List all registered handlers
	names := reg.List()
	sort.Strings(names)
	fmt.Println("Registered handlers:", names)

	// Get and execute a handler
	if handler, err := reg.Get("greeting"); err == nil {
		fmt.Println(handler())
	}

	// Output:
	// Registered handlers: [farewell greeting]
	// Hello, World!
}
