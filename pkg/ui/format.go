package ui

import (
	"fmt"
	"os"
	"strings"

	"github.com/mattn/go-isatty"
	"github.com/muesli/termenv"
)

// Format represents the output format type
type Format int

const (
	// FormatAuto automatically detects the appropriate format based on terminal capabilities
	FormatAuto Format = iota
	// FormatTerminal renders rich terminal output with colors and styling
	FormatTerminal
	// FormatText renders plain text output without any styling
	FormatText
	// FormatJSON renders machine-readable JSON output
	FormatJSON
)

// String returns the string representation of the format
func (f Format) String() string {
	switch f {
	case FormatAuto:
		return "auto"
	case FormatTerminal:
		return "term"
	case FormatText:
		return "text"
	case FormatJSON:
		return "json"
	default:
		return "unknown"
	}
}

// ParseFormat parses a string into a Format value
func ParseFormat(s string) (Format, error) {
	switch strings.ToLower(s) {
	case "auto", "":
		return FormatAuto, nil
	case "term", "terminal":
		return FormatTerminal, nil
	case "text", "plain":
		return FormatText, nil
	case "json":
		return FormatJSON, nil
	default:
		return FormatAuto, fmt.Errorf("unknown format: %s", s)
	}
}

// DetectFormat determines the appropriate output format based on environment and terminal capabilities
func DetectFormat(output *os.File) Format {
	// Check if NO_COLOR is set
	if os.Getenv("NO_COLOR") != "" {
		return FormatText
	}

	// Check if we're being piped or redirected
	if !isatty.IsTerminal(output.Fd()) && !isatty.IsCygwinTerminal(output.Fd()) {
		return FormatText
	}

	// Check terminal color support
	colorProfile := termenv.ColorProfile()
	if colorProfile == termenv.Ascii {
		return FormatText
	}

	// Terminal supports colors
	return FormatTerminal
}
