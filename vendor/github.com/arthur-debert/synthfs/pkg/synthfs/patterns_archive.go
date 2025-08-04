package synthfs

import (
	"context"
	"path/filepath"
	"strings"

	"github.com/arthur-debert/synthfs/pkg/synthfs/operations"
	"github.com/arthur-debert/synthfs/pkg/synthfs/targets"
)

// detectArchiveFormat detects the archive format from the file extension
func detectArchiveFormat(path string) targets.ArchiveFormat {
	ext := strings.ToLower(filepath.Ext(path))
	switch ext {
	case ".zip":
		return targets.ArchiveFormatZip
	case ".tar":
		return targets.ArchiveFormatTarGz // Default tar to tar.gz for now
	case ".gz", ".tgz":
		if strings.HasSuffix(strings.ToLower(path), ".tar.gz") || ext == ".tgz" {
			return targets.ArchiveFormatTarGz
		}
	}
	return targets.ArchiveFormatZip // Default to zip
}

// CreateArchive creates an archive operation
func (s *SynthFS) CreateArchive(archivePath string, sources ...string) Operation {
	id := s.idGen("create_archive", archivePath)

	// Detect format from file extension
	format := detectArchiveFormat(archivePath)

	// Create the archive target
	archive := targets.NewArchive(archivePath, format, sources)

	// Create the operation
	op := operations.NewCreateArchiveOperation(id, archivePath)
	op.SetItem(archive)

	// Also set sources and format in description details as fallback
	op.SetDescriptionDetail("sources", sources)
	op.SetDescriptionDetail("format", format.String())

	return &OperationsPackageAdapter{opsOperation: op}
}

// CreateZipArchive creates a ZIP archive operation
func (s *SynthFS) CreateZipArchive(archivePath string, sources ...string) Operation {
	id := s.idGen("create_archive", archivePath)

	// Create the archive target with ZIP format
	archive := targets.NewArchive(archivePath, targets.ArchiveFormatZip, sources)

	// Create the operation
	op := operations.NewCreateArchiveOperation(id, archivePath)
	op.SetItem(archive)

	// Also set sources and format in description details as fallback
	op.SetDescriptionDetail("sources", sources)
	op.SetDescriptionDetail("format", "zip")

	return &OperationsPackageAdapter{opsOperation: op}
}

// CreateTarArchive creates a TAR archive operation
func (s *SynthFS) CreateTarArchive(archivePath string, sources ...string) Operation {
	// Note: We don't have a plain TAR format in targets, so we use TarGz
	return s.CreateTarGzArchive(archivePath, sources...)
}

// CreateTarGzArchive creates a gzipped TAR archive operation
func (s *SynthFS) CreateTarGzArchive(archivePath string, sources ...string) Operation {
	id := s.idGen("create_archive", archivePath)

	// Create the archive target with TAR.GZ format
	archive := targets.NewArchive(archivePath, targets.ArchiveFormatTarGz, sources)

	// Create the operation
	op := operations.NewCreateArchiveOperation(id, archivePath)
	op.SetItem(archive)

	// Also set sources and format in description details as fallback
	op.SetDescriptionDetail("sources", sources)
	op.SetDescriptionDetail("format", "tar.gz")

	return &OperationsPackageAdapter{opsOperation: op}
}

// ExtractArchive creates an unarchive operation
func (s *SynthFS) ExtractArchive(archivePath, extractPath string) Operation {
	id := s.idGen("unarchive", archivePath)

	// Create the unarchive item
	unarchive := targets.NewUnarchive(archivePath, extractPath)

	// Create the operation
	op := operations.NewUnarchiveOperation(id, archivePath)
	op.SetItem(unarchive)

	return &OperationsPackageAdapter{opsOperation: op}
}

// ExtractArchiveWithPatterns creates an unarchive operation with file patterns
func (s *SynthFS) ExtractArchiveWithPatterns(archivePath, extractPath string, patterns ...string) Operation {
	id := s.idGen("unarchive", archivePath)

	// Create the unarchive item with patterns
	unarchive := targets.NewUnarchive(archivePath, extractPath).WithPatterns(patterns...)

	// Create the operation
	op := operations.NewUnarchiveOperation(id, archivePath)
	op.SetItem(unarchive)

	return &OperationsPackageAdapter{opsOperation: op}
}

// Archive provides direct archive creation with execution
func Archive(ctx context.Context, fs FileSystem, archivePath string, sources ...string) error {
	op := New().CreateArchive(archivePath, sources...)
	return op.Execute(ctx, fs)
}

// Extract provides direct archive extraction with execution
func Extract(ctx context.Context, fs FileSystem, archivePath, extractPath string) error {
	op := New().ExtractArchive(archivePath, extractPath)
	return op.Execute(ctx, fs)
}

// ArchiveBuilder provides a fluent interface for creating archives
type ArchiveBuilder struct {
	archivePath string
	sources     []string
	format      targets.ArchiveFormat
}

// NewArchiveBuilder creates a new archive builder
func NewArchiveBuilder(archivePath string) *ArchiveBuilder {
	return &ArchiveBuilder{
		archivePath: archivePath,
		sources:     []string{},
		format:      detectArchiveFormat(archivePath),
	}
}

// AddSource adds a source to the archive
func (ab *ArchiveBuilder) AddSource(source string) *ArchiveBuilder {
	ab.sources = append(ab.sources, source)
	return ab
}

// AddSources adds multiple sources to the archive
func (ab *ArchiveBuilder) AddSources(sources ...string) *ArchiveBuilder {
	ab.sources = append(ab.sources, sources...)
	return ab
}

// WithFormat sets the archive format explicitly
func (ab *ArchiveBuilder) WithFormat(format targets.ArchiveFormat) *ArchiveBuilder {
	ab.format = format
	return ab
}

// AsZip sets the format to ZIP
func (ab *ArchiveBuilder) AsZip() *ArchiveBuilder {
	return ab.WithFormat(targets.ArchiveFormatZip)
}

// AsTar sets the format to TAR
func (ab *ArchiveBuilder) AsTar() *ArchiveBuilder {
	// Note: We don't have a plain TAR format, so we use TarGz
	return ab.WithFormat(targets.ArchiveFormatTarGz)
}

// AsTarGz sets the format to gzipped TAR
func (ab *ArchiveBuilder) AsTarGz() *ArchiveBuilder {
	return ab.WithFormat(targets.ArchiveFormatTarGz)
}

// Build creates the archive operation
func (ab *ArchiveBuilder) Build() Operation {
	sfs := New()
	if len(ab.sources) == 0 {
		// Return an operation that will fail validation
		return sfs.CreateArchive(ab.archivePath)
	}

	var op Operation
	switch ab.format {
	case targets.ArchiveFormatZip:
		op = sfs.CreateZipArchive(ab.archivePath, ab.sources...)
	case targets.ArchiveFormatTarGz:
		op = sfs.CreateTarGzArchive(ab.archivePath, ab.sources...)
	default:
		op = sfs.CreateArchive(ab.archivePath, ab.sources...)
	}

	return op
}

// Execute creates and executes the archive operation
func (ab *ArchiveBuilder) Execute(ctx context.Context, fs FileSystem) error {
	op := ab.Build()
	return op.Execute(ctx, fs)
}

// ExtractBuilder provides a fluent interface for extracting archives
type ExtractBuilder struct {
	archivePath string
	extractPath string
	patterns    []string
}

// NewExtractBuilder creates a new extract builder
func NewExtractBuilder(archivePath string) *ExtractBuilder {
	return &ExtractBuilder{
		archivePath: archivePath,
		extractPath: ".",
		patterns:    []string{},
	}
}

// To sets the extraction destination
func (eb *ExtractBuilder) To(path string) *ExtractBuilder {
	eb.extractPath = path
	return eb
}

// WithPattern adds a file pattern to extract
func (eb *ExtractBuilder) WithPattern(pattern string) *ExtractBuilder {
	eb.patterns = append(eb.patterns, pattern)
	return eb
}

// WithPatterns adds multiple file patterns to extract
func (eb *ExtractBuilder) WithPatterns(patterns ...string) *ExtractBuilder {
	eb.patterns = append(eb.patterns, patterns...)
	return eb
}

// OnlyFiles extracts only files matching the given patterns
func (eb *ExtractBuilder) OnlyFiles(patterns ...string) *ExtractBuilder {
	return eb.WithPatterns(patterns...)
}

// Build creates the extract operation
func (eb *ExtractBuilder) Build() Operation {
	sfs := New()
	if len(eb.patterns) > 0 {
		return sfs.ExtractArchiveWithPatterns(eb.archivePath, eb.extractPath, eb.patterns...)
	}
	return sfs.ExtractArchive(eb.archivePath, eb.extractPath)
}

// Execute creates and executes the extract operation
func (eb *ExtractBuilder) Execute(ctx context.Context, fs FileSystem) error {
	op := eb.Build()
	return op.Execute(ctx, fs)
}
