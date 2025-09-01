package topics

// Renderer defines the interface for rendering topic content
type Renderer interface {
	// Render takes raw content and returns formatted content for terminal display
	Render(content string, format string) string
}

// PlainRenderer is the default renderer that returns content as-is
type PlainRenderer struct{}

// Render returns the content unchanged
func (r *PlainRenderer) Render(content string, format string) string {
	return content
}
