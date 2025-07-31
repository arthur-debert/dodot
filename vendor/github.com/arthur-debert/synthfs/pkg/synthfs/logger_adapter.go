package synthfs

import (
	"github.com/arthur-debert/synthfs/pkg/synthfs/core"
	"github.com/rs/zerolog"
)

// loggerAdapter adapts zerolog.Logger to core.Logger interface
type loggerAdapter struct {
	logger *zerolog.Logger
}

// NewLoggerAdapter creates a new logger adapter
func NewLoggerAdapter(logger *zerolog.Logger) core.Logger {
	return &loggerAdapter{logger: logger}
}

// logEventAdapter adapts zerolog.Event to core.LogEvent interface
type logEventAdapter struct {
	event *zerolog.Event
}

func (l *loggerAdapter) Info() core.LogEvent {
	return &logEventAdapter{event: l.logger.Info()}
}

func (l *loggerAdapter) Debug() core.LogEvent {
	return &logEventAdapter{event: l.logger.Debug()}
}

func (l *loggerAdapter) Warn() core.LogEvent {
	return &logEventAdapter{event: l.logger.Warn()}
}

func (l *loggerAdapter) Error() core.LogEvent {
	return &logEventAdapter{event: l.logger.Error()}
}

func (l *loggerAdapter) Trace() core.LogEvent {
	return &logEventAdapter{event: l.logger.Trace()}
}

// Implement LogEvent methods
func (e *logEventAdapter) Str(key, val string) core.LogEvent {
	e.event = e.event.Str(key, val)
	return e
}

func (e *logEventAdapter) Int(key string, val int) core.LogEvent {
	e.event = e.event.Int(key, val)
	return e
}

func (e *logEventAdapter) Err(err error) core.LogEvent {
	e.event = e.event.Err(err)
	return e
}

func (e *logEventAdapter) Float64(key string, val float64) core.LogEvent {
	e.event = e.event.Float64(key, val)
	return e
}

func (e *logEventAdapter) Bool(key string, val bool) core.LogEvent {
	e.event = e.event.Bool(key, val)
	return e
}

func (e *logEventAdapter) Dur(key string, val interface{}) core.LogEvent {
	// Convert to time.Duration if possible
	e.event = e.event.Interface(key, val)
	return e
}

func (e *logEventAdapter) Interface(key string, val interface{}) core.LogEvent {
	e.event = e.event.Interface(key, val)
	return e
}

func (e *logEventAdapter) Msg(msg string) {
	e.event.Msg(msg)
}
