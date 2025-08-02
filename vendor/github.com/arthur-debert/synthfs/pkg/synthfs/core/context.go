package core

// Logger interface defines logging capabilities
type Logger interface {
	Info() LogEvent
	Debug() LogEvent
	Warn() LogEvent
	Error() LogEvent
	Trace() LogEvent
}

// LogEvent interface for structured logging
type LogEvent interface {
	Str(key, val string) LogEvent
	Int(key string, val int) LogEvent
	Err(err error) LogEvent
	Float64(key string, val float64) LogEvent
	Bool(key string, val bool) LogEvent
	Dur(key string, val interface{}) LogEvent
	Interface(key string, val interface{}) LogEvent
	Msg(msg string)
}

// ExecutionContext provides all the dependencies needed for operation execution
type ExecutionContext struct {
	Logger   Logger
	Budget   *BackupBudget
	EventBus EventBus
	// Note: FileSystem will be passed separately to avoid import cycles
}
