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
	CatchallTriggerName     = "catchall"
	CatchallTriggerPriority = 0 // Lowest priority to run last
)

// CatchallTrigger matches all files and directories not matched by other triggers
type CatchallTrigger struct {
	excludePatterns []string
	priority        int
}

// NewCatchallTrigger creates a new CatchallTrigger with the given options
func NewCatchallTrigger(options map[string]interface{}) (*CatchallTrigger, error) {
	logger := logging.GetLogger("triggers.catchall")

	trigger := &CatchallTrigger{
		excludePatterns: []string{
			".dodot.toml",
			".dodotignore",
		},
		priority: CatchallTriggerPriority,
	}

	// Extract additional exclude patterns from options if provided
	if excludes, ok := options["excludePatterns"]; ok {
		switch v := excludes.(type) {
		case []string:
			trigger.excludePatterns = append(trigger.excludePatterns, v...)
		case []interface{}:
			for _, pattern := range v {
				if str, ok := pattern.(string); ok {
					trigger.excludePatterns = append(trigger.excludePatterns, str)
				}
			}
		}
	}

	logger.Trace().
		Strs("excludePatterns", trigger.excludePatterns).
		Msg("created catchall trigger")

	return trigger, nil
}

// Name returns the unique name of this trigger
func (t *CatchallTrigger) Name() string {
	return CatchallTriggerName
}

// Description returns a human-readable description of what this trigger matches
func (t *CatchallTrigger) Description() string {
	return "Matches all files and directories not matched by specific triggers"
}

// Match checks if the given file should be caught by the catchall
func (t *CatchallTrigger) Match(path string, info fs.FileInfo) (bool, map[string]interface{}) {
	logger := logging.GetLogger("triggers.catchall")

	// Check if the file matches any exclude pattern
	basename := filepath.Base(path)
	for _, pattern := range t.excludePatterns {
		matched, err := filepath.Match(pattern, basename)
		if err != nil {
			logger.Error().
				Err(err).
				Str("pattern", pattern).
				Str("path", path).
				Msg("error matching exclude pattern")
			continue
		}
		if matched {
			logger.Trace().
				Str("path", path).
				Str("pattern", pattern).
				Msg("file excluded by pattern")
			return false, nil
		}
	}

	// Catchall matches everything not excluded
	logger.Trace().
		Str("path", path).
		Bool("isDir", info.IsDir()).
		Msg("file matched catchall trigger")

	metadata := map[string]interface{}{
		"isDir":    info.IsDir(),
		"basename": basename,
	}

	return true, metadata
}

// Priority returns the priority of this trigger
func (t *CatchallTrigger) Priority() int {
	return t.priority
}

// Type returns the trigger type - this is a catchall trigger
func (t *CatchallTrigger) Type() types.TriggerType {
	return types.TriggerTypeCatchall
}

func init() {
	// Register the catchall trigger factory
	err := registry.RegisterTriggerFactory(CatchallTriggerName, func(config map[string]interface{}) (types.Trigger, error) {
		return NewCatchallTrigger(config)
	})
	if err != nil {
		panic(fmt.Sprintf("failed to register CatchallTrigger factory: %v", err))
	}
}
