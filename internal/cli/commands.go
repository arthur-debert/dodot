package cli

import (
	"github.com/arthur-debert/dodot/internal/version"
	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/rs/zerolog/log"
	"github.com/spf13/cobra"
)

// NewRootCmd creates and returns the root command
func NewRootCmd() *cobra.Command {
	var (
		verbosity int
		dryRun    bool
		force     bool
	)

	rootCmd := &cobra.Command{
		Use:   "dodot",
		Short: "A stateless dotfiles manager",
		Long: `dodot is a stateless dotfiles manager that helps you organize and deploy
your configuration files in a structured, safe way while letting git handle
versioning and history.`,
		Version: version.Version,
		PersistentPreRun: func(cmd *cobra.Command, args []string) {
			// Setup logging based on verbosity
			logging.SetupLogger(verbosity)
			log.Debug().Str("command", cmd.Name()).Msg("Command started")
		},
		SilenceUsage:  true,
		SilenceErrors: true,
	}

	// Global flags
	rootCmd.PersistentFlags().CountVarP(&verbosity, "verbose", "v", "Increase verbosity (-v INFO, -vv DEBUG, -vvv TRACE)")
	rootCmd.PersistentFlags().BoolVar(&dryRun, "dry-run", false, "Preview changes without executing them")
	rootCmd.PersistentFlags().BoolVar(&force, "force", false, "Force execution of run-once power-ups even if already executed")

	// Add all commands
	rootCmd.AddCommand(newVersionCmd())
	rootCmd.AddCommand(newDeployCmd())
	rootCmd.AddCommand(newInstallCmd())
	rootCmd.AddCommand(newListCmd())
	rootCmd.AddCommand(newStatusCmd())
	rootCmd.AddCommand(newInitCmd())
	rootCmd.AddCommand(newFillCmd())

	return rootCmd
}

func newVersionCmd() *cobra.Command {
	return &cobra.Command{
		Use:   "version",
		Short: "Print version information",
		Long:  `Print detailed version information including commit hash and build date`,
	}
}

func newDeployCmd() *cobra.Command {
	return &cobra.Command{
		Use:   "deploy [packs...]",
		Short: "Deploy dotfiles to the system",
		Long: `Deploy processes all packs in your dotfiles directory and creates
the necessary symlinks, installs packages, and performs other configured actions.

If no packs are specified, all packs in the DOTFILES_ROOT will be deployed.`,
		Example: `  # Deploy all packs
  dodot deploy
  
  # Deploy specific packs
  dodot deploy vim zsh
  
  # Dry run to preview changes
  dodot deploy --dry-run`,
	}
}

func newInstallCmd() *cobra.Command {
	return &cobra.Command{
		Use:   "install [packs...]",
		Short: "Install and deploy dotfiles to the system",
		Long: `Install is an alias for deploy. It processes all packs in your dotfiles
directory and creates the necessary symlinks, installs packages, and performs
other configured actions.`,
		Example: `  # Install all packs
  dodot install
  
  # Install specific packs
  dodot install vim zsh`,
	}
}

func newListCmd() *cobra.Command {
	return &cobra.Command{
		Use:   "list",
		Short: "List all available packs",
		Long:  `List displays all packs found in your DOTFILES_ROOT directory.`,
		Example: `  # List all packs
  dodot list`,
	}
}

func newStatusCmd() *cobra.Command {
	return &cobra.Command{
		Use:   "status [packs...]",
		Short: "Show deployment status of packs",
		Long: `Status shows the current deployment state of packs, including
which files are deployed, which power-ups have been executed, and any
pending changes.

If no packs are specified, status for all packs will be shown.`,
		Example: `  # Show status for all packs
  dodot status
  
  # Show status for specific packs
  dodot status vim zsh`,
	}
}

func newInitCmd() *cobra.Command {
	cmd := &cobra.Command{
		Use:   "init <pack-name>",
		Short: "Create a new pack with template files",
		Long: `Init creates a new pack directory with template configuration files.
This helps you get started with a new pack quickly.

The pack will be created in your DOTFILES_ROOT directory with a basic
triggers.toml file and appropriate directory structure.`,
		Args: cobra.ExactArgs(1),
		Example: `  # Create a basic pack
  dodot init mypack
  
  # Create a specific type of pack
  dodot init --type shell myshell`,
	}

	cmd.Flags().StringP("type", "t", "basic", "Type of pack to create (basic, shell, vim, etc.)")

	return cmd
}

func newFillCmd() *cobra.Command {
	return &cobra.Command{
		Use:   "fill <pack-name>",
		Short: "Add placeholder files to an existing pack",
		Long: `Fill analyzes an existing pack's triggers and creates placeholder files
for any patterns that don't match existing files.

This is useful when you want to see what files a pack expects before
actually creating them.`,
		Args: cobra.ExactArgs(1),
		Example: `  # Create placeholder files for a pack
  dodot fill mypack`,
	}
}