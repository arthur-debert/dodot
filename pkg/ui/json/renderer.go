// Package json provides machine-readable JSON output
package json

import (
	"encoding/json"
	"io"
)

// Renderer provides JSON output for machine consumption
type Renderer struct {
	output  io.Writer
	encoder *json.Encoder
}

// New creates a new JSON renderer
func New(output io.Writer) (*Renderer, error) {
	encoder := json.NewEncoder(output)
	encoder.SetIndent("", "  ")
	return &Renderer{
		output:  output,
		encoder: encoder,
	}, nil
}

// RenderResult renders any result type as JSON
func (r *Renderer) RenderResult(result interface{}) error {
	return r.encoder.Encode(result)
}

// RenderError renders an error as JSON
func (r *Renderer) RenderError(err error) error {
	errorObj := map[string]string{
		"error": err.Error(),
	}
	return r.encoder.Encode(errorObj)
}

// RenderMessage renders a simple message as JSON
func (r *Renderer) RenderMessage(msg string) error {
	messageObj := map[string]string{
		"message": msg,
	}
	return r.encoder.Encode(messageObj)
}
