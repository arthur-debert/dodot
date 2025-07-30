package logging

import (
	"fmt"
	"io"
	"os"
	"path/filepath"
	"time"

	"github.com/rs/zerolog"
	"github.com/rs/zerolog/log"
)

// SetupLogger configures the global logger based on verbosity level
// It sets up dual output to both console and a log file
func SetupLogger(verbosity int) {
	// Configure zerolog based on verbosity
	switch verbosity {
	case 0:
		zerolog.SetGlobalLevel(zerolog.WarnLevel)
	case 1:
		zerolog.SetGlobalLevel(zerolog.InfoLevel)
	case 2:
		zerolog.SetGlobalLevel(zerolog.DebugLevel)
	default:
		zerolog.SetGlobalLevel(zerolog.TraceLevel)
	}

	// Configure console output with pretty printing
	consoleWriter := zerolog.ConsoleWriter{
		Out:        os.Stderr,
		TimeFormat: time.Kitchen,
		NoColor:    false,
	}

	// Set up file logging
	var writers []io.Writer
	writers = append(writers, consoleWriter)

	// Get log file path from XDG_STATE_HOME or default
	logFile := getLogFilePath()
	logFileHandle, err := setupLogFile(logFile)
	if err == nil {
		writers = append(writers, logFileHandle)
	}

	// Create multi-writer
	multi := io.MultiWriter(writers...)
	log.Logger = zerolog.New(multi).With().Timestamp().Logger()

	// If we couldn't create the log file, log the error now with the new logger
	if err != nil {
		log.Warn().Err(err).Str("path", logFile).Msg("Failed to create log file, logging to console only")
	}

	// Add caller information for debug and trace levels
	if verbosity >= 2 {
		log.Logger = log.Logger.With().Caller().Logger()
	}

	// Log the logging level
	log.Debug().Int("verbosity", verbosity).Str("logFile", logFile).Msg("Logger initialized")
}

// GetLogger returns a contextualized logger with the given name
func GetLogger(name string) zerolog.Logger {
	return log.With().Str("component", name).Logger()
}

// WithFields returns a logger with additional fields
func WithFields(fields map[string]interface{}) zerolog.Logger {
	logger := log.Logger
	for k, v := range fields {
		logger = logger.With().Interface(k, v).Logger()
	}
	return logger
}

// getLogFilePath returns the path to the log file
// It respects XDG_STATE_HOME if set, otherwise uses ~/.local/state/dodot/
func getLogFilePath() string {
	stateHome := os.Getenv("XDG_STATE_HOME")
	if stateHome == "" {
		home, err := os.UserHomeDir()
		if err != nil {
			// Fallback to current directory if we can't get home
			return "dodot.log"
		}
		stateHome = filepath.Join(home, ".local", "state")
	}
	return filepath.Join(stateHome, "dodot", "dodot.log")
}

// setupLogFile creates the log file and its parent directories
func setupLogFile(logPath string) (*os.File, error) {
	// Create parent directories
	logDir := filepath.Dir(logPath)
	if err := os.MkdirAll(logDir, 0755); err != nil {
		return nil, fmt.Errorf("failed to create log directory: %w", err)
	}

	// Open log file in append mode
	file, err := os.OpenFile(logPath, os.O_CREATE|os.O_APPEND|os.O_WRONLY, 0644)
	if err != nil {
		return nil, fmt.Errorf("failed to open log file: %w", err)
	}

	return file, nil
}

// Must logs a fatal error and exits if err is not nil
func Must(err error, msg string) {
	if err != nil {
		log.Fatal().Err(err).Msg(msg)
	}
}

// LogCommand logs a command execution with its arguments
func LogCommand(cmd string, args []string) {
	log.Debug().
		Str("command", cmd).
		Strs("args", args).
		Msg("Executing command")
}

// LogDuration logs the duration of an operation
func LogDuration(start time.Time, operation string) {
	log.Debug().
		Str("operation", operation).
		Dur("duration", time.Since(start)).
		Msg("Operation completed")
}

// LogOperationStart logs the start of an operation and returns a function to log its completion
func LogOperationStart(logger zerolog.Logger, operation string) func() {
	start := time.Now()
	logger.Debug().
		Str("operation", operation).
		Msg("Operation started")

	return func() {
		logger.Debug().
			Str("operation", operation).
			Dur("duration", time.Since(start)).
			Msg("Operation completed")
	}
}
