package targets

// ArchiveFormat defines the type of an archive, e.g., tar.gz or zip.
type ArchiveFormat int

const (
	// ArchiveFormatTarGz represents a .tar.gz archive.
	ArchiveFormatTarGz ArchiveFormat = iota
	// ArchiveFormatZip represents a .zip archive.
	ArchiveFormatZip
)

// String returns the string representation of the archive format.
func (af ArchiveFormat) String() string {
	switch af {
	case ArchiveFormatTarGz:
		return "tar.gz"
	case ArchiveFormatZip:
		return "zip"
	default:
		return "unknown"
	}
}

// ArchiveItem represents an archive to be created.
type ArchiveItem struct {
	path    string
	format  ArchiveFormat
	sources []string
}

// NewArchive creates a new ArchiveItem.
func NewArchive(path string, format ArchiveFormat, sources []string) *ArchiveItem {
	return &ArchiveItem{
		path:    path,
		format:  format,
		sources: sources,
	}
}

// Path returns the archive's path.
func (ai *ArchiveItem) Path() string {
	return ai.path
}

// Type returns the string "archive".
func (ai *ArchiveItem) Type() string {
	return "archive"
}

// Format returns the archive's format.
func (ai *ArchiveItem) Format() ArchiveFormat {
	return ai.format
}

// Sources returns the list of source paths for the archive.
func (ai *ArchiveItem) Sources() []string {
	return ai.sources
}

// WithSources sets the sources for the archive.
func (ai *ArchiveItem) WithSources(sources []string) *ArchiveItem {
	ai.sources = sources
	return ai
}

// UnarchiveItem represents an unarchive operation.
type UnarchiveItem struct {
	archivePath string
	extractPath string
	patterns    []string
	overwrite   bool
}

// NewUnarchive creates a new UnarchiveItem.
func NewUnarchive(archivePath, extractPath string) *UnarchiveItem {
	return &UnarchiveItem{
		archivePath: archivePath,
		extractPath: extractPath,
		patterns:    []string{},
		overwrite:   false,
	}
}

// Path returns the source archive's path.
func (ui *UnarchiveItem) Path() string {
	return ui.archivePath
}

// Type returns the string "unarchive".
func (ui *UnarchiveItem) Type() string {
	return "unarchive"
}

// ArchivePath returns the path of the archive to extract.
func (ui *UnarchiveItem) ArchivePath() string {
	return ui.archivePath
}

// ExtractPath returns the destination path for extraction.
func (ui *UnarchiveItem) ExtractPath() string {
	return ui.extractPath
}

// Patterns returns the glob patterns for filtering which files to extract.
func (ui *UnarchiveItem) Patterns() []string {
	return ui.patterns
}

// Overwrite returns true if existing files should be overwritten.
func (ui *UnarchiveItem) Overwrite() bool {
	return ui.overwrite
}

// WithPatterns sets the glob patterns for filtering.
func (ui *UnarchiveItem) WithPatterns(patterns ...string) *UnarchiveItem {
	ui.patterns = patterns
	return ui
}

// WithOverwrite sets the overwrite behavior.
func (ui *UnarchiveItem) WithOverwrite(overwrite bool) *UnarchiveItem {
	ui.overwrite = overwrite
	return ui
}
