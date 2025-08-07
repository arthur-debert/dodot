package synthfs

import (
	"bytes"
	"context"
	"fmt"
	"io/fs"
	"text/template"

	"github.com/arthur-debert/synthfs/pkg/synthfs/core"
	"github.com/arthur-debert/synthfs/pkg/synthfs/filesystem"
)

// TemplateData holds template data for rendering
type TemplateData map[string]interface{}

// WriteTemplateOperation writes files from templates
type WriteTemplateOperation struct {
	id       OperationID
	desc     OperationDesc
	path     string
	template string
	data     TemplateData
	mode     fs.FileMode
}

// NewWriteTemplateOperation creates a new template write operation
func (s *SynthFS) NewWriteTemplateOperation(path, templateContent string, data TemplateData, mode fs.FileMode) *WriteTemplateOperation {
	id := s.idGen("write_template", path)
	return &WriteTemplateOperation{
		id: id,
		desc: OperationDesc{
			Type: "write_template",
			Path: path,
			Details: map[string]interface{}{
				"template": templateContent,
				"data":     data,
				"mode":     mode,
			},
		},
		path:     path,
		template: templateContent,
		data:     data,
		mode:     mode,
	}
}

// ID returns the operation ID
func (op *WriteTemplateOperation) ID() OperationID {
	return op.id
}

// Describe returns the operation description
func (op *WriteTemplateOperation) Describe() OperationDesc {
	return op.desc
}

// Dependencies returns empty - no dependencies
func (op *WriteTemplateOperation) Dependencies() []OperationID {
	return nil
}

// Conflicts returns empty - no conflicts
func (op *WriteTemplateOperation) Conflicts() []OperationID {
	return nil
}

// Prerequisites returns prerequisites for the operation
func (op *WriteTemplateOperation) Prerequisites() []core.Prerequisite {
	return []core.Prerequisite{
		core.NewParentDirPrerequisite(op.path),
		core.NewNoConflictPrerequisite(op.path),
	}
}

// GetItem returns nil - no specific item
func (op *WriteTemplateOperation) GetItem() FsItem {
	return nil
}

// SetDescriptionDetail sets a detail in the description
func (op *WriteTemplateOperation) SetDescriptionDetail(key string, value interface{}) {
	if op.desc.Details == nil {
		op.desc.Details = make(map[string]interface{})
	}
	op.desc.Details[key] = value
}

// AddDependency adds a dependency
func (op *WriteTemplateOperation) AddDependency(depID OperationID) {
	// Not implemented for this operation
}

// SetPaths sets source and destination paths
func (op *WriteTemplateOperation) SetPaths(src, dst string) {
	op.path = dst
	op.desc.Path = dst
}

// GetChecksum returns nil
func (op *WriteTemplateOperation) GetChecksum(path string) *ChecksumRecord {
	return nil
}

// GetAllChecksums returns nil
func (op *WriteTemplateOperation) GetAllChecksums() map[string]*ChecksumRecord {
	return nil
}

// ExecuteV2 is not implemented
func (op *WriteTemplateOperation) ExecuteV2(ctx interface{}, execCtx *core.ExecutionContext, fsys interface{}) error {
	if contextOp, ok := ctx.(context.Context); ok {
		if fsysOp, ok := fsys.(FileSystem); ok {
			return op.Execute(contextOp, fsysOp)
		}
	}
	return fmt.Errorf("ExecuteV2 not implemented for WriteTemplateOperation")
}

// ValidateV2 is not implemented
func (op *WriteTemplateOperation) ValidateV2(ctx interface{}, execCtx *core.ExecutionContext, fsys interface{}) error {
	if contextOp, ok := ctx.(context.Context); ok {
		if fsysOp, ok := fsys.(FileSystem); ok {
			return op.Validate(contextOp, fsysOp)
		}
	}
	return fmt.Errorf("ValidateV2 not implemented for WriteTemplateOperation")
}

// Rollback removes the created file
func (op *WriteTemplateOperation) Rollback(ctx context.Context, fsys FileSystem) error {
	if writeFS, ok := fsys.(WriteFS); ok {
		return writeFS.Remove(op.path)
	}
	return fmt.Errorf("filesystem does not support Remove")
}

// ReverseOps generates reverse operations
func (op *WriteTemplateOperation) ReverseOps(ctx context.Context, fsys FileSystem, budget *core.BackupBudget) ([]Operation, *core.BackupData, error) {
	// Would create a delete operation
	deleteOp := New().Delete(op.path)
	return []Operation{deleteOp}, nil, nil
}

// Execute performs the template write operation
func (op *WriteTemplateOperation) Execute(ctx context.Context, fsys FileSystem) error {
	// Parse and execute template
	tmpl, err := template.New("file").Parse(op.template)
	if err != nil {
		return fmt.Errorf("failed to parse template: %w", err)
	}

	var buf bytes.Buffer
	if err := tmpl.Execute(&buf, op.data); err != nil {
		return fmt.Errorf("failed to execute template: %w", err)
	}

	// Write the rendered content
	if writeFS, ok := fsys.(WriteFS); ok {
		return writeFS.WriteFile(op.path, buf.Bytes(), op.mode)
	}

	return fmt.Errorf("filesystem does not support WriteFile")
}

// Validate checks if the operation can be performed
func (op *WriteTemplateOperation) Validate(ctx context.Context, fsys FileSystem) error {
	// Validate template syntax
	if _, err := template.New("validate").Parse(op.template); err != nil {
		return fmt.Errorf("invalid template syntax: %w", err)
	}

	// Check if filesystem supports write
	if _, ok := fsys.(WriteFS); !ok {
		return fmt.Errorf("filesystem does not support WriteFile")
	}

	return nil
}

// WriteTemplate creates a template write operation
func (s *SynthFS) WriteTemplate(path, templateContent string, data TemplateData) Operation {
	return s.NewWriteTemplateOperation(path, templateContent, data, 0644)
}

// WriteTemplateWithMode creates a template write operation with custom mode
func (s *SynthFS) WriteTemplateWithMode(path, templateContent string, data TemplateData, mode fs.FileMode) Operation {
	return s.NewWriteTemplateOperation(path, templateContent, data, mode)
}

// WriteTemplateFile is a convenience function that writes a template directly
func WriteTemplateFile(ctx context.Context, fs filesystem.FullFileSystem, path, templateContent string, data TemplateData) error {
	op := New().WriteTemplate(path, templateContent, data)
	return op.Execute(ctx, fs)
}

// TemplateBuilder provides a fluent interface for template operations
type TemplateBuilder struct {
	path     string
	template string
	data     TemplateData
	mode     fs.FileMode
}

// NewTemplateBuilder creates a new template builder
func NewTemplateBuilder(path string) *TemplateBuilder {
	return &TemplateBuilder{
		path: path,
		data: make(TemplateData),
		mode: 0644,
	}
}

// WithTemplate sets the template content
func (tb *TemplateBuilder) WithTemplate(template string) *TemplateBuilder {
	tb.template = template
	return tb
}

// WithData sets all template data at once
func (tb *TemplateBuilder) WithData(data TemplateData) *TemplateBuilder {
	tb.data = data
	return tb
}

// Set adds a single key-value pair to the template data
func (tb *TemplateBuilder) Set(key string, value interface{}) *TemplateBuilder {
	tb.data[key] = value
	return tb
}

// WithMode sets the file mode
func (tb *TemplateBuilder) WithMode(mode fs.FileMode) *TemplateBuilder {
	tb.mode = mode
	return tb
}

// Build creates the write template operation
func (tb *TemplateBuilder) Build() Operation {
	return New().NewWriteTemplateOperation(tb.path, tb.template, tb.data, tb.mode)
}

// Execute builds and executes the operation
func (tb *TemplateBuilder) Execute(ctx context.Context, fs filesystem.FullFileSystem) error {
	op := tb.Build()
	return op.Execute(ctx, fs)
}

// BatchTemplateWriter helps write multiple templates
type BatchTemplateWriter struct {
	templates map[string]struct {
		template string
		data     TemplateData
		mode     fs.FileMode
	}
}

// NewBatchTemplateWriter creates a new batch template writer
func NewBatchTemplateWriter() *BatchTemplateWriter {
	return &BatchTemplateWriter{
		templates: make(map[string]struct {
			template string
			data     TemplateData
			mode     fs.FileMode
		}),
	}
}

// Add adds a template to the batch
func (btw *BatchTemplateWriter) Add(path, template string, data TemplateData) *BatchTemplateWriter {
	btw.templates[path] = struct {
		template string
		data     TemplateData
		mode     fs.FileMode
	}{
		template: template,
		data:     data,
		mode:     0644,
	}
	return btw
}

// AddWithMode adds a template with custom mode
func (btw *BatchTemplateWriter) AddWithMode(path, template string, data TemplateData, mode fs.FileMode) *BatchTemplateWriter {
	btw.templates[path] = struct {
		template string
		data     TemplateData
		mode     fs.FileMode
	}{
		template: template,
		data:     data,
		mode:     mode,
	}
	return btw
}

// BuildOperations creates all template operations
func (btw *BatchTemplateWriter) BuildOperations() []Operation {
	sfs := New()
	var ops []Operation
	for path, tmpl := range btw.templates {
		op := sfs.NewWriteTemplateOperation(path, tmpl.template, tmpl.data, tmpl.mode)
		ops = append(ops, op)
	}
	return ops
}

// Execute writes all templates
func (btw *BatchTemplateWriter) Execute(ctx context.Context, fs filesystem.FullFileSystem) error {
	ops := btw.BuildOperations()
	result, err := Run(ctx, fs, ops...)
	if err != nil {
		return err
	}
	if !result.IsSuccess() {
		return fmt.Errorf("batch template write failed")
	}
	return nil
}
