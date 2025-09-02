package execution

// OperationStatus defines the state of an operation execution
type OperationStatus string

const (
	StatusReady    OperationStatus = "ready"
	StatusSkipped  OperationStatus = "skipped"
	StatusConflict OperationStatus = "conflict"
	StatusError    OperationStatus = "error"
	StatusUnknown  OperationStatus = "unknown"
)