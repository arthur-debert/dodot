package fill

import (
	"github.com/spf13/cobra"
)

// NewCommand creates the fill command
// The command logic is kept in the main commands file for now to avoid circular dependencies
func NewCommand() *cobra.Command {
	// This will be filled in by the root command
	return &cobra.Command{
		Use:     "fill <pack-name>",
		Short:   MsgShort,
		Long:    MsgLong,
		Args:    cobra.ExactArgs(1),
		Example: MsgExample,
		GroupID: "single-pack",
	}
}
