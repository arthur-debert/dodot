package execution

// ExecutionStatus represents the overall status of a pack's execution
type ExecutionStatus string

const (
	// ExecutionStatusSuccess means all handlers succeeded
	ExecutionStatusSuccess ExecutionStatus = "success"

	// ExecutionStatusPartial means some handlers succeeded, some failed
	ExecutionStatusPartial ExecutionStatus = "partial"

	// ExecutionStatusError means all handlers failed
	ExecutionStatusError ExecutionStatus = "error"

	// ExecutionStatusSkipped means all handlers were skipped
	ExecutionStatusSkipped ExecutionStatus = "skipped"

	// ExecutionStatusPending means execution hasn't started
	ExecutionStatusPending ExecutionStatus = "pending"
)
