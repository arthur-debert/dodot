package logging

import (
	"os"
	"time"

	"github.com/rs/zerolog"
	"github.com/rs/zerolog/log"
)

// SetupLogger configures the global logger based on verbosity level
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
	output := zerolog.ConsoleWriter{
		Out:        os.Stderr,
		TimeFormat: time.RFC3339,
		NoColor:    false,
	}

	log.Logger = log.Output(output)

	// Add caller information for debug and trace levels
	if verbosity >= 2 {
		log.Logger = log.Logger.With().Caller().Logger()
	}

	// Log the logging level
	log.Debug().Int("verbosity", verbosity).Msg("Logger initialized")
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