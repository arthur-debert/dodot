package genconfig

import (
	"github.com/spf13/cobra"
)

// NewCommand creates the gen-config command
// The command logic is kept in the main commands file for now to avoid circular dependencies
func NewCommand() *cobra.Command {
	// This will be filled in by the root command
	cmd := &cobra.Command{
		Use:     "gen-config [<pack>...]",
		Short:   MsgShort,
		Long:    MsgLong,
		Example: MsgExample,
		GroupID: "config",
	}

	cmd.Flags().BoolP("write", "w", false, "Write config to file(s) instead of stdout")

	return cmd
}
