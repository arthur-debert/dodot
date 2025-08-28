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

func TestMemoryFS_SymlinkOperations(t *testing.T) {
	fs := NewMemoryFS()

	t.Run("Symlink_NonExistentTarget", func(t *testing.T) {
		// Symlinks can point to non-existent targets
		err := fs.Symlink("/nonexistent", "/link")
		if err != nil {
			t.Fatalf("Symlink to non-existent target should succeed: %v", err)
		}

		dest, err := fs.Readlink("/link")
		if err != nil {
			t.Fatalf("Readlink failed: %v", err)
		}
		if dest != "/nonexistent" {
			t.Errorf("wrong destination: got %q, want %q", dest, "/nonexistent")
		}
	})

	t.Run("Symlink_Overwrite", func(t *testing.T) {
		// Create initial symlink
		fs.WriteFile("/target1.txt", []byte("target1"), 0644)
		fs.WriteFile("/target2.txt", []byte("target2"), 0644)
		
		err := fs.Symlink("/target1.txt", "/link.txt")
		if err != nil {
			t.Fatalf("First symlink failed: %v", err)
		}

		// Try to create symlink at same path - should fail
		err = fs.Symlink("/target2.txt", "/link.txt")
		if err == nil {
			t.Error("Creating symlink over existing file should fail")
		}
	})

	t.Run("Symlink_RelativePaths", func(t *testing.T) {
		// Create a directory structure
		fs.MkdirAll("/dir/subdir", 0755)
		fs.WriteFile("/dir/target.txt", []byte("content"), 0644)
		
		// Create symlink with relative path
		err := fs.Symlink("../target.txt", "/dir/subdir/link.txt")
		if err != nil {
			t.Fatalf("Symlink with relative path failed: %v", err)
		}

		dest, err := fs.Readlink("/dir/subdir/link.txt")
		if err != nil {
			t.Fatalf("Readlink failed: %v", err)
		}
		if dest != "../target.txt" {
			t.Errorf("wrong destination: got %q, want %q", dest, "../target.txt")
		}
	})

	t.Run("Readlink_NotALink", func(t *testing.T) {
		// Create regular file
		fs.WriteFile("/regular.txt", []byte("content"), 0644)
		
		// Try to read it as symlink
		_, err := fs.Readlink("/regular.txt")
		if err == nil {
			t.Error("Readlink on regular file should fail")
		}
	})

	t.Run("Readlink_NonExistent", func(t *testing.T) {
		_, err := fs.Readlink("/nonexistent")
		if err == nil {
			t.Error("Readlink on non-existent file should fail")
		}
	})

	t.Run("Lstat_Symlink", func(t *testing.T) {
		// Create target and symlink
		fs.WriteFile("/target.txt", []byte("content"), 0644)
		fs.Symlink("/target.txt", "/link.txt")

		// Lstat should show info about the link itself
		info, err := fs.Lstat("/link.txt")
		if err != nil {
			t.Fatalf("Lstat failed: %v", err)
		}

		// Check that it reports as symlink
		if info.Mode()&os.ModeSymlink == 0 {
			t.Error("Lstat should report file as symlink")
		}
	})

	t.Run("Remove_Symlink", func(t *testing.T) {
		// Create target and symlink
		fs.WriteFile("/target.txt", []byte("content"), 0644)
		fs.Symlink("/target.txt", "/link.txt")

		// Remove the symlink
		err := fs.Remove("/link.txt")
		if err != nil {
			t.Fatalf("Remove symlink failed: %v", err)
		}

		// Target should still exist
		if _, err := fs.Stat("/target.txt"); err != nil {
			t.Error("Target should still exist after removing symlink")
		}

		// Symlink should be gone
		if _, err := fs.Lstat("/link.txt"); err == nil {
			t.Error("Symlink should be removed")
		}
	})

	t.Run("SymlinkChain", func(t *testing.T) {
		// Create a chain of symlinks
		fs.WriteFile("/target.txt", []byte("content"), 0644)
		fs.Symlink("/target.txt", "/link1.txt")
		fs.Symlink("/link1.txt", "/link2.txt")
		fs.Symlink("/link2.txt", "/link3.txt")

		// Each readlink should return immediate target
		dest, _ := fs.Readlink("/link1.txt")
		if dest != "/target.txt" {
			t.Errorf("link1 wrong target: %q", dest)
		}

		dest, _ = fs.Readlink("/link2.txt")
		if dest != "/link1.txt" {
			t.Errorf("link2 wrong target: %q", dest)
		}

		dest, _ = fs.Readlink("/link3.txt")
		if dest != "/link2.txt" {
			t.Errorf("link3 wrong target: %q", dest)
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