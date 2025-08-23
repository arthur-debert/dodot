package types

// RunMode indicates how often a handler should be executed
type RunMode string

const (
	// RunModeLinking indicates the handler creates/updates links and can be run multiple times safely
	RunModeLinking RunMode = "linking"

	// RunModeProvisioning indicates the handler provisions resources and should only run once per pack
	RunModeProvisioning RunMode = "provisioning"
)

const (
	// OverridePriority is a high priority value for config overrides
	OverridePriority = 100
)
