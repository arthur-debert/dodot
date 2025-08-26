package ui_test

import (
	"testing"

	"github.com/arthur-debert/dodot/pkg/ui"
	"github.com/stretchr/testify/assert"
)

func TestFormatString(t *testing.T) {
	tests := []struct {
		format   ui.Format
		expected string
	}{
		{ui.FormatAuto, "auto"},
		{ui.FormatTerminal, "term"},
		{ui.FormatText, "text"},
		{ui.FormatJSON, "json"},
		{ui.Format(999), "unknown"},
	}

	for _, tt := range tests {
		t.Run(tt.expected, func(t *testing.T) {
			assert.Equal(t, tt.expected, tt.format.String())
		})
	}
}
