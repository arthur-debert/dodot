package logging

import (
	"bytes"
	"errors"
	"os"
	"strings"
	"testing"
	"time"

	"github.com/rs/zerolog"
	"github.com/rs/zerolog/log"
	"github.com/stretchr/testify/assert"
)

func TestLogCommand(t *testing.T) {
	// Capture log output
	var buf bytes.Buffer

	// Set up logger with our buffer before calling SetupLogger
	log.Logger = zerolog.New(&buf).Level(zerolog.DebugLevel)

	// Log a command
	LogCommand("test-cmd", []string{"arg1", "arg2"})

	// Check output
	output := buf.String()
	assert.Contains(t, output, "test-cmd")
	assert.Contains(t, output, "arg1")
	assert.Contains(t, output, "arg2")
	assert.Contains(t, output, "Executing command")
}

func TestLogDuration(t *testing.T) {
	// Capture log output
	var buf bytes.Buffer

	// Set up logger with our buffer
	log.Logger = zerolog.New(&buf).Level(zerolog.DebugLevel)

	// Log a duration
	start := time.Now().Add(-5 * time.Second)
	LogDuration(start, "test-operation")

	// Check output
	output := buf.String()
	assert.Contains(t, output, "test-operation")
	assert.Contains(t, output, "duration")
	// Should contain a duration of approximately 5 seconds
	assert.True(t, strings.Contains(output, "5") || strings.Contains(output, "5000"))
}

func TestMust_NoError(t *testing.T) {
	// Should not panic when error is nil
	assert.NotPanics(t, func() {
		Must(nil, "this should not panic")
	})
}

func TestMust_WithError(t *testing.T) {
	if os.Getenv("BE_CRASHER") == "1" {
		Must(errors.New("test error"), "expected panic")
		return
	}

	// Run the test in a subprocess
	cmd := os.Args[0]
	args := []string{"-test.run=TestMust_WithError"}
	env := append(os.Environ(), "BE_CRASHER=1")

	// Create command
	proc := &os.ProcAttr{
		Env:   env,
		Files: []*os.File{os.Stdin, os.Stdout, os.Stderr},
	}

	process, err := os.StartProcess(cmd, append([]string{cmd}, args...), proc)
	if err != nil {
		t.Fatal(err)
	}

	// Wait for process to exit
	state, err := process.Wait()
	if err != nil {
		t.Fatal(err)
	}

	// Should have exited with non-zero status
	assert.False(t, state.Success(), "process should have exited with error")
}
