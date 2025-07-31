// Package synthfs provides a high-level API for filesystem operations.
package synthfs

import (
	"github.com/arthur-debert/synthfs/pkg/synthfs/targets"
)

// This file provides backward compatibility for the public API.
// It forwards type and function definitions to the new `targets` package.

// --- Item Types ---

// FileItem forwards to targets.FileItem.
type FileItem = targets.FileItem

// DirectoryItem forwards to targets.DirectoryItem.
type DirectoryItem = targets.DirectoryItem

// SymlinkItem forwards to targets.SymlinkItem.
type SymlinkItem = targets.SymlinkItem

// ArchiveItem forwards to targets.ArchiveItem.
type ArchiveItem = targets.ArchiveItem

// UnarchiveItem forwards to targets.UnarchiveItem.
type UnarchiveItem = targets.UnarchiveItem

// --- Functions ---

// NewFile forwards to targets.NewFile.
func NewFile(path string) *targets.FileItem {
	return targets.NewFile(path)
}

// NewDirectory forwards to targets.NewDirectory.
func NewDirectory(path string) *targets.DirectoryItem {
	return targets.NewDirectory(path)
}

// NewSymlink forwards to targets.NewSymlink.
func NewSymlink(path, target string) *targets.SymlinkItem {
	return targets.NewSymlink(path, target)
}

// NewArchive forwards to targets.NewArchive.
func NewArchive(path string, format ArchiveFormat, sources []string) *targets.ArchiveItem {
	return targets.NewArchive(path, targets.ArchiveFormat(format), sources)
}

// NewUnarchive forwards to targets.NewUnarchive.
func NewUnarchive(archivePath, extractPath string) *targets.UnarchiveItem {
	return targets.NewUnarchive(archivePath, extractPath)
}
