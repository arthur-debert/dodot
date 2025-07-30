package triggers

import (
	"fmt"
	"io/fs"
	"path/filepath"

	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/registry"
	"github.com/arthur-debert/dodot/pkg/types"
)

// PathPatternTriggerName is the name used to reference this trigger
const PathPatternTriggerName = "path_pattern"

// PathPatternTrigger matches files based on their full path pattern
type PathPatternTrigger struct {
	pattern string
}

// NewPathPatternTrigger creates a new PathPatternTrigger with the given options
func NewPathPatternTrigger(options map[string]interface{}) (*PathPatternTrigger, error) {
	logger := logging.GetLogger("triggers.path_pattern")

	// Extract pattern from options
	pattern, ok := options["pattern"].(string)
	if !ok || pattern == "" {
		return nil, fmt.Errorf("path_pattern trigger requires a 'pattern' option")
	}

	logger.Debug().
		Str("pattern", pattern).
		Msg("created path pattern trigger")

	return &PathPatternTrigger{
		pattern: pattern,
	}, nil
}

// Name returns the name of this trigger
func (t *PathPatternTrigger) Name() string {
	return PathPatternTriggerName
}

// Description returns a human-readable description of this trigger
func (t *PathPatternTrigger) Description() string {
	return fmt.Sprintf("Matches paths matching pattern '%s'", t.pattern)
}

// Match checks if the given path matches this trigger's pattern
func (t *PathPatternTrigger) Match(path string, info fs.FileInfo) (bool, map[string]interface{}) {
	// Match against the relative path
	matched, err := filepath.Match(t.pattern, path)
	if err != nil {
		logger := logging.GetLogger("triggers.path_pattern")
		logger.Error().
			Err(err).
			Str("pattern", t.pattern).
			Str("path", path).
			Msg("error matching path pattern")
		return false, nil
	}

	if matched {
		logger := logging.GetLogger("triggers.path_pattern")
		logger.Debug().
			Str("path", path).
			Str("pattern", t.pattern).
			Bool("isDir", info.IsDir()).
			Msg("path pattern matched")

		metadata := map[string]interface{}{
			"pattern":  t.pattern,
			"fullPath": path,
			"isDir":    info.IsDir(),
		}
		return true, metadata
	}

	return false, nil
}

// Priority returns the priority of this trigger
func (t *PathPatternTrigger) Priority() int {
	return 70 // Medium priority
}

// ValidateOptions checks if the provided options are valid for this trigger
func (t *PathPatternTrigger) ValidateOptions(options map[string]interface{}) error {
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
	// Register the path pattern trigger factory
	err := registry.RegisterTriggerFactory(PathPatternTriggerName, func(options map[string]interface{}) (types.Trigger, error) {
		return NewPathPatternTrigger(options)
	})
	if err != nil {
		panic(fmt.Sprintf("failed to register path pattern trigger: %v", err))
	}
}
