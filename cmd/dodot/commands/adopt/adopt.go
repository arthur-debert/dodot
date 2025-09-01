package adopt

import (
	"github.com/spf13/cobra"
)

// NewCommand creates the adopt command
// The command logic is kept in the main commands file for now to avoid circular dependencies
func NewCommand() *cobra.Command {
	// This will be filled in by the root command
	return &cobra.Command{
		Use:     "adopt <pack> <source-path> [<source-path>...]",
		Short:   MsgShort,
		Long:    MsgLong,
		Args:    cobra.MinimumNArgs(2),
		Example: MsgExample,
		GroupID: "single-pack",
	}
}
