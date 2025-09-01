package addignore

import (
	"github.com/spf13/cobra"
)

// NewCommand creates the add-ignore command
// The command logic is kept in the main commands file for now to avoid circular dependencies
func NewCommand() *cobra.Command {
	// This will be filled in by the root command
	return &cobra.Command{
		Use:     "add-ignore <pack-name>",
		Short:   MsgShort,
		Long:    MsgLong,
		Args:    cobra.ExactArgs(1),
		Example: MsgExample,
		GroupID: "single-pack",
	}
}
