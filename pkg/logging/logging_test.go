package logging

import (
	"os"
	"path/filepath"
	"strings"
	"testing"

	"github.com/rs/zerolog"
)

func TestSetupLogger(t *testing.T) {
	tests := []struct {
		name      string
		verbosity int
		wantLevel zerolog.Level
	}{
		{"default warn level", 0, zerolog.WarnLevel},
		{"info level", 1, zerolog.InfoLevel},
		{"debug level", 2, zerolog.DebugLevel},
		{"trace level", 3, zerolog.TraceLevel},
		{"high verbosity defaults to trace", 5, zerolog.TraceLevel},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			// Create temp dir for log file
			tempDir := t.TempDir()
			t.Setenv("XDG_STATE_HOME", tempDir)

			SetupLogger(tt.verbosity)

			if zerolog.GlobalLevel() != tt.wantLevel {
				t.Errorf("SetupLogger(%d) set level to %v, want %v",
					tt.verbosity, zerolog.GlobalLevel(), tt.wantLevel)
			}

			// Check that log file was created
			logPath := filepath.Join(tempDir, "dodot", "dodot.log")
			if _, err := os.Stat(logPath); os.IsNotExist(err) {
				t.Errorf("Log file was not created at %s", logPath)
			}
		})
	}
}

func TestGetLogFilePath(t *testing.T) {
	tests := []struct {
		name        string
		xdgState    string
		wantContains string
	}{
		{
			name:        "with XDG_STATE_HOME",
			xdgState:    "/custom/state",
			wantContains: "/custom/state/dodot/dodot.log",
		},
		{
			name:        "without XDG_STATE_HOME",
			xdgState:    "",
			wantContains: ".local/state/dodot/dodot.log",
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			if tt.xdgState != "" {
				t.Setenv("XDG_STATE_HOME", tt.xdgState)
			}

			got := getLogFilePath()
			if !filepath.IsAbs(got) {
				t.Errorf("getLogFilePath() returned relative path: %s", got)
			}
			if !contains(got, tt.wantContains) {
				t.Errorf("getLogFilePath() = %s, want to contain %s", got, tt.wantContains)
			}
		})
	}
}

func TestGetLogger(t *testing.T) {
	logger := GetLogger("test-component")
	
	// This is a basic test - in practice we'd capture the output
	// and verify the component field is set
	logger.Info().Msg("test message")
}

func TestWithFields(t *testing.T) {
	fields := map[string]interface{}{
		"key1": "value1",
		"key2": 42,
		"key3": true,
	}

	logger := WithFields(fields)
	
	// This is a basic test - in practice we'd capture the output
	// and verify all fields are present
	logger.Info().Msg("test message with fields")
}

// Helper function
func contains(s, substr string) bool {
	// Clean paths to handle different OS separators
	cleanedS := filepath.ToSlash(s)
	cleanedSubstr := filepath.ToSlash(substr)
	return strings.Contains(cleanedS, cleanedSubstr)
}