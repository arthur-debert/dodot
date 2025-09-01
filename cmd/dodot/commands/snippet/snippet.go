package snippet

import (
	"github.com/spf13/cobra"
)

// NewCommand creates the snippet command
// The command logic is kept in the main commands file for now to avoid circular dependencies
func NewCommand() *cobra.Command {
	// This will be filled in by the root command
	cmd := &cobra.Command{
		Use:     "snippet",
		Short:   MsgShort,
		Long:    MsgLong,
		Example: MsgExample,
		GroupID: "config",
	}

	cmd.Flags().StringP("shell", "s", "bash", "Shell type (bash, zsh, fish)")
	cmd.Flags().Bool("provision", false, "Install shell integration scripts to data directory")

	return cmd
}
