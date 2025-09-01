package init

import (
	"github.com/spf13/cobra"
)

// NewCommand creates the init command
// The command logic is kept in the main commands file for now to avoid circular dependencies
func NewCommand() *cobra.Command {
	// This will be filled in by the root command
	cmd := &cobra.Command{
		Use:     "init <pack-name>",
		Short:   MsgShort,
		Long:    MsgLong,
		Args:    cobra.ExactArgs(1),
		Example: MsgExample,
		GroupID: "single-pack",
	}

	cmd.Flags().StringP("type", "t", "basic", MsgFlagType)

	return cmd
}
