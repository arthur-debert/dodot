package operations

import (
	"archive/tar"
	"archive/zip"
	"bytes"
	"compress/gzip"
	"context"
	"fmt"
	"io"
	"os"
	"path/filepath"
	"strings"

	"github.com/arthur-debert/synthfs/pkg/synthfs/core"
)

// CreateArchiveOperation represents an archive creation operation.
type CreateArchiveOperation struct {
	*BaseOperation
}

// NewCreateArchiveOperation creates a new archive creation operation.
func NewCreateArchiveOperation(id core.OperationID, archivePath string) *CreateArchiveOperation {
	return &CreateArchiveOperation{
		BaseOperation: NewBaseOperation(id, "create_archive", archivePath),
	}
}

// Prerequisites returns the prerequisites for creating an archive
func (op *CreateArchiveOperation) Prerequisites() []core.Prerequisite {
	var prereqs []core.Prerequisite
	
	// Need parent directory for archive to exist
	if filepath.Dir(op.description.Path) != "." && filepath.Dir(op.description.Path) != "/" {
		prereqs = append(prereqs, core.NewParentDirPrerequisite(op.description.Path))
	}
	
	// Need no conflict with existing files
	prereqs = append(prereqs, core.NewNoConflictPrerequisite(op.description.Path))
	
	// Check sources exist (from item or details)
	var sources []string
	if item := op.GetItem(); item != nil {
		if archiveItem, ok := item.(interface{ Sources() []string }); ok {
			sources = archiveItem.Sources()
		}
	}
	if len(sources) == 0 {
		if detailSources, ok := op.description.Details["sources"].([]string); ok {
			sources = detailSources
		}
	}
	
	for _, source := range sources {
		prereqs = append(prereqs, core.NewSourceExistsPrerequisite(source))
	}
	
	return prereqs
}

// Execute creates the archive.
func (op *CreateArchiveOperation) Execute(ctx context.Context, fsys interface{}) error {
	// Get sources - first try from item, then from details
	var sources []string
	var format interface{}

	if item := op.GetItem(); item != nil {
		if archiveItem, ok := item.(interface {
			Sources() []string
			Format() interface{}
		}); ok {
			sources = archiveItem.Sources()
			format = archiveItem.Format()
		}
	}

	// Fallback to details
	if len(sources) == 0 {
		if detailSources, ok := op.description.Details["sources"].([]string); ok {
			sources = detailSources
		}
	}
	if len(sources) == 0 {
		return fmt.Errorf("create_archive operation requires sources")
	}

	if format == nil {
		format = op.description.Details["format"]
	}
	if format == nil {
		return fmt.Errorf("create_archive operation requires format")
	}

	// For now, we'll need to use the OS filesystem for archive creation
	// This is a limitation we'll address in future iterations
	archivePath := op.description.Path

	// Determine archive type based on format or file extension
	formatStr := fmt.Sprintf("%v", format)
	switch strings.ToLower(formatStr) {
	case "zip":
		return op.createZipArchive(archivePath, sources, fsys)
	case "tar", "tar.gz", "tgz":
		return op.createTarArchive(archivePath, sources, fsys, strings.HasSuffix(strings.ToLower(archivePath), ".gz"))
	default:
		// Try to determine from file extension
		ext := strings.ToLower(filepath.Ext(archivePath))
		switch ext {
		case ".zip":
			return op.createZipArchive(archivePath, sources, fsys)
		case ".tar":
			return op.createTarArchive(archivePath, sources, fsys, false)
		case ".gz", ".tgz":
			return op.createTarArchive(archivePath, sources, fsys, true)
		default:
			return fmt.Errorf("unsupported archive format: %s", formatStr)
		}
	}
}

// createZipArchive creates a ZIP archive.
func (op *CreateArchiveOperation) createZipArchive(archivePath string, sources []string, fsys interface{}) error {
	// Create a buffer to hold the archive data
	var buf bytes.Buffer

	// Create zip writer
	zipWriter := zip.NewWriter(&buf)

	// Add sources to archive
	for _, source := range sources {
		// Try to stat the source
		var sourceInfo interface{}
		if stat, ok := getStatMethod(fsys); ok {
			info, err := stat(source)
			if err != nil {
				return fmt.Errorf("failed to stat source %s: %w", source, err)
			}
			sourceInfo = info
		}

		// Check if it's a directory
		isDir := false
		if fi, ok := sourceInfo.(interface{ IsDir() bool }); ok {
			isDir = fi.IsDir()
		}

		if isDir {
			// Skip directories for now - would need to walk them
			continue
		}

		// Read file content
		var content []byte
		if open, ok := getOpenMethod(fsys); ok {
			file, err := open(source)
			if err != nil {
				return fmt.Errorf("failed to open source %s: %w", source, err)
			}
			if reader, ok := file.(io.Reader); ok {
				content, err = io.ReadAll(reader)
				if err != nil {
					return fmt.Errorf("failed to read source %s: %w", source, err)
				}
			}
			if closer, ok := file.(io.Closer); ok {
				_ = closer.Close()
			}
		}

		// Add file to archive
		writer, err := zipWriter.Create(source)
		if err != nil {
			return fmt.Errorf("failed to create zip entry for %s: %w", source, err)
		}

		if _, err := writer.Write(content); err != nil {
			return fmt.Errorf("failed to write content for %s: %w", source, err)
		}
	}

	// Close the zip writer
	if err := zipWriter.Close(); err != nil {
		return fmt.Errorf("failed to close zip writer: %w", err)
	}

	// Write the archive to filesystem
	if writeFile, ok := getWriteFileMethod(fsys); ok {
		return writeFile(archivePath, buf.Bytes(), 0644)
	}

	return fmt.Errorf("filesystem does not support WriteFile")
}

// createTarArchive creates a TAR or TAR.GZ archive.
func (op *CreateArchiveOperation) createTarArchive(archivePath string, sources []string, fsys interface{}, compress bool) error {
	// Create a buffer to hold the archive data
	var buf bytes.Buffer

	var tarWriter *tar.Writer
	if compress {
		gzWriter := gzip.NewWriter(&buf)
		defer func() { _ = gzWriter.Close() }()
		tarWriter = tar.NewWriter(gzWriter)
	} else {
		tarWriter = tar.NewWriter(&buf)
	}
	defer func() { _ = tarWriter.Close() }()

	// Add sources to archive
	for _, source := range sources {
		// Try to stat the source
		var sourceInfo interface{}
		if stat, ok := getStatMethod(fsys); ok {
			info, err := stat(source)
			if err != nil {
				return fmt.Errorf("failed to stat source %s: %w", source, err)
			}
			sourceInfo = info
		}

		// Check if it's a directory
		isDir := false
		var mode os.FileMode = 0644
		
		if fi, ok := sourceInfo.(interface{ 
			IsDir() bool
			Size() int64
			Mode() os.FileMode
		}); ok {
			isDir = fi.IsDir()
			mode = fi.Mode()
		}

		if isDir {
			// Skip directories for now - would need to walk them
			continue
		}

		// Read file content
		var content []byte
		if open, ok := getOpenMethod(fsys); ok {
			file, err := open(source)
			if err != nil {
				return fmt.Errorf("failed to open source %s: %w", source, err)
			}
			if reader, ok := file.(io.Reader); ok {
				content, err = io.ReadAll(reader)
				if err != nil {
					return fmt.Errorf("failed to read source %s: %w", source, err)
				}
			}
			if closer, ok := file.(io.Closer); ok {
				_ = closer.Close()
			}
		}

		// Create tar header
		header := &tar.Header{
			Name: source,
			Mode: int64(mode.Perm()),
			Size: int64(len(content)),
		}
		
		if err := tarWriter.WriteHeader(header); err != nil {
			return fmt.Errorf("failed to write header for %s: %w", source, err)
		}

		if _, err := tarWriter.Write(content); err != nil {
			return fmt.Errorf("failed to write content for %s: %w", source, err)
		}
	}

	// Close the tar writer  
	if err := tarWriter.Close(); err != nil {
		return fmt.Errorf("failed to close tar writer: %w", err)
	}

	// Write the archive to filesystem
	if writeFile, ok := getWriteFileMethod(fsys); ok {
		return writeFile(archivePath, buf.Bytes(), 0644)
	}

	return fmt.Errorf("filesystem does not support WriteFile")
}

// ExecuteV2 performs the archive creation with execution context support.
func (op *CreateArchiveOperation) ExecuteV2(ctx interface{}, execCtx *core.ExecutionContext, fsys interface{}) error {
	// Convert context
	context, ok := ctx.(context.Context)
	if !ok {
		return fmt.Errorf("invalid context type")
	}

	// Call the operation's Execute method with proper event handling
	return executeWithEvents(op, context, execCtx, fsys, op.Execute)
}

// ValidateV2 checks if the archive can be created using ExecutionContext.
func (op *CreateArchiveOperation) ValidateV2(ctx interface{}, execCtx *core.ExecutionContext, fsys interface{}) error {
	return validateV2Helper(op, ctx, execCtx, fsys)
}

// Validate checks if the archive can be created.
func (op *CreateArchiveOperation) Validate(ctx context.Context, fsys interface{}) error {
	// First do base validation
	if err := op.BaseOperation.Validate(ctx, fsys); err != nil {
		return err
	}

	// Check sources from item first
	var sources []string
	if item := op.GetItem(); item != nil {
		if archiveItem, ok := item.(interface{ Sources() []string }); ok {
			sources = archiveItem.Sources()
		}
	}

	// If no sources from item, check description details
	if len(sources) == 0 {
		if detailSources, ok := op.description.Details["sources"].([]string); ok {
			sources = detailSources
		}
	}

	if len(sources) == 0 {
		return &core.ValidationError{
			OperationID:   op.ID(),
			OperationDesc: op.Describe(),
			Reason:        "must specify at least one source",
		}
	}

	// Check if sources exist
	if stat, ok := getStatMethod(fsys); ok {
		for _, source := range sources {
			if _, err := stat(source); err != nil {
				return &core.ValidationError{
					OperationID:   op.ID(),
					OperationDesc: op.Describe(),
					Reason:        fmt.Sprintf("source does not exist: %s", source),
					Cause:         err,
				}
			}
		}
	}

	return nil
}

// Rollback removes the created archive.
func (op *CreateArchiveOperation) Rollback(ctx context.Context, fsys interface{}) error {
	remove, ok := getRemoveMethod(fsys)
	if !ok {
		return fmt.Errorf("filesystem does not support Remove")
	}

	// Remove the archive
	_ = remove(op.description.Path) // Ignore error - might not exist
	return nil
}

// UnarchiveOperation represents an archive extraction operation.
type UnarchiveOperation struct {
	*BaseOperation
}

// NewUnarchiveOperation creates a new unarchive operation.
func NewUnarchiveOperation(id core.OperationID, archivePath string) *UnarchiveOperation {
	return &UnarchiveOperation{
		BaseOperation: NewBaseOperation(id, "unarchive", archivePath),
	}
}

// Prerequisites returns the prerequisites for unarchiving
func (op *UnarchiveOperation) Prerequisites() []core.Prerequisite {
	var prereqs []core.Prerequisite
	
	// Need archive to exist
	prereqs = append(prereqs, core.NewSourceExistsPrerequisite(op.description.Path))
	
	// Need extract path parent directory to exist
	if item := op.GetItem(); item != nil {
		if extractor, ok := item.(interface{ ExtractPath() string }); ok {
			extractPath := extractor.ExtractPath()
			if extractPath != "" {
				if filepath.Dir(extractPath) != "." && filepath.Dir(extractPath) != "/" {
					prereqs = append(prereqs, core.NewParentDirPrerequisite(extractPath))
				}
			}
		}
	}
	
	return prereqs
}

// Execute extracts the archive.
func (op *UnarchiveOperation) Execute(ctx context.Context, fsys interface{}) error {
	// Get extract path - first check item, then details
	var extractPath string
	if op.item != nil {
		if extractor, ok := op.item.(interface{ ExtractPath() string }); ok {
			extractPath = extractor.ExtractPath()
		}
	}
	
	// If not found in item, check details
	if extractPath == "" {
		if path, ok := op.description.Details["extract_path"].(string); ok {
			extractPath = path
		}
	}
	
	// Default to current directory if still empty
	if extractPath == "" {
		extractPath = "."
	}

	// Get patterns - first check item, then details
	var patterns []string
	if op.item != nil {
		if patterned, ok := op.item.(interface{ Patterns() []string }); ok {
			patterns = patterned.Patterns()
		}
	}
	
	// If not found in item, check details
	if len(patterns) == 0 {
		if p, ok := op.description.Details["patterns"].([]string); ok {
			patterns = p
		}
	}

	archivePath := op.description.Path

	// Determine archive type based on file extension
	ext := strings.ToLower(filepath.Ext(archivePath))
	switch ext {
	case ".zip":
		return op.extractZipArchive(archivePath, extractPath, patterns, fsys)
	case ".tar":
		return op.extractTarArchive(archivePath, extractPath, patterns, fsys, false)
	case ".gz", ".tgz":
		if strings.HasSuffix(archivePath, ".tar.gz") || ext == ".tgz" {
			return op.extractTarArchive(archivePath, extractPath, patterns, fsys, true)
		}
		return fmt.Errorf("unsupported archive format: %s", ext)
	default:
		return fmt.Errorf("unsupported archive format: %s", ext)
	}
}

// extractZipArchive extracts a ZIP archive.
func (op *UnarchiveOperation) extractZipArchive(archivePath, extractPath string, patterns []string, fsys interface{}) error {
	// Get filesystem methods
	open, hasOpen := getOpenMethod(fsys)
	if !hasOpen {
		return fmt.Errorf("filesystem does not support Open")
	}

	// Open archive file through filesystem interface
	file, err := open(archivePath)
	if err != nil {
		return fmt.Errorf("failed to open archive: %w", err)
	}
	defer func() {
		if closer, ok := file.(io.Closer); ok {
			_ = closer.Close()
		}
	}()

	// Read the entire file content
	var archiveData []byte
	if reader, ok := file.(io.Reader); ok {
		archiveData, err = io.ReadAll(reader)
		if err != nil {
			return fmt.Errorf("failed to read archive: %w", err)
		}
	} else {
		return fmt.Errorf("file does not implement io.Reader")
	}

	// Create a zip reader from the bytes
	reader, err := zip.NewReader(bytes.NewReader(archiveData), int64(len(archiveData)))
	if err != nil {
		return fmt.Errorf("failed to create zip reader: %w", err)
	}

	// Get filesystem methods
	mkdirAll, _ := getMkdirAllMethod(fsys)
	writeFile, _ := getWriteFileMethod(fsys)

	for _, file := range reader.File {
		// Check patterns if provided
		if len(patterns) > 0 && !matchesPatterns(file.Name, patterns) {
			continue
		}

		path := filepath.Join(extractPath, file.Name)

		if file.FileInfo().IsDir() {
			if mkdirAll != nil {
				_ = mkdirAll(path, file.Mode())
			}
			continue
		}

		// Create directory for file
		if mkdirAll != nil {
			_ = mkdirAll(filepath.Dir(path), 0755)
		}

		// Extract file
		if writeFile != nil {
			rc, err := file.Open()
			if err != nil {
				continue
			}
			content, _ := io.ReadAll(rc)
			_ = rc.Close()
			_ = writeFile(path, content, file.Mode())
		}
	}

	return nil
}

// extractTarArchive extracts a TAR or TAR.GZ archive.
func (op *UnarchiveOperation) extractTarArchive(archivePath, extractPath string, patterns []string, fsys interface{}, compressed bool) error {
	// Get filesystem methods
	open, hasOpen := getOpenMethod(fsys)
	if !hasOpen {
		return fmt.Errorf("filesystem does not support Open")
	}

	// Open archive file through filesystem interface
	file, err := open(archivePath)
	if err != nil {
		return fmt.Errorf("failed to open archive: %w", err)
	}
	defer func() {
		if closer, ok := file.(io.Closer); ok {
			_ = closer.Close()
		}
	}()

	// Convert file to io.Reader
	var reader io.Reader
	if r, ok := file.(io.Reader); ok {
		reader = r
	} else {
		return fmt.Errorf("file does not implement io.Reader")
	}

	var tarReader *tar.Reader
	if compressed {
		gzReader, err := gzip.NewReader(reader)
		if err != nil {
			return fmt.Errorf("failed to create gzip reader: %w", err)
		}
		defer func() { _ = gzReader.Close() }()
		tarReader = tar.NewReader(gzReader)
	} else {
		tarReader = tar.NewReader(reader)
	}

	// Get filesystem methods
	mkdirAll, _ := getMkdirAllMethod(fsys)
	writeFile, _ := getWriteFileMethod(fsys)

	for {
		header, err := tarReader.Next()
		if err == io.EOF {
			break
		}
		if err != nil {
			return fmt.Errorf("failed to read tar header: %w", err)
		}

		// Check patterns if provided
		if len(patterns) > 0 && !matchesPatterns(header.Name, patterns) {
			continue
		}

		path := filepath.Join(extractPath, header.Name)

		switch header.Typeflag {
		case tar.TypeDir:
			if mkdirAll != nil {
				_ = mkdirAll(path, os.FileMode(header.Mode))
			}
		case tar.TypeReg:
			// Create directory for file
			if mkdirAll != nil {
				_ = mkdirAll(filepath.Dir(path), 0755)
			}

			// Extract file
			if writeFile != nil {
				content, _ := io.ReadAll(tarReader)
				_ = writeFile(path, content, os.FileMode(header.Mode))
			}
		}
	}

	return nil
}

// matchesPatterns checks if a name matches any of the provided patterns.
func matchesPatterns(name string, patterns []string) bool {
	if len(patterns) == 0 {
		return true
	}

	for _, pattern := range patterns {
		matched, _ := filepath.Match(pattern, name)
		if matched {
			return true
		}
		// Also check if the name contains the pattern as a substring
		if strings.Contains(name, pattern) {
			return true
		}
	}

	return false
}

// ExecuteV2 performs the unarchive with execution context support.
func (op *UnarchiveOperation) ExecuteV2(ctx interface{}, execCtx *core.ExecutionContext, fsys interface{}) error {
	// Convert context
	context, ok := ctx.(context.Context)
	if !ok {
		return fmt.Errorf("invalid context type")
	}

	// Call the operation's Execute method with proper event handling
	return executeWithEvents(op, context, execCtx, fsys, op.Execute)
}

// ValidateV2 checks if the unarchive operation can be performed using ExecutionContext.
func (op *UnarchiveOperation) ValidateV2(ctx interface{}, execCtx *core.ExecutionContext, fsys interface{}) error {
	return validateV2Helper(op, ctx, execCtx, fsys)
}

// Validate checks if the unarchive operation can be performed.
func (op *UnarchiveOperation) Validate(ctx context.Context, fsys interface{}) error {
	// First do base validation
	if err := op.BaseOperation.Validate(ctx, fsys); err != nil {
		return err
	}

	// Check if we have an item
	if op.item == nil {
		return &core.ValidationError{
			OperationID:   op.ID(),
			OperationDesc: op.Describe(),
			Reason:        "unarchive operation requires an UnarchiveItem",
		}
	}

	// Check if item implements the expected interfaces
	archiver, hasArchivePath := op.item.(interface{ ArchivePath() string })
	extractor, hasExtractPath := op.item.(interface{ ExtractPath() string })
	
	if !hasArchivePath || !hasExtractPath {
		return &core.ValidationError{
			OperationID:   op.ID(),
			OperationDesc: op.Describe(),
			Reason:        "expected UnarchiveItem but got different type",
		}
	}

	// Get archive path and extract path from the item
	archivePath := archiver.ArchivePath()
	extractPath := extractor.ExtractPath()

	// Validate archive path is not empty
	if archivePath == "" {
		return &core.ValidationError{
			OperationID:   op.ID(),
			OperationDesc: op.Describe(),
			Reason:        "archive path cannot be empty",
		}
	}

	// Validate extract path is not empty
	if extractPath == "" {
		return &core.ValidationError{
			OperationID:   op.ID(),
			OperationDesc: op.Describe(),
			Reason:        "extract path cannot be empty",
		}
	}

	// Validate archive format
	ext := strings.ToLower(filepath.Ext(archivePath))
	switch ext {
	case ".zip", ".tar", ".gz", ".tar.gz", ".tgz":
		// Supported formats
	default:
		return &core.ValidationError{
			OperationID:   op.ID(),
			OperationDesc: op.Describe(),
			Reason:        fmt.Sprintf("unsupported archive format for file: %s", archivePath),
		}
	}

	// Check if archive exists
	if stat, ok := getStatMethod(fsys); ok {
		if _, err := stat(op.description.Path); err != nil {
			return &core.ValidationError{
				OperationID:   op.ID(),
				OperationDesc: op.Describe(),
				Reason:        "archive does not exist",
				Cause:         err,
			}
		}
	}

	return nil
}

// Rollback for unarchive would need to track and remove all extracted files.
func (op *UnarchiveOperation) Rollback(ctx context.Context, fsys interface{}) error {
	// TODO: Implement tracking of extracted files for proper rollback
	return fmt.Errorf("rollback of unarchive operations not yet implemented")
}
