// Test Type: Unit Test
// Description: Tests for the logging package - logger configuration and utility functions

package logging_test

import (
	"bytes"
	"os"
	"path/filepath"
	"testing"
	"time"

	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/rs/zerolog"
	"github.com/rs/zerolog/log"
	"github.com/stretchr/testify/assert"
)

func TestSetupLogger_VerbosityLevels(t *testing.T) {
	tests := []struct {
		name      string
		verbosity int
		wantLevel zerolog.Level
	}{
		{
			name:      "verbosity_0_sets_warn_level",
			verbosity: 0,
			wantLevel: zerolog.WarnLevel,
		},
		{
			name:      "verbosity_1_sets_info_level",
			verbosity: 1,
			wantLevel: zerolog.InfoLevel,
		},
		{
			name:      "verbosity_2_sets_debug_level",
			verbosity: 2,
			wantLevel: zerolog.DebugLevel,
		},
		{
			name:      "verbosity_3_sets_trace_level",
			verbosity: 3,
			wantLevel: zerolog.TraceLevel,
		},
		{
			name:      "high_verbosity_defaults_to_trace",
			verbosity: 5,
			wantLevel: zerolog.TraceLevel,
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			// Create temp dir for log file
			tempDir := t.TempDir()
			t.Setenv("XDG_STATE_HOME", tempDir)

			// Setup logger
			logging.SetupLogger(tt.verbosity)

			// Check global level
			assert.Equal(t, tt.wantLevel, zerolog.GlobalLevel())

			// Verify log file was created
			logPath := filepath.Join(tempDir, "dodot", "dodot.log")
			_, err := os.Stat(logPath)
			assert.NoError(t, err, "Log file should be created")
		})
	}
}

func TestGetLogger(t *testing.T) {
	// Set up a buffer to capture log output
	var buf bytes.Buffer
	log.Logger = zerolog.New(&buf).With().Timestamp().Logger()

	// Get a logger with component name
	logger := logging.GetLogger("test-component")
	logger.Info().Msg("test message")

	// Verify the output contains the component field
	output := buf.String()
	assert.Contains(t, output, "test-component")
	assert.Contains(t, output, "test message")
	assert.Contains(t, output, "component")
}

func TestWithFields(t *testing.T) {
	// Set up a buffer to capture log output
	var buf bytes.Buffer
	log.Logger = zerolog.New(&buf).With().Timestamp().Logger()

	// Create logger with fields
	fields := map[string]interface{}{
		"key1": "value1",
		"key2": 42,
		"key3": true,
	}

	logger := logging.WithFields(fields)
	logger.Info().Msg("test with fields")

	// Verify all fields are in the output
	output := buf.String()
	assert.Contains(t, output, "key1")
	assert.Contains(t, output, "value1")
	assert.Contains(t, output, "key2")
	assert.Contains(t, output, "42")
	assert.Contains(t, output, "key3")
	assert.Contains(t, output, "true")
}

func TestLogCommand(t *testing.T) {
	// Set up a buffer to capture log output
	var buf bytes.Buffer
	log.Logger = zerolog.New(&buf).Level(zerolog.DebugLevel)

	// Log a command
	logging.LogCommand("test-cmd", []string{"arg1", "arg2", "--flag"})

	// Verify output
	output := buf.String()
	assert.Contains(t, output, "test-cmd")
	assert.Contains(t, output, "arg1")
	assert.Contains(t, output, "arg2")
	assert.Contains(t, output, "--flag")
	assert.Contains(t, output, "Executing command")
}

func TestLogDuration(t *testing.T) {
	// Set up a buffer to capture log output
	var buf bytes.Buffer
	log.Logger = zerolog.New(&buf).Level(zerolog.DebugLevel)

	// Log a duration
	start := time.Now().Add(-5 * time.Second)
	logging.LogDuration(start, "test-operation")

	// Verify output
	output := buf.String()
	assert.Contains(t, output, "test-operation")
	assert.Contains(t, output, "duration")
	assert.Contains(t, output, "Operation completed")
}

func TestLogOperationStart(t *testing.T) {
	// Set up a buffer to capture log output
	var buf bytes.Buffer
	logger := zerolog.New(&buf).Level(zerolog.DebugLevel)

	// Start an operation
	done := logging.LogOperationStart(logger, "test-operation")

	// Check start message
	output := buf.String()
	assert.Contains(t, output, "test-operation")
	assert.Contains(t, output, "Operation started")

	// Clear buffer
	buf.Reset()

	// Simulate some work
	time.Sleep(10 * time.Millisecond)

	// Call done
	done()

	// Check completion message
	output = buf.String()
	assert.Contains(t, output, "test-operation")
	assert.Contains(t, output, "Operation completed")
	assert.Contains(t, output, "duration")
}

func TestMust_NoError(t *testing.T) {
	// Must should not panic when error is nil
	assert.NotPanics(t, func() {
		logging.Must(nil, "should not panic")
	})
}

// TestMust_WithError is omitted because log.Fatal() exits the process
// rather than panicking, which requires subprocess testing to verify

func TestSetupLogger_LogFileCreation(t *testing.T) {
	tests := []struct {
		name         string
		xdgStateHome string
		wantLogPath  string
	}{
		{
			name:         "custom_xdg_state_home",
			xdgStateHome: "/custom/state",
			wantLogPath:  "/custom/state/dodot/dodot.log",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			tempDir := t.TempDir()

			if tt.xdgStateHome != "" {
				// Use temp dir as custom XDG_STATE_HOME
				customDir := filepath.Join(tempDir, "custom", "state")
				t.Setenv("XDG_STATE_HOME", customDir)
			}

			// Setup logger
			logging.SetupLogger(1)

			// Check log file exists
			var logPath string
			if tt.xdgStateHome != "" {
				logPath = filepath.Join(tempDir, "custom", "state", "dodot", "dodot.log")
			} else {
				// Default would be in XDG state home
				logPath = filepath.Join(tempDir, ".local", "state", "dodot", "dodot.log")
			}

			// Verify directory was created
			logDir := filepath.Dir(logPath)
			_, err := os.Stat(logDir)
			assert.NoError(t, err, "Log directory should be created")
		})
	}
}

func TestSetupLogger_MultipleCalls(t *testing.T) {
	// Create temp dir for log files
	tempDir := t.TempDir()
	t.Setenv("XDG_STATE_HOME", tempDir)

	// Call SetupLogger multiple times with different verbosity
	logging.SetupLogger(0)
	assert.Equal(t, zerolog.WarnLevel, zerolog.GlobalLevel())

	logging.SetupLogger(2)
	assert.Equal(t, zerolog.DebugLevel, zerolog.GlobalLevel())

	logging.SetupLogger(1)
	assert.Equal(t, zerolog.InfoLevel, zerolog.GlobalLevel())
}
