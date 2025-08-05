package display

import (
	"fmt"
	"strings"

	"github.com/arthur-debert/dodot/pkg/types"
)

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
		Files:               make([]FileResult, 0, len(per.Operations)),
	}

	// Set pack description from metadata if available
	if per.Pack != nil && per.Pack.Metadata != nil {
		if desc, ok := per.Pack.Metadata["description"].(string); ok {
			result.Description = desc
		}
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

	result := FileResult{
		Action:      c.getActionVerb(op),
		Path:        c.makePathRelative(c.getDisplayPath(op)),
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
	switch or.Status {
	case types.StatusReady:
		return "Applied"
	case types.StatusSkipped:
		// Provide more context for skipped operations
		if or.Operation.PowerUp == "homebrew" || or.Operation.PowerUp == "install" {
			return "Already processed (checksum match)"
		}
		return "Already up to date"
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

// GroupPacksByStatus groups packs by their execution status for summary display
func GroupPacksByStatus(packs []PackResult) map[types.ExecutionStatus][]string {
	groups := make(map[types.ExecutionStatus][]string)
	for _, pack := range packs {
		groups[pack.Status] = append(groups[pack.Status], pack.Name)
	}
	return groups
}
