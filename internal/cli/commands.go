package cli

import (
	"fmt"
	"os"
	"path/filepath"

	"github.com/arthur-debert/dodot/internal/version"
	"github.com/arthur-debert/dodot/pkg/cobrax/topics"
	"github.com/arthur-debert/dodot/pkg/core"
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
		Run: func(cmd *cobra.Command, args []string) {
			fmt.Printf("dodot version %s\n", version.Version)
			if version.Commit != "" {
				fmt.Printf("Commit: %s\n", version.Commit)
			}
			if version.Date != "" {
				fmt.Printf("Built:  %s\n", version.Date)
			}
		},
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

			// Get dry-run flag value (it's a persistent flag)
			dryRun, _ := cmd.Root().PersistentFlags().GetBool("dry-run")

			log.Info().
				Str("dotfiles_root", p.DotfilesRoot()).
				Bool("dry_run", dryRun).
				Msg("Deploying from dotfiles root")

			// Use the actual DeployPacks implementation
			result, err := core.DeployPacks(core.DeployPacksOptions{
				DotfilesRoot: p.DotfilesRoot(),
				PackNames:    args,
				DryRun:       dryRun,
			})
			if err != nil {
				return fmt.Errorf("failed to deploy packs: %w", err)
			}

			// Display results
			if dryRun {
				fmt.Println("\nDRY RUN MODE - No changes were made")
			}

			if len(result.Operations) == 0 {
				fmt.Println("No operations needed.")
			} else {
				fmt.Printf("\nPerformed %d operations:\n", len(result.Operations))
				for _, op := range result.Operations {
					fmt.Printf("  ✓ %s\n", op.Description)
				}
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

			// Get flags (they're persistent flags)
			dryRun, _ := cmd.Root().PersistentFlags().GetBool("dry-run")
			force, _ := cmd.Root().PersistentFlags().GetBool("force")

			log.Info().
				Str("dotfiles_root", p.DotfilesRoot()).
				Bool("dry_run", dryRun).
				Bool("force", force).
				Msg("Installing from dotfiles root")

			// Use the actual InstallPacks implementation
			result, err := core.InstallPacks(core.InstallPacksOptions{
				DotfilesRoot: p.DotfilesRoot(),
				PackNames:    args,
				DryRun:       dryRun,
				Force:        force,
			})
			if err != nil {
				return fmt.Errorf("failed to install packs: %w", err)
			}

			// Display results
			if dryRun {
				fmt.Println("\nDRY RUN MODE - No changes were made")
			}

			if len(result.Operations) == 0 {
				fmt.Println("No operations needed.")
			} else {
				fmt.Printf("\nPerformed %d operations:\n", len(result.Operations))
				for _, op := range result.Operations {
					fmt.Printf("  ✓ %s\n", op.Description)
				}
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

			// Use the actual ListPacks implementation
			result, err := core.ListPacks(core.ListPacksOptions{
				DotfilesRoot: p.DotfilesRoot(),
			})
			if err != nil {
				return fmt.Errorf("failed to list packs: %w", err)
			}

			// Display the packs
			if len(result.Packs) == 0 {
				fmt.Println("No packs found.")
			} else {
				fmt.Println("Available packs:")
				for _, pack := range result.Packs {
					fmt.Printf("  %s\n", pack.Name)
				}
			}

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

			// Use the actual StatusPacks implementation
			result, err := core.StatusPacks(core.StatusPacksOptions{
				DotfilesRoot: p.DotfilesRoot(),
				PackNames:    args,
			})
			if err != nil {
				return fmt.Errorf("failed to get pack status: %w", err)
			}

			// Display status for each pack
			for _, packStatus := range result.Packs {
				fmt.Printf("\n%s:\n", packStatus.Name)

				// Show power-up statuses
				for _, ps := range packStatus.PowerUpState {
					fmt.Printf("  %s: %s", ps.Name, ps.State)
					if ps.Description != "" {
						fmt.Printf(" - %s", ps.Description)
					}
					fmt.Println()
				}
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

			log.Info().
				Str("dotfiles_root", p.DotfilesRoot()).
				Str("pack", packName).
				Msg("Creating new pack")

			// Use the actual InitPack implementation
			result, err := core.InitPack(core.InitPackOptions{
				DotfilesRoot: p.DotfilesRoot(),
				PackName:     packName,
			})
			if err != nil {
				return fmt.Errorf("failed to initialize pack: %w", err)
			}

			// Display results
			fmt.Printf("Created pack '%s' with the following files:\n", packName)
			for _, file := range result.FilesCreated {
				fmt.Printf("  ✓ %s\n", file)
			}

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

			// Use the actual FillPack implementation
			result, err := core.FillPack(core.FillPackOptions{
				DotfilesRoot: p.DotfilesRoot(),
				PackName:     packName,
			})
			if err != nil {
				return fmt.Errorf("failed to fill pack: %w", err)
			}

			// Display results
			if len(result.FilesCreated) == 0 {
				fmt.Printf("Pack '%s' already has all standard files.\n", packName)
			} else {
				fmt.Printf("Added the following files to pack '%s':\n", packName)
				for _, file := range result.FilesCreated {
					fmt.Printf("  ✓ %s\n", file)
				}
			}

			return nil
		},
	}
}
