package display

import (
	"fmt"
	"strings"

	"github.com/arthur-debert/dodot/pkg/types"
)

// FIXME: ARCHITECTURAL PROBLEM - Display converter should NOT work with Operations!
// It should work with Pack+PowerUp+File results, not operation results.
// The execution system should roll up operation status to PowerUp level:
// - PowerUp has 7 operations, any fail = PowerUp fails (atomic unit)
// - UI shows status at PowerUp level: "vim: symlink .vimrc -> failed"
// - NOT individual operation statuses: "Operation CreateDir: success, Operation CreateSymlink: failed"
// See docs/design/display.txxt
// Converter transforms execution context data into display-friendly formats
type Converter struct {
	// homeDir is used to make paths relative when possible
	homeDir string
}

// NewConverter creates a new display converter
func NewConverter(homeDir string) *Converter {
	return &Converter{
		homeDir: homeDir,
	}
}

// ConvertExecutionContext transforms an ExecutionContext into a CommandResult
// suitable for display. This is the main entry point for converting execution
// data into the display layer.
func (c *Converter) ConvertExecutionContext(ctx *types.ExecutionContext) CommandResult {
	result := CommandResult{
		Command:  ctx.Command,
		DryRun:   ctx.DryRun,
		Duration: ctx.EndTime.Sub(ctx.StartTime),
		Packs:    make([]PackResult, 0, len(ctx.PackResults)),
	}

	// Initialize summary
	result.Summary = Summary{
		StartTime: ctx.StartTime,
		EndTime:   ctx.EndTime,
		Duration:  result.Duration,
	}

	// Convert each pack result
	for packName, packExec := range ctx.PackResults {
		packResult := c.ConvertPackExecutionResult(packName, packExec)
		result.Packs = append(result.Packs, packResult)

		// Update summary statistics
		result.Summary.TotalPacks++
		result.Summary.TotalOperations += packResult.TotalOperations
		result.Summary.CompletedOperations += packResult.CompletedOperations
		result.Summary.FailedOperations += packResult.FailedOperations
		result.Summary.SkippedOperations += packResult.SkippedOperations

		// Track pack status
		switch packResult.Status {
		case types.ExecutionStatusSuccess:
			result.Summary.SuccessfulPacks++
		case types.ExecutionStatusPartial:
			result.Summary.PartialPacks++
		case types.ExecutionStatusError:
			result.Summary.FailedPacks++
		case types.ExecutionStatusSkipped:
			result.Summary.SkippedPacks++
		}
	}

	return result
}

// ConvertPackExecutionResult transforms a PackExecutionResult into a PackResult
func (c *Converter) ConvertPackExecutionResult(packName string, per *types.PackExecutionResult) PackResult {
	result := PackResult{
		Name:                packName,
		Status:              per.Status,
		TotalOperations:     per.TotalOperations,
		CompletedOperations: per.CompletedOperations,
		FailedOperations:    per.FailedOperations,
		SkippedOperations:   per.SkippedOperations,
		Files:               make([]FileResult, 0, len(per.Operations)+1), // +1 for potential config file
	}

	// Set pack description from metadata if available
	if per.Pack != nil && per.Pack.Metadata != nil {
		if desc, ok := per.Pack.Metadata["description"].(string); ok {
			result.Description = desc
		}
	}

	// Check if pack has a config file and add it as a special file result
	if per.Pack != nil && c.packHasConfig(per.Pack) {
		configFile := FileResult{
			PowerUp: "config",
			Path:    ".dodot.toml",
			Message: "dodot config file found",
			Status:  types.StatusReady, // Config files are always "ready"
		}
		result.Files = append(result.Files, configFile)
	}

	// Convert each operation result
	for _, opResult := range per.Operations {
		fileResult := c.ConvertOperationResult(opResult)
		result.Files = append(result.Files, fileResult)
	}

	return result
}

// ConvertOperationResult transforms an OperationResult into a FileResult
func (c *Converter) ConvertOperationResult(or *types.OperationResult) FileResult {
	op := or.Operation

	// Get the display path
	displayPath := c.makePathRelative(c.getDisplayPath(op))

	// Check if this file was overridden (marked with asterisk)
	if op.TriggerInfo != nil && op.TriggerInfo.TriggerName == "override-rule" {
		displayPath = "*" + displayPath
	}

	result := FileResult{
		Action:      c.getActionVerb(op),
		Path:        displayPath,
		Status:      or.Status,
		Message:     c.getStatusMessage(or),
		PowerUp:     op.PowerUp,
		Pack:        op.Pack,
		GroupID:     op.GroupID,
		Error:       or.Error,
		Output:      or.Output,
		IsNewChange: or.Status == types.StatusReady,
		Metadata:    or.Metadata,
	}

	// Extract last applied time if available
	if !or.EndTime.IsZero() {
		result.LastApplied = or.EndTime
	}

	return result
}

// getActionVerb returns the appropriate action verb based on the PowerUp type
// These verbs are defined in the design spec and provide clear, concise
// descriptions of what each PowerUp does
func (c *Converter) getActionVerb(op *types.Operation) string {
	// Map PowerUp names to their display verbs
	verbMap := map[string]string{
		"symlink":       "Link",
		"shell_profile": "Source",
		"add_path":      "Add to PATH",
		"homebrew":      "Install",
		"install":       "Run",
		"template":      "Generate",
	}

	if verb, ok := verbMap[op.PowerUp]; ok {
		return verb
	}

	// Fallback to operation type if PowerUp not recognized
	switch op.Type {
	case types.OperationCreateSymlink:
		return "Link"
	case types.OperationExecute:
		return "Execute"
	case types.OperationWriteFile:
		return "Write"
	case types.OperationCreateDir:
		return "Create"
	default:
		return "Process"
	}
}

// getDisplayPath returns the most appropriate path to display for an operation
// Prioritizes the target path for operations that create/modify files
func (c *Converter) getDisplayPath(op *types.Operation) string {
	// For execute operations (install scripts), show the source file name
	if op.Type == types.OperationExecute && op.TriggerInfo != nil && op.TriggerInfo.OriginalPath != "" {
		return op.TriggerInfo.OriginalPath
	}

	// For most operations, show the target path
	if op.Target != "" {
		return op.Target
	}
	// For source-only operations (like brew/install), show source
	return op.Source
}

// makePathRelative converts absolute paths to relative paths when possible
// This makes the output more readable by showing ~ for home directory
func (c *Converter) makePathRelative(path string) string {
	if path == "" {
		return path
	}

	// Replace home directory with ~
	if c.homeDir != "" && strings.HasPrefix(path, c.homeDir) {
		return "~" + strings.TrimPrefix(path, c.homeDir)
	}

	// For paths in current directory, remove leading ./
	if strings.HasPrefix(path, "./") {
		return strings.TrimPrefix(path, "./")
	}

	return path
}

// getStatusMessage generates an appropriate message based on operation status
// and any error information
func (c *Converter) getStatusMessage(or *types.OperationResult) string {
	// Get the appropriate verb tense based on status and PowerUp
	verb := c.getVerbForStatus(or.Operation.PowerUp, or.Status)

	switch or.Status {
	case types.StatusReady:
		// For operations that are ready to be executed (future tense)
		return verb
	case types.StatusSkipped:
		// For operations that were skipped because already done (past tense)
		return verb
	case types.StatusConflict:
		if or.Error != nil {
			return fmt.Sprintf("Conflict: %s", or.Error.Error())
		}
		return "Conflict detected"
	case types.StatusError:
		if or.Error != nil {
			return fmt.Sprintf("Error: %s", or.Error.Error())
		}
		return "Failed"
	default:
		return string(or.Status)
	}
}

// getVerbForStatus returns the appropriate verb tense based on PowerUp type and status
func (c *Converter) getVerbForStatus(powerUp string, status types.OperationStatus) string {
	// Define verb forms for each PowerUp type
	verbForms := map[string]struct {
		past   string
		future string
	}{
		"symlink":       {past: "linked to target", future: "will be linked to target"},
		"shell_profile": {past: "included in shell", future: "to be included in shell"},
		"homebrew":      {past: "executed", future: "to be installed"},
		"add_path":      {past: "added to $PATH", future: "to be added to $PATH"},
		"install":       {past: "executed during installation", future: "to be executed"},
		"template":      {past: "generated from template", future: "to be generated"},
		"config":        {past: "found", future: "found"}, // Config always uses present tense
	}

	verbs, ok := verbForms[powerUp]
	if !ok {
		// Fallback for unknown PowerUps
		if status == types.StatusReady {
			return "to be processed"
		}
		return "processed"
	}

	// Choose past or future tense based on status
	if status == types.StatusReady {
		return verbs.future
	}
	return verbs.past
}

// ConvertFileStatus transforms a FileStatus (from status checking) into a FileResult
// This is used when we want to display the current status without execution
func (c *Converter) ConvertFileStatus(fs *types.FileStatus) FileResult {
	// Create a minimal operation to leverage existing conversion logic
	op := &types.Operation{
		PowerUp: fs.PowerUp,
		Target:  fs.Path,
	}

	// Create a synthetic operation result
	or := &types.OperationResult{
		Operation: op,
		Status:    fs.Status,
		Metadata:  fs.Metadata,
		EndTime:   fs.LastApplied,
	}

	result := c.ConvertOperationResult(or)
	result.Message = fs.Message

	return result
}

// packHasConfig checks if a pack has a .dodot.toml configuration file
func (c *Converter) packHasConfig(pack *types.Pack) bool {
	// Check if the pack has any configuration rules
	return len(pack.Config.Ignore) > 0 || len(pack.Config.Override) > 0
}

// GroupPacksByStatus groups packs by their execution status for summary display
func GroupPacksByStatus(packs []PackResult) map[types.ExecutionStatus][]string {
	groups := make(map[types.ExecutionStatus][]string)
	for _, pack := range packs {
		groups[pack.Status] = append(groups[pack.Status], pack.Name)
	}
	return groups
}
