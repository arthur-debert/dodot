package synthfs

import (
	"context"
	"crypto/md5"
	"crypto/sha1"
	"crypto/sha256"
	"crypto/sha512"
	"fmt"
	"hash"
	"io"
	"io/fs"

	"github.com/arthur-debert/synthfs/pkg/synthfs/core"
	"github.com/arthur-debert/synthfs/pkg/synthfs/filesystem"
	"github.com/arthur-debert/synthfs/pkg/synthfs/operations"
	"github.com/arthur-debert/synthfs/pkg/synthfs/targets"
)

// SynthFS provides a simplified interface for creating and executing filesystem operations.
type SynthFS struct {
	idGen IDGenerator
}

// New creates a new SynthFS instance with the default ID generator.
func New() *SynthFS {
	return &SynthFS{
		idGen: HashIDGenerator,
	}
}

// WithIDGenerator creates a new SynthFS instance with a custom ID generator.
func WithIDGenerator(gen IDGenerator) *SynthFS {
	return &SynthFS{
		idGen: gen,
	}
}

// CreateFile creates a file creation operation with an auto-generated ID.
func (s *SynthFS) CreateFile(path string, content []byte, mode fs.FileMode) Operation {
	id := s.idGen("create_file", path)
	op := operations.NewCreateFileOperation(id, path)
	item := targets.NewFile(path).WithContent(content).WithMode(mode)
	op.SetItem(item)
	return NewOperationsPackageAdapter(op)
}

// CreateDir creates a directory creation operation with an auto-generated ID.
func (s *SynthFS) CreateDir(path string, mode fs.FileMode) Operation {
	id := s.idGen("create_directory", path)
	op := operations.NewCreateDirectoryOperation(id, path)
	item := targets.NewDirectory(path).WithMode(mode)
	op.SetItem(item)
	return NewOperationsPackageAdapter(op)
}

// Delete creates a delete operation with an auto-generated ID.
func (s *SynthFS) Delete(path string) Operation {
	id := s.idGen("delete", path)
	return NewOperationsPackageAdapter(operations.NewDeleteOperation(id, path))
}

// Copy creates a copy operation with an auto-generated ID.
func (s *SynthFS) Copy(src, dst string) Operation {
	id := s.idGen("copy", src)
	op := operations.NewCopyOperation(id, src)
	op.SetPaths(src, dst)
	return NewOperationsPackageAdapter(op)
}

// Move creates a move operation with an auto-generated ID.
func (s *SynthFS) Move(src, dst string) Operation {
	id := s.idGen("move", src)
	op := operations.NewMoveOperation(id, src)
	op.SetPaths(src, dst)
	return NewOperationsPackageAdapter(op)
}

// CreateSymlink creates a symlink operation with an auto-generated ID.
func (s *SynthFS) CreateSymlink(target, linkPath string) Operation {
	id := s.idGen("create_symlink", linkPath)
	op := operations.NewCreateSymlinkOperation(id, linkPath)
	item := targets.NewSymlink(linkPath, target)
	op.SetItem(item)
	op.SetDescriptionDetail("target", target)
	return NewOperationsPackageAdapter(op)
}

// CreateFileWithID creates a file creation operation with an explicit ID.
func (s *SynthFS) CreateFileWithID(id string, path string, content []byte, mode fs.FileMode) Operation {
	op := operations.NewCreateFileOperation(core.OperationID(id), path)
	item := targets.NewFile(path).WithContent(content).WithMode(mode)
	op.SetItem(item)
	return NewOperationsPackageAdapter(op)
}

// CreateDirWithID creates a directory creation operation with an explicit ID.
func (s *SynthFS) CreateDirWithID(id string, path string, mode fs.FileMode) Operation {
	op := operations.NewCreateDirectoryOperation(core.OperationID(id), path)
	item := targets.NewDirectory(path).WithMode(mode)
	op.SetItem(item)
	return NewOperationsPackageAdapter(op)
}

// DeleteWithID creates a delete operation with an explicit ID.
func (s *SynthFS) DeleteWithID(id string, path string) Operation {
	return NewOperationsPackageAdapter(operations.NewDeleteOperation(core.OperationID(id), path))
}

// CopyWithID creates a copy operation with an explicit ID.
func (s *SynthFS) CopyWithID(id string, src, dst string) Operation {
	op := operations.NewCopyOperation(core.OperationID(id), src)
	op.SetPaths(src, dst)
	return NewOperationsPackageAdapter(op)
}

// MoveWithID creates a move operation with an explicit ID.
func (s *SynthFS) MoveWithID(id string, src, dst string) Operation {
	op := operations.NewMoveOperation(core.OperationID(id), src)
	op.SetPaths(src, dst)
	return NewOperationsPackageAdapter(op)
}

// CreateSymlinkWithID creates a symlink operation with an explicit ID.
func (s *SynthFS) CreateSymlinkWithID(id string, target, linkPath string) Operation {
	op := operations.NewCreateSymlinkOperation(core.OperationID(id), linkPath)
	item := targets.NewSymlink(linkPath, target)
	op.SetItem(item)
	op.SetDescriptionDetail("target", target)
	return NewOperationsPackageAdapter(op)
}

// CustomOperation creates a custom operation with an auto-generated ID.
// This allows users to define their own operations that integrate with SynthFS's pipeline system.
//
// Example:
//
//	op := sfs.CustomOperation("run-tests", func(ctx context.Context, fs filesystem.FileSystem) error {
//	    // Custom logic here
//	    return exec.Command("go", "test", "./...").Run()
//	})
func (s *SynthFS) CustomOperation(name string, executeFunc CustomOperationFunc) Operation {
	id := s.idGen("custom", name)
	op := NewCustomOperation(string(id), executeFunc)
	return NewCustomOperationAdapter(op)
}

// CustomOperationWithID creates a custom operation with an explicit ID.
// This allows users to define their own operations with a specific ID for dependency management.
func (s *SynthFS) CustomOperationWithID(id string, executeFunc CustomOperationFunc) Operation {
	op := NewCustomOperation(id, executeFunc)
	return NewCustomOperationAdapter(op)
}

// CustomOperationWithOutput creates a custom operation that can store output.
// The storeOutput function passed to executeFunc can be used to store values that
// will be available in the operation's description details after execution.
//
// Example:
//
//	op := sfs.CustomOperationWithOutput("process-data",
//	    func(ctx context.Context, fs filesystem.FileSystem, storeOutput func(string, interface{})) error {
//	        result := processData()
//	        storeOutput("result", result)
//	        storeOutput("recordCount", 42)
//	        return nil
//	    })
func (s *SynthFS) CustomOperationWithOutput(name string, executeFunc CustomOperationWithOutputFunc) Operation {
	id := s.idGen("custom", name)
	op := NewCustomOperationWithOutput(string(id), executeFunc)
	return NewCustomOperationAdapter(op)
}

// CustomOperationWithOutputAndID creates a custom operation with explicit ID that can store output.
func (s *SynthFS) CustomOperationWithOutputAndID(id string, executeFunc CustomOperationWithOutputFunc) Operation {
	op := NewCustomOperationWithOutput(id, executeFunc)
	return NewCustomOperationAdapter(op)
}

// ReadFile creates a file read operation with auto-generated ID.
// The file content is captured and stored as "content" output.
//
// Example:
//
//	op := sfs.ReadFile("/path/to/file")
//	result, _ := synthfs.Run(ctx, fs, op)
//	content := GetOperationOutput(op, "content")
func (s *SynthFS) ReadFile(path string) Operation {
	id := s.idGen("read_file", path)
	return s.ReadFileWithID(string(id), path)
}

// ReadFileWithID creates a file read operation with explicit ID.
func (s *SynthFS) ReadFileWithID(id string, path string) Operation {
	op := NewCustomOperationWithOutput(id, func(ctx context.Context, fs filesystem.FileSystem, storeOutput func(string, interface{})) error {
		// Cast to FullFileSystem to access Stat
		fullFS, ok := fs.(filesystem.FullFileSystem)
		if !ok {
			return fmt.Errorf("filesystem does not support Stat operation")
		}

		// Check if file exists and is readable
		info, err := fullFS.Stat(path)
		if err != nil {
			return fmt.Errorf("failed to stat file %s: %w", path, err)
		}

		if info.IsDir() {
			return fmt.Errorf("cannot read directory as file: %s", path)
		}

		// Open and read file content
		file, err := fs.Open(path)
		if err != nil {
			return fmt.Errorf("failed to open file %s: %w", path, err)
		}
		defer func() {
			if closeErr := file.Close(); closeErr != nil {
				// Log the close error, but don't override the main operation error
				_ = closeErr
			}
		}()

		content, err := io.ReadAll(file)
		if err != nil {
			return fmt.Errorf("failed to read file %s: %w", path, err)
		}

		// Store file content and metadata
		storeOutput("content", string(content))
		storeOutput("size", info.Size())
		storeOutput("modTime", info.ModTime())

		return nil
	})

	op = op.WithDescription(fmt.Sprintf("Read file: %s", path))
	return NewCustomOperationAdapter(op)
}

// ChecksumAlgorithm represents supported checksum algorithms
type ChecksumAlgorithm string

const (
	MD5    ChecksumAlgorithm = "md5"
	SHA1   ChecksumAlgorithm = "sha1"
	SHA256 ChecksumAlgorithm = "sha256"
	SHA512 ChecksumAlgorithm = "sha512"
)

// Checksum creates a checksum operation with auto-generated ID.
// The checksum value is stored as output using the algorithm name (e.g., "md5", "sha256").
// Additional metadata like size and modTime are also stored.
//
// Example:
//
//	op := sfs.Checksum("/path/to/file", SHA256)
//	result, _ := synthfs.Run(ctx, fs, op)
//	hash := GetOperationOutput(op, "sha256")
func (s *SynthFS) Checksum(path string, algorithm ChecksumAlgorithm) Operation {
	id := s.idGen("checksum", path)
	return s.ChecksumWithID(string(id), path, algorithm)
}

// ChecksumWithID creates a checksum operation with explicit ID.
func (s *SynthFS) ChecksumWithID(id string, path string, algorithm ChecksumAlgorithm) Operation {
	op := NewCustomOperationWithOutput(id, func(ctx context.Context, fs filesystem.FileSystem, storeOutput func(string, interface{})) error {
		// Cast to FullFileSystem to access Stat
		fullFS, ok := fs.(filesystem.FullFileSystem)
		if !ok {
			return fmt.Errorf("filesystem does not support Stat operation")
		}

		// Check if file exists and is readable
		info, err := fullFS.Stat(path)
		if err != nil {
			return fmt.Errorf("failed to stat file %s: %w", path, err)
		}

		if info.IsDir() {
			return fmt.Errorf("cannot checksum directory: %s", path)
		}

		// Open file for reading
		file, err := fs.Open(path)
		if err != nil {
			return fmt.Errorf("failed to open file %s: %w", path, err)
		}
		defer func() {
			if closeErr := file.Close(); closeErr != nil {
				// Log the close error, but don't override the main operation error
				_ = closeErr
			}
		}()

		// Create appropriate hash algorithm
		var hasher hash.Hash
		switch algorithm {
		case MD5:
			hasher = md5.New()
		case SHA1:
			hasher = sha1.New()
		case SHA256:
			hasher = sha256.New()
		case SHA512:
			hasher = sha512.New()
		default:
			return fmt.Errorf("unsupported checksum algorithm: %s", algorithm)
		}

		// Calculate checksum
		if _, err := io.Copy(hasher, file); err != nil {
			return fmt.Errorf("failed to calculate %s checksum for %s: %w", algorithm, path, err)
		}

		checksumValue := fmt.Sprintf("%x", hasher.Sum(nil))

		// Store checksum and metadata
		storeOutput(string(algorithm), checksumValue)
		storeOutput("size", info.Size())
		storeOutput("modTime", info.ModTime())
		storeOutput("algorithm", string(algorithm))

		return nil
	})

	op = op.WithDescription(fmt.Sprintf("Calculate %s checksum: %s", algorithm, path))
	return NewCustomOperationAdapter(op)
}
