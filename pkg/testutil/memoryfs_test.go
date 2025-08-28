// pkg/testutil/memoryfs_test.go
// TEST TYPE: Unit Test
// DEPENDENCIES: None
// PURPOSE: Test MemoryFS implementation

package testutil

import (
	"os"
	"testing"
)

func TestMemoryFS_BasicOperations(t *testing.T) {
	fs := NewMemoryFS()
	
	// Test WriteFile and ReadFile
	t.Run("WriteAndRead", func(t *testing.T) {
		content := []byte("test content")
		err := fs.WriteFile("/test.txt", content, 0644)
		if err != nil {
			t.Fatalf("WriteFile failed: %v", err)
		}
		
		read, err := fs.ReadFile("/test.txt")
		if err != nil {
			t.Fatalf("ReadFile failed: %v", err)
		}
		
		if string(read) != string(content) {
			t.Errorf("content mismatch: got %q, want %q", read, content)
		}
	})
	
	// Test MkdirAll
	t.Run("MkdirAll", func(t *testing.T) {
		err := fs.MkdirAll("/path/to/dir", 0755)
		if err != nil {
			t.Fatalf("MkdirAll failed: %v", err)
		}
		
		info, err := fs.Stat("/path/to/dir")
		if err != nil {
			t.Fatalf("Stat failed: %v", err)
		}
		
		if !info.IsDir() {
			t.Error("created path is not a directory")
		}
	})
	
	// Test Symlink
	t.Run("Symlink", func(t *testing.T) {
		err := fs.WriteFile("/target.txt", []byte("target content"), 0644)
		if err != nil {
			t.Fatalf("WriteFile failed: %v", err)
		}
		
		err = fs.Symlink("/target.txt", "/link.txt")
		if err != nil {
			t.Fatalf("Symlink failed: %v", err)
		}
		
		dest, err := fs.Readlink("/link.txt")
		if err != nil {
			t.Fatalf("Readlink failed: %v", err)
		}
		
		if dest != "/target.txt" {
			t.Errorf("wrong link destination: got %q, want %q", dest, "/target.txt")
		}
	})
}

func TestMemoryFS_ErrorInjection(t *testing.T) {
	fs := NewMemoryFS()
	
	// Inject error
	fs.WithError("/error.txt", os.ErrPermission)
	
	// Try to read - should get injected error
	_, err := fs.ReadFile("/error.txt")
	if err != os.ErrPermission {
		t.Errorf("expected permission error, got: %v", err)
	}
	
	// Try to write - should get injected error
	err = fs.WriteFile("/error.txt", []byte("data"), 0644)
	if err != os.ErrPermission {
		t.Errorf("expected permission error, got: %v", err)
	}
}

func TestMemoryFS_Stats(t *testing.T) {
	fs := NewMemoryFS()
	
	// Initial stats
	reads, writes := fs.Stats()
	if reads != 0 || writes != 0 {
		t.Errorf("initial stats wrong: reads=%d, writes=%d", reads, writes)
	}
	
	// Do some operations
	fs.WriteFile("/file1.txt", []byte("data"), 0644)
	fs.ReadFile("/file1.txt")
	fs.ReadFile("/file1.txt")
	
	reads, writes = fs.Stats()
	if reads != 2 || writes != 1 {
		t.Errorf("stats after operations wrong: reads=%d, writes=%d", reads, writes)
	}
}