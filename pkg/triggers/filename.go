package triggers

import (
	"fmt"
	"io/fs"
	"path/filepath"

	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/registry"
	"github.com/arthur-debert/dodot/pkg/types"
)

const (
	FileNameTriggerName     = "filename"
	FileNameTriggerPriority = 100
)

// FileNameTrigger matches files based on their name or glob pattern
type FileNameTrigger struct {
	pattern  string
	isGlob   bool
	priority int
}

// NewFileNameTrigger creates a new FileNameTrigger with the given pattern
func NewFileNameTrigger(pattern string) *FileNameTrigger {
	isGlob := containsGlobChars(pattern)
	return &FileNameTrigger{
		pattern:  pattern,
		isGlob:   isGlob,
		priority: FileNameTriggerPriority,
	}
}

// Name returns the unique name of this trigger
func (t *FileNameTrigger) Name() string {
	return FileNameTriggerName
}

// Description returns a human-readable description of what this trigger matches
func (t *FileNameTrigger) Description() string {
	if t.isGlob {
		return "Matches files by glob pattern: " + t.pattern
	}
	return "Matches files by exact name: " + t.pattern
}

// Match checks if the given file matches this trigger's pattern
func (t *FileNameTrigger) Match(path string, info fs.FileInfo) (bool, map[string]interface{}) {
	logger := logging.GetLogger("triggers.filename")
	
	// Skip directories
	if info.IsDir() {
		return false, nil
	}
	
	filename := filepath.Base(path)
	var matched bool
	var err error
	
	if t.isGlob {
		matched, err = filepath.Match(t.pattern, filename)
		if err != nil {
			logger.Error().
				Err(err).
				Str("pattern", t.pattern).
				Str("filename", filename).
				Msg("error matching glob pattern")
			return false, nil
		}
	} else {
		matched = filename == t.pattern
	}
	
	if matched {
		logger.Debug().
			Str("trigger", t.Name()).
			Str("pattern", t.pattern).
			Str("file", path).
			Bool("is_glob", t.isGlob).
			Msg("file matched trigger")
		
		metadata := map[string]interface{}{
			"pattern":  t.pattern,
			"filename": filename,
			"is_glob":  t.isGlob,
		}
		return true, metadata
	}
	
	return false, nil
}

// Priority returns the priority of this trigger
func (t *FileNameTrigger) Priority() int {
	return t.priority
}

// containsGlobChars checks if a pattern contains glob special characters
func containsGlobChars(pattern string) bool {
	for _, char := range pattern {
		switch char {
		case '*', '?', '[', ']', '{', '}':
			return true
		}
	}
	return false
}

func init() {
	// Register a factory function that creates triggers with custom patterns
	err := registry.RegisterTriggerFactory(FileNameTriggerName, func(config map[string]interface{}) (types.Trigger, error) {
		pattern, ok := config["pattern"].(string)
		if !ok {
			pattern = "*" // Default to match all files
		}
		return NewFileNameTrigger(pattern), nil
	})
	if err != nil {
		panic(fmt.Sprintf("failed to register FileNameTrigger factory: %v", err))
	}
}