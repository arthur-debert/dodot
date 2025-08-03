package triggers

import (
	"fmt"
	"io/fs"
	"path/filepath"
	"strings"

	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/registry"
	"github.com/arthur-debert/dodot/pkg/types"
)

// ExtensionTriggerName is the name used to reference this trigger
const ExtensionTriggerName = "extension"

// ExtensionTrigger matches files by their extension
type ExtensionTrigger struct {
	extension string
}

// NewExtensionTrigger creates a new ExtensionTrigger with the given options
func NewExtensionTrigger(options map[string]interface{}) (*ExtensionTrigger, error) {
	logger := logging.GetLogger("triggers.extension")

	// Extract extension from options
	extension, ok := options["extension"].(string)
	if !ok || extension == "" {
		return nil, fmt.Errorf("extension trigger requires an 'extension' option")
	}

	// Ensure extension starts with a dot
	if !strings.HasPrefix(extension, ".") {
		extension = "." + extension
	}

	logger.Trace().
		Str("extension", extension).
		Msg("created extension trigger")

	return &ExtensionTrigger{
		extension: extension,
	}, nil
}

// Name returns the name of this trigger
func (t *ExtensionTrigger) Name() string {
	return ExtensionTriggerName
}

// Description returns a human-readable description of this trigger
func (t *ExtensionTrigger) Description() string {
	return fmt.Sprintf("Matches files with extension '%s'", t.extension)
}

// Match checks if the given path matches this trigger's extension
func (t *ExtensionTrigger) Match(path string, info fs.FileInfo) (bool, map[string]interface{}) {
	// Skip directories
	if info.IsDir() {
		return false, nil
	}

	// Get the file extension
	ext := filepath.Ext(path)

	// Check if it matches our extension
	if strings.EqualFold(ext, t.extension) {
		logger := logging.GetLogger("triggers.extension")
		logger.Trace().
			Str("path", path).
			Str("extension", t.extension).
			Msg("file extension matched")

		metadata := map[string]interface{}{
			"extension": ext,
			"basename":  strings.TrimSuffix(filepath.Base(path), ext),
		}
		return true, metadata
	}

	return false, nil
}

// Priority returns the priority of this trigger
func (t *ExtensionTrigger) Priority() int {
	return 80 // Medium-high priority
}

// Type returns the trigger type - this is a specific trigger
func (t *ExtensionTrigger) Type() types.TriggerType {
	return types.TriggerTypeSpecific
}

// ValidateOptions checks if the provided options are valid for this trigger
func (t *ExtensionTrigger) ValidateOptions(options map[string]interface{}) error {
	if options == nil {
		return fmt.Errorf("options cannot be nil")
	}

	extension, ok := options["extension"]
	if !ok {
		return fmt.Errorf("missing required option: extension")
	}

	if _, ok := extension.(string); !ok {
		return fmt.Errorf("extension must be a string")
	}

	// Check for unknown options
	for key := range options {
		if key != "extension" {
			return fmt.Errorf("unknown option: %s", key)
		}
	}

	return nil
}

func init() {
	// Register the extension trigger factory
	err := registry.RegisterTriggerFactory(ExtensionTriggerName, func(options map[string]interface{}) (types.Trigger, error) {
		return NewExtensionTrigger(options)
	})
	if err != nil {
		panic(fmt.Sprintf("failed to register extension trigger: %v", err))
	}
}
