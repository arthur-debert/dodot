// Package output implements a declarative, template-based rendering system for
// dodot's command-line interface.
//
// # Architecture Overview
//
// The output package provides a clean separation between data, structure, and
// presentation through three main components:
//
//  1. Style Registry (styles/): Defines all visual styles using lipgloss
//  2. Templates (templates/): Go templates that define output structure
//  3. Renderer: Orchestrates template execution and style application
//
// # Rendering Pipeline
//
// The rendering process follows these steps:
//
//  1. Commands return structured data (DisplayResult or ExecutionContext)
//  2. Renderer executes the appropriate Go template with the data
//  3. Template output contains XML-like style tags (e.g., <Bold>text</Bold>)
//  4. Lipbalm expands style tags to ANSI escape codes
//  5. Final output is written to the provided io.Writer
//
// # Usage Example
//
//	// Create a renderer
//	renderer, err := output.NewRenderer(os.Stdout, false)
//	if err != nil {
//	    return err
//	}
//
//	// Render a DisplayResult
//	result := &types.DisplayResult{
//	    Command: "status",
//	    Packs: []types.DisplayPack{...},
//	}
//	err = renderer.Render(result)
//
// # Template System
//
// Templates use standard Go text/template syntax with custom style tags:
//
//	<CommandHeader>{{.Command}}</CommandHeader>
//	<Handler>{{.Handler}}</Handler>
//	<Success>✓</Success>
//
// Style tags correspond to entries in the style registry and are automatically
// expanded to the appropriate ANSI codes based on terminal capabilities.
//
// # Color Support
//
// The renderer automatically detects terminal capabilities and handles:
//   - Full color terminals (256 colors, true color)
//   - NO_COLOR environment variable
//   - Adaptive colors that adjust to light/dark themes
//   - Graceful fallback to plain text
//
// # Extending the System
//
// To add new output formats:
//  1. Define new styles in styles/styles.go
//  2. Create templates in templates/
//  3. Use the style tags in your templates
//
// For detailed architecture documentation, see docs/dev/20_cli-architecture.txxt
package output
