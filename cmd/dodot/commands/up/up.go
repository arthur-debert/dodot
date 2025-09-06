package up

import (
	"github.com/spf13/cobra"
)

// NewCommand creates the up command
func NewCommand() *cobra.Command {
	cmd := &cobra.Command{
		Use:     "up [packs...]",
		Short:   MsgShort,
		Long:    MsgLong,
		Example: MsgExample,
		GroupID: "core",
	}

	// Add command-specific flags
	cmd.Flags().Bool("no-provision", false, "Skip provisioning handlers (only link files)")
	cmd.Flags().Bool("provision-rerun", false, "Force re-run provisioning even if already done")

	// Mark mutually exclusive flags
	cmd.MarkFlagsMutuallyExclusive("no-provision", "provision-rerun")

	return cmd
}
