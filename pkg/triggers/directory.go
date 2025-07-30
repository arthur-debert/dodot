package triggers

import (
	"fmt"
	"io/fs"
	"path/filepath"

	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/registry"
	"github.com/arthur-debert/dodot/pkg/types"
)

// DirectoryTriggerName is the name used to reference this trigger
const DirectoryTriggerName = "directory"

// DirectoryTrigger matches entire directories by name
type DirectoryTrigger struct {
	pattern string
}

// NewDirectoryTrigger creates a new DirectoryTrigger with the given options
func NewDirectoryTrigger(options map[string]interface{}) (*DirectoryTrigger, error) {
	logger := logging.GetLogger("triggers.directory")

	// Extract pattern from options
	pattern, ok := options["pattern"].(string)
	if !ok || pattern == "" {
		return nil, fmt.Errorf("directory trigger requires a 'pattern' option")
	}

	logger.Trace().
		Str("pattern", pattern).
		Msg("created directory trigger")

	return &DirectoryTrigger{
		pattern: pattern,
	}, nil
}

// Name returns the name of this trigger
func (t *DirectoryTrigger) Name() string {
	return DirectoryTriggerName
}

// Description returns a human-readable description of this trigger
func (t *DirectoryTrigger) Description() string {
	return fmt.Sprintf("Matches directories named '%s'", t.pattern)
}

// Match checks if the given path matches this trigger's pattern
func (t *DirectoryTrigger) Match(path string, info fs.FileInfo) (bool, map[string]interface{}) {
	logger := logging.GetLogger("triggers.directory")

	// Only match directories
	if !info.IsDir() {
		return false, nil
	}

	// Get the directory name
	dirName := filepath.Base(path)

	// Check if it matches our pattern
	matched, err := filepath.Match(t.pattern, dirName)
	if err != nil {
		logger.Error().
			Err(err).
			Str("pattern", t.pattern).
			Str("dirName", dirName).
			Msg("error matching directory pattern")
		return false, nil
	}

	if matched {
		logger.Trace().
			Str("path", path).
			Str("pattern", t.pattern).
			Msg("directory matched")

		metadata := map[string]interface{}{
			"directory": dirName,
			"pattern":   t.pattern,
		}
		return true, metadata
	}

	return false, nil
}

// Priority returns the priority of this trigger
func (t *DirectoryTrigger) Priority() int {
	return 100 // High priority for directory matching
}

// ValidateOptions checks if the provided options are valid for this trigger
func (t *DirectoryTrigger) ValidateOptions(options map[string]interface{}) error {
	if options == nil {
		return fmt.Errorf("options cannot be nil")
	}

	pattern, ok := options["pattern"]
	if !ok {
		return fmt.Errorf("missing required option: pattern")
	}

	if _, ok := pattern.(string); !ok {
		return fmt.Errorf("pattern must be a string")
	}

	// Check for unknown options
	for key := range options {
		if key != "pattern" {
			return fmt.Errorf("unknown option: %s", key)
		}
	}

	return nil
}

func init() {
	// Register the directory trigger factory
	err := registry.RegisterTriggerFactory(DirectoryTriggerName, func(options map[string]interface{}) (types.Trigger, error) {
		return NewDirectoryTrigger(options)
	})
	if err != nil {
		panic(fmt.Sprintf("failed to register directory trigger: %v", err))
	}
}
