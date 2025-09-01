package off

import (
	"github.com/spf13/cobra"
)

// NewCommand creates the off command
// The command logic is kept in the main commands file for now to avoid circular dependencies
func NewCommand() *cobra.Command {
	// This will be filled in by the root command
	return &cobra.Command{
		Use:     "off [packs...]",
		Short:   MsgShort,
		Long:    MsgLong,
		Example: MsgExample,
		GroupID: "core",
	}
}
