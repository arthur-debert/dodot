package core

import (
	"fmt"
	"io/fs"
	"time"
)

// OperationID uniquely identifies an operation within a batch
type OperationID string

// OperationDesc describes an operation's type and target
type OperationDesc struct {
	Type    string
	Path    string
	Details map[string]interface{}
}

// OperationStatus indicates the outcome of an individual operation's execution
type OperationStatus string

const (
	// StatusSuccess indicates the operation completed successfully
	StatusSuccess OperationStatus = "SUCCESS"
	// StatusFailure indicates the operation failed during execution
	StatusFailure OperationStatus = "FAILURE"
	// StatusValidation indicates the operation failed during validation
	StatusValidation OperationStatus = "VALIDATION_FAILURE"
)

// PathStateType represents the type of a filesystem object in the projected state
type PathStateType int

const (
	// PathStateUnknown represents an unknown or non-existent path
	PathStateUnknown PathStateType = iota
	// PathStateFile represents a file
	PathStateFile
	// PathStateDir represents a directory
	PathStateDir
	// PathStateSymlink represents a symbolic link
	PathStateSymlink
)

// String returns the string representation of the PathStateType
func (t PathStateType) String() string {
	switch t {
	case PathStateFile:
		return "file"
	case PathStateDir:
		return "directory"
	case PathStateSymlink:
		return "symlink"
	default:
		return "unknown"
	}
}

// Default values
const (
	// DefaultMaxBackupMB is the default maximum backup size in MB
	DefaultMaxBackupMB = 10
)

// ArchiveFormat defines the type of archive
type ArchiveFormat int

const (
	// ArchiveFormatTarGz represents a tar.gz archive format
	ArchiveFormatTarGz ArchiveFormat = iota
	// ArchiveFormatZip represents a zip archive format
	ArchiveFormatZip
)

// BackupData contains information about backed up data for an operation
type BackupData struct {
	OperationID   OperationID
	BackupType    string
	OriginalPath  string
	BackupContent []byte
	BackupMode    fs.FileMode
	BackupTime    time.Time
	SizeMB        float64
	Metadata      map[string]interface{}
}

// BackupBudget tracks the backup storage budget for operations
type BackupBudget struct {
	TotalMB     float64
	RemainingMB float64
	UsedMB      float64
}

// ConsumeBackup reduces the remaining budget by the specified amount
func (b *BackupBudget) ConsumeBackup(sizeMB float64) error {
	if sizeMB > b.RemainingMB {
		return fmt.Errorf("backup size %.2fMB exceeds remaining budget %.2fMB", sizeMB, b.RemainingMB)
	}
	b.RemainingMB -= sizeMB
	b.UsedMB += sizeMB
	return nil
}

// RestoreBackup increases the remaining budget by the specified amount
func (b *BackupBudget) RestoreBackup(sizeMB float64) {
	b.RemainingMB += sizeMB
	b.UsedMB -= sizeMB
	if b.UsedMB < 0 {
		b.UsedMB = 0
	}
	if b.RemainingMB > b.TotalMB {
		b.RemainingMB = b.TotalMB
	}
}
