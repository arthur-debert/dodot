package topics

import (
	"github.com/spf13/cobra"
)

// NewCommand creates the topics command
// The command logic is kept in the main commands file for now to avoid circular dependencies
func NewCommand() *cobra.Command {
	// This will be filled in by the root command
	return &cobra.Command{
		Use:   "topics",
		Short: MsgShort,
		Long:  MsgLong,
	}
}
