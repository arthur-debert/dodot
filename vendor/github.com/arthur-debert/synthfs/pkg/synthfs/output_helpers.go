package synthfs

// GetOperationOutput retrieves stored output from an operation's description details.
// This is useful for accessing output from shell commands or custom operations after execution.
//
// Example:
//
//	result, err := synthfs.Run(ctx, fs, ops...)
//	if err == nil {
//	    for _, opResult := range result.GetOperations() {
//	        if op, ok := opResult.(OperationResult); ok {
//	            stdout := GetOperationOutput(op.Operation, "stdout")
//	            if stdout != "" {
//	                fmt.Printf("Command output: %s\n", stdout)
//	            }
//	        }
//	    }
//	}
func GetOperationOutput(op Operation, key string) string {
	desc := op.Describe()
	if desc.Details != nil {
		if val, ok := desc.Details[key]; ok {
			if str, ok := val.(string); ok {
				return str
			}
		}
	}
	return ""
}

// GetOperationOutputValue retrieves stored output as an interface{} value.
// This is useful when the stored value is not a string.
func GetOperationOutputValue(op Operation, key string) interface{} {
	desc := op.Describe()
	if desc.Details != nil {
		return desc.Details[key]
	}
	return nil
}

// GetAllOperationOutputs retrieves all stored outputs from an operation.
func GetAllOperationOutputs(op Operation) map[string]interface{} {
	desc := op.Describe()
	if desc.Details != nil {
		// Return a copy to prevent modification
		result := make(map[string]interface{})
		for k, v := range desc.Details {
			result[k] = v
		}
		return result
	}
	return make(map[string]interface{})
}
