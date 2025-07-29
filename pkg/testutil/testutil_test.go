package testutil

import (
	"path/filepath"
	"runtime"
	"testing"
)

func TestCreateFile(t *testing.T) {
	dir := TempDir(t, "test-create-file")
	
	// Test simple file creation
	path := CreateFile(t, dir, "test.txt", "hello world")
	
	if !FileExists(t, path) {
		t.Error("File was not created")
	}
	
	content := ReadFile(t, path)
	if content != "hello world" {
		t.Errorf("File content = %q, want %q", content, "hello world")
	}
	
	// Test file creation in subdirectory
	path2 := CreateFile(t, dir, "sub/dir/test2.txt", "nested")
	
	if !FileExists(t, path2) {
		t.Error("Nested file was not created")
	}
}

func TestCreateDir(t *testing.T) {
	parent := TempDir(t, "test-create-dir")
	
	// Test simple directory creation
	dir := CreateDir(t, parent, "subdir")
	
	if !DirExists(t, dir) {
		t.Error("Directory was not created")
	}
	
	// Test nested directory creation
	nested := CreateDir(t, parent, "a/b/c")
	
	if !DirExists(t, nested) {
		t.Error("Nested directory was not created")
	}
}

func TestCreateSymlink(t *testing.T) {
	if runtime.GOOS == "windows" {
		t.Skip("Symlink tests not supported on Windows")
	}
	
	dir := TempDir(t, "test-symlink")
	
	// Create a target file
	target := CreateFile(t, dir, "target.txt", "target content")
	link := filepath.Join(dir, "link.txt")
	
	CreateSymlink(t, target, link)
	
	if !SymlinkExists(t, link) {
		t.Error("Symlink was not created")
	}
	
	actualTarget := ReadSymlink(t, link)
	if actualTarget != target {
		t.Errorf("Symlink target = %q, want %q", actualTarget, target)
	}
}

func TestFileExists(t *testing.T) {
	dir := TempDir(t, "test-exists")
	
	// Test with existing file
	file := CreateFile(t, dir, "exists.txt", "content")
	if !FileExists(t, file) {
		t.Error("FileExists returned false for existing file")
	}
	
	// Test with non-existing file
	if FileExists(t, filepath.Join(dir, "notexists.txt")) {
		t.Error("FileExists returned true for non-existing file")
	}
	
	// Test with directory
	subdir := CreateDir(t, dir, "subdir")
	if FileExists(t, subdir) {
		t.Error("FileExists returned true for directory")
	}
}

func TestDirExists(t *testing.T) {
	dir := TempDir(t, "test-dir-exists")
	
	// Test with existing directory
	subdir := CreateDir(t, dir, "subdir")
	if !DirExists(t, subdir) {
		t.Error("DirExists returned false for existing directory")
	}
	
	// Test with non-existing directory
	if DirExists(t, filepath.Join(dir, "notexists")) {
		t.Error("DirExists returned true for non-existing directory")
	}
	
	// Test with file
	file := CreateFile(t, dir, "file.txt", "content")
	if DirExists(t, file) {
		t.Error("DirExists returned true for file")
	}
}

func TestReadFile(t *testing.T) {
	dir := TempDir(t, "test-read")
	file := CreateFile(t, dir, "read.txt", "test content")
	
	content := ReadFile(t, file)
	if content != "test content" {
		t.Errorf("ReadFile() = %q, want %q", content, "test content")
	}
}

func TestReadSymlink(t *testing.T) {
	if runtime.GOOS == "windows" {
		t.Skip("Symlink tests not supported on Windows")
	}
	
	dir := TempDir(t, "test-read-symlink")
	target := CreateFile(t, dir, "target.txt", "content")
	link := filepath.Join(dir, "link.txt")
	
	CreateSymlink(t, target, link)
	
	actualTarget := ReadSymlink(t, link)
	if actualTarget != target {
		t.Errorf("ReadSymlink() = %q, want %q", actualTarget, target)
	}
}

func TestAssertFileContent(t *testing.T) {
	dir := TempDir(t, "test-assert-content")
	file := CreateFile(t, dir, "test.txt", "expected content")
	
	// This should pass
	AssertFileContent(t, file, "expected content")
}

func TestAssertSymlink(t *testing.T) {
	if runtime.GOOS == "windows" {
		t.Skip("Symlink tests not supported on Windows")
	}
	
	dir := TempDir(t, "test-assert-symlink")
	target := CreateFile(t, dir, "target.txt", "content")
	link := filepath.Join(dir, "link.txt")
	
	CreateSymlink(t, target, link)
	
	// This should pass
	AssertSymlink(t, link, target)
}

func TestAssertNoFile(t *testing.T) {
	dir := TempDir(t, "test-assert-no-file")
	
	// This should pass
	AssertNoFile(t, filepath.Join(dir, "notexists.txt"))
}