package validation

import (
	"crypto/md5"
	"fmt"
	"io"
	"time"

	"github.com/arthur-debert/synthfs/pkg/synthfs/filesystem"
)

// ChecksumRecord stores file checksum information
type ChecksumRecord struct {
	Path         string
	MD5          string
	Size         int64
	ModTime      time.Time
	ChecksumTime time.Time
}

// ComputeFileChecksum calculates the MD5 checksum and gathers file metadata.
func ComputeFileChecksum(fsys filesystem.FullFileSystem, filePath string) (*ChecksumRecord, error) {
	info, err := fsys.Stat(filePath)
	if err != nil {
		return nil, fmt.Errorf("failed to stat file %s: %w", filePath, err)
	}

	if info.IsDir() {
		return nil, nil
	}

	file, err := fsys.Open(filePath)
	if err != nil {
		return nil, fmt.Errorf("failed to open file %s for checksumming: %w", filePath, err)
	}
	defer func() {
		_ = file.Close()
	}()

	hash := md5.New()
	if _, err := io.Copy(hash, file); err != nil {
		return nil, fmt.Errorf("failed to calculate checksum for %s: %w", filePath, err)
	}

	checksum := &ChecksumRecord{
		Path:         filePath,
		MD5:          fmt.Sprintf("%x", hash.Sum(nil)),
		Size:         info.Size(),
		ModTime:      info.ModTime(),
		ChecksumTime: time.Now(),
	}

	return checksum, nil
}
