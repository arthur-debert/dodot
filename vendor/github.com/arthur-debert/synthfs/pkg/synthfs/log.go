package synthfs

import (
	"io"
	"os"
	"strings"
	"time"

	"github.com/rs/zerolog"
)

// NewLogger creates a new logger instance with a specified level and output.
func NewLogger(w io.Writer, level zerolog.Level) zerolog.Logger {
	output := zerolog.ConsoleWriter{
		Out:        w,
		TimeFormat: time.RFC3339,
		NoColor:    true,
	}
	return zerolog.New(output).
		Level(level).
		With().
		Timestamp().
		Str("lib", "synthfs").
		Logger()
}

// NewTestLogger creates a logger instance for tests with a specified verbosity.
func NewTestLogger(w io.Writer, verbose int) zerolog.Logger {
	var level zerolog.Level
	switch verbose {
	case 0:
		level = zerolog.WarnLevel
	case 1:
		level = zerolog.InfoLevel
	case 2:
		level = zerolog.DebugLevel
	default:
		level = zerolog.TraceLevel
	}
	return NewLogger(w, level)
}

// LogLevelFromString parses a string to a zerolog.Level.
func LogLevelFromString(levelStr string) (zerolog.Level, error) {
	return zerolog.ParseLevel(strings.ToLower(levelStr))
}

// DefaultLogger returns a logger with default settings (warn level, stderr output).
func DefaultLogger() zerolog.Logger {
	return NewLogger(os.Stderr, zerolog.WarnLevel)
}
