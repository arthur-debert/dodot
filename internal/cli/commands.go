package cli

import (
	"fmt"
	"os"
	"path/filepath"

	"github.com/arthur-debert/dodot/internal/version"
	"github.com/arthur-debert/dodot/pkg/cobrax/topics"
	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/paths"
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
		CompletionOptions: cobra.CompletionOptions{
			DisableDefaultCmd: true,
		},
		DisableAutoGenTag: true,
	}

	// Global flags
	rootCmd.PersistentFlags().CountVarP(&verbosity, "verbose", "v", "Increase verbosity (-v INFO, -vv DEBUG, -vvv TRACE)")
	rootCmd.PersistentFlags().BoolVar(&dryRun, "dry-run", false, "Preview changes without executing them")
	rootCmd.PersistentFlags().BoolVar(&force, "force", false, "Force execution of run-once power-ups even if already executed")

	// Disable automatic help command (we'll use our custom one from topics)
	rootCmd.SetHelpCommand(&cobra.Command{Hidden: true})

	// Add all commands
	rootCmd.AddCommand(newVersionCmd())
	rootCmd.AddCommand(newDeployCmd())
	rootCmd.AddCommand(newInstallCmd())
	rootCmd.AddCommand(newListCmd())
	rootCmd.AddCommand(newStatusCmd())
	rootCmd.AddCommand(newInitCmd())
	rootCmd.AddCommand(newFillCmd())

	// Initialize topic-based help system
	// Try to find help topics relative to the executable location
	exe, err := os.Executable()
	if err == nil {
		// Look for help topics in various locations
		possiblePaths := []string{
			filepath.Join(filepath.Dir(exe), "..", "..", "docs", "help"), // Development
			filepath.Join(filepath.Dir(exe), "docs", "help"),             // Installed
			"docs/help", // Current directory
		}

		for _, helpPath := range possiblePaths {
			if _, err := os.Stat(helpPath); err == nil {
				// Initialize topics without logging (logging not set up yet)
				if err := topics.Initialize(rootCmd, helpPath); err == nil {
					break
				}
			}
		}
	}

	return rootCmd
}

// initPaths initializes the paths instance and shows a warning if using fallback
func initPaths() (*paths.Paths, error) {
	p, err := paths.New("")
	if err != nil {
		return nil, fmt.Errorf("failed to initialize paths: %w", err)
	}

	if p.UsedFallback() {
		fmt.Fprintf(os.Stderr, "Warning: Not in a git repository and DOTFILES_ROOT not set.\n")
		fmt.Fprintf(os.Stderr, "Using current directory: %s\n", p.DotfilesRoot())
		fmt.Fprintf(os.Stderr, "For better results, either:\n")
		fmt.Fprintf(os.Stderr, "  - Run from within a git repository containing your dotfiles\n")
		fmt.Fprintf(os.Stderr, "  - Set DOTFILES_ROOT environment variable\n\n")
	} else {
		// Debug: log how we found the path
		if os.Getenv("DODOT_DEBUG") != "" {
			fmt.Fprintf(os.Stderr, "Debug: Using dotfiles root: %s (fallback=%v)\n", p.DotfilesRoot(), p.UsedFallback())
		}
	}

	return p, nil
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
		RunE: func(cmd *cobra.Command, args []string) error {
			// Initialize paths (will show warning if using fallback)
			p, err := initPaths()
			if err != nil {
				return err
			}

			log.Info().Str("dotfiles_root", p.DotfilesRoot()).Msg("Deploying from dotfiles root")

			fmt.Printf("Deploy command would run with dotfiles root: %s\n", p.DotfilesRoot())
			if len(args) > 0 {
				fmt.Printf("Deploying packs: %v\n", args)
			} else {
				fmt.Println("Deploying all packs")
			}

			return nil
		},
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
		RunE: func(cmd *cobra.Command, args []string) error {
			// Initialize paths (will show warning if using fallback)
			p, err := initPaths()
			if err != nil {
				return err
			}

			log.Info().Str("dotfiles_root", p.DotfilesRoot()).Msg("Installing from dotfiles root")

			// TODO: Implement actual install logic
			fmt.Printf("Install command would run with dotfiles root: %s\n", p.DotfilesRoot())
			if len(args) > 0 {
				fmt.Printf("Installing packs: %v\n", args)
			} else {
				fmt.Println("Installing all packs")
			}

			return nil
		},
	}
}

func newListCmd() *cobra.Command {
	return &cobra.Command{
		Use:   "list",
		Short: "List all available packs",
		Long:  `List displays all packs found in your DOTFILES_ROOT directory.`,
		Example: `  # List all packs
  dodot list`,
		RunE: func(cmd *cobra.Command, args []string) error {
			// Initialize paths (will show warning if using fallback)
			p, err := initPaths()
			if err != nil {
				return err
			}

			log.Info().Str("dotfiles_root", p.DotfilesRoot()).Msg("Listing packs from dotfiles root")

			// TODO: Implement actual list logic
			fmt.Printf("Listing packs from: %s\n", p.DotfilesRoot())

			return nil
		},
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
		RunE: func(cmd *cobra.Command, args []string) error {
			// Initialize paths (will show warning if using fallback)
			p, err := initPaths()
			if err != nil {
				return err
			}

			log.Info().Str("dotfiles_root", p.DotfilesRoot()).Msg("Checking status from dotfiles root")

			// TODO: Implement actual status logic
			fmt.Printf("Status command would run with dotfiles root: %s\n", p.DotfilesRoot())
			if len(args) > 0 {
				fmt.Printf("Checking status for packs: %v\n", args)
			} else {
				fmt.Println("Checking status for all packs")
			}

			return nil
		},
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
		RunE: func(cmd *cobra.Command, args []string) error {
			// Initialize paths (will show warning if using fallback)
			p, err := initPaths()
			if err != nil {
				return err
			}

			packName := args[0]
			packType, _ := cmd.Flags().GetString("type")

			log.Info().
				Str("dotfiles_root", p.DotfilesRoot()).
				Str("pack", packName).
				Str("type", packType).
				Msg("Creating new pack")

			// TODO: Implement actual init logic
			fmt.Printf("Would create pack '%s' of type '%s' in: %s\n", packName, packType, p.DotfilesRoot())

			return nil
		},
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
		RunE: func(cmd *cobra.Command, args []string) error {
			// Initialize paths (will show warning if using fallback)
			p, err := initPaths()
			if err != nil {
				return err
			}

			packName := args[0]

			log.Info().
				Str("dotfiles_root", p.DotfilesRoot()).
				Str("pack", packName).
				Msg("Filling pack with placeholder files")

			// TODO: Implement actual fill logic
			fmt.Printf("Would fill pack '%s' in: %s\n", packName, p.DotfilesRoot())

			return nil
		},
	}
}
