package dodot

import (
	"errors"
	"fmt"
	"os"
	"path/filepath"

	"github.com/arthur-debert/dodot/internal/version"
	"github.com/arthur-debert/dodot/pkg/cobrax/topics"
	"github.com/arthur-debert/dodot/pkg/commands"
	doerrors "github.com/arthur-debert/dodot/pkg/errors"
	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/output"
	"github.com/arthur-debert/dodot/pkg/paths"
	shellpkg "github.com/arthur-debert/dodot/pkg/shell"
	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/rs/zerolog/log"
	"github.com/spf13/cobra"
)

// NewRootCmd creates and returns the root command
func NewRootCmd() *cobra.Command {
	// Initialize custom template formatting functions
	initTemplateFormatting()

	var (
		verbosity  int
		dryRun     bool
		force      bool
		configFile string
	)

	rootCmd := &cobra.Command{
		Use:     "dodot",
		Short:   MsgRootShort,
		Long:    MsgRootLong,
		Version: version.Version,
		PersistentPreRun: func(cmd *cobra.Command, args []string) {
			// Setup logging based on verbosity
			logging.SetupLogger(verbosity)
			log.Debug().Str("command", cmd.Name()).Msg("Command started")

			// Load custom styles if specified
			if configFile != "" {
				if err := output.LoadStylesFromFile(configFile); err != nil {
					log.Error().Err(err).Str("config", configFile).Msg("Failed to load custom styles")
					_, _ = fmt.Fprintf(cmd.ErrOrStderr(), "Warning: Failed to load custom styles from %s: %v\n", configFile, err)
				} else {
					log.Info().Str("config", configFile).Msg("Loaded custom styles")
				}
			}
		},
		RunE: func(cmd *cobra.Command, args []string) error {
			// If we get here, no subcommand was provided
			// Show help but return an error to indicate incorrect usage
			_ = cmd.Help()
			return fmt.Errorf("no command specified")
		},
		SilenceUsage:      true,
		SilenceErrors:     true,
		DisableAutoGenTag: true,
	}

	// Global flags
	rootCmd.PersistentFlags().CountVarP(&verbosity, "verbose", "v", MsgFlagVerbose)
	rootCmd.PersistentFlags().BoolVar(&dryRun, "dry-run", false, MsgFlagDryRun)
	rootCmd.PersistentFlags().BoolVar(&force, "force", false, MsgFlagForce)
	rootCmd.PersistentFlags().StringVar(&configFile, "config", "", "Path to custom styles configuration file")

	// Disable automatic help command (we'll use our custom one from topics)
	rootCmd.SetHelpCommand(&cobra.Command{Hidden: true})

	// Define command groups
	rootCmd.AddGroup(&cobra.Group{
		ID:    "core",
		Title: "COMMANDS:",
	})
	rootCmd.AddGroup(&cobra.Group{
		ID:    "misc",
		Title: "MISC:",
	})

	// Set custom help template
	rootCmd.SetUsageTemplate(MsgUsageTemplate)

	// Add all commands
	rootCmd.AddCommand(newDeployCmd())
	rootCmd.AddCommand(newInstallCmd())
	rootCmd.AddCommand(newListCmd())
	rootCmd.AddCommand(newStatusCmd())
	rootCmd.AddCommand(newInitCmd())
	rootCmd.AddCommand(newFillCmd())
	rootCmd.AddCommand(newAddIgnoreCmd())
	rootCmd.AddCommand(newSnippetCmd())
	rootCmd.AddCommand(newTopicsCmd())
	// Completion command removed - use dodot-completions tool instead

	// Initialize topic-based help system
	// Try to find help topics relative to the executable location
	exe, err := os.Executable()
	if err == nil {
		// Look for help topics in various locations
		possiblePaths := []string{
			filepath.Join(filepath.Dir(exe), "topics"),                             // Same directory as binary (production)
			filepath.Join(filepath.Dir(exe), "..", "..", "cmd", "dodot", "topics"), // Development
			"cmd/dodot/topics", // Current directory fallback
		}

		for _, helpPath := range possiblePaths {
			if _, err := os.Stat(helpPath); err == nil {
				// Initialize topics with .txt, .md, and .txxt extensions
				opts := topics.Options{
					Extensions: []string{".txt", ".md", ".txxt"},
					// Always use Glamour renderer for markdown files
					Renderer: topics.NewGlamourRenderer(),
				}

				if err := topics.InitializeWithOptions(rootCmd, helpPath, opts); err == nil {
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
		return nil, fmt.Errorf(MsgErrInitPaths, err)
	}

	if p.UsedFallback() {
		fmt.Fprintf(os.Stderr, MsgFallbackWarning, p.DotfilesRoot())
	} else {
		// Debug: log how we found the path
		if os.Getenv("DODOT_DEBUG") != "" {
			fmt.Fprintf(os.Stderr, MsgDebugDotfilesRoot, p.DotfilesRoot(), p.UsedFallback())
		}
	}

	return p, nil
}

// handlePackNotFoundError provides detailed error information when packs are not found
func handlePackNotFoundError(dodotErr *doerrors.DodotError, p types.Pather, operation string) error {
	// Display detailed error information
	fmt.Fprintf(os.Stderr, "\nError: Pack(s) not found\n\n")
	fmt.Fprintf(os.Stderr, "Searching for your dotfiles root:\n")

	// Show the search process
	if envRoot := os.Getenv("DOTFILES_ROOT"); envRoot != "" {
		fmt.Fprintf(os.Stderr, "  1. $DOTFILES_ROOT is set to: %s\n", envRoot)
	} else {
		fmt.Fprintf(os.Stderr, "  1. $DOTFILES_ROOT not set: searching for dotfiles repo\n")

		if source, ok := dodotErr.Details["source"].(string); ok {
			switch source {
			case "git repository root":
				fmt.Fprintf(os.Stderr, "  2. Found git repository root: %s\n", p.DotfilesRoot())
			case "current working directory (fallback)":
				fmt.Fprintf(os.Stderr, "  2. No git repo found: using current directory\n")
				fmt.Fprintf(os.Stderr, "  3. Using: %s\n", p.DotfilesRoot())
			}
		}
	}

	fmt.Fprintf(os.Stderr, "\n")

	// Show what we were looking for
	if notFound, ok := dodotErr.Details["notFound"].([]string); ok && len(notFound) > 0 {
		fmt.Fprintf(os.Stderr, "Looking for pack(s): %v\n", notFound)
		fmt.Fprintf(os.Stderr, "No directory named \"%s\" in %s\n\n", notFound[0], p.DotfilesRoot())
	}

	// Show available packs if any
	if available, ok := dodotErr.Details["available"].([]string); ok {
		if len(available) == 0 {
			fmt.Fprintf(os.Stderr, "No packs found in %s\n", p.DotfilesRoot())
			fmt.Fprintf(os.Stderr, "This might mean:\n")
			fmt.Fprintf(os.Stderr, "  - The directory has no subdirectories\n")
			fmt.Fprintf(os.Stderr, "  - All subdirectories have .dodotignore files\n")
			fmt.Fprintf(os.Stderr, "  - All subdirectories are empty\n")
		} else {
			fmt.Fprintf(os.Stderr, "Available packs in %s:\n", p.DotfilesRoot())
			for _, pack := range available {
				fmt.Fprintf(os.Stderr, "  - %s\n", pack)
			}
		}
	}

	return fmt.Errorf("%s failed", operation)
}

// packNamesCompletion provides shell completion for pack names
func packNamesCompletion(cmd *cobra.Command, args []string, toComplete string) ([]string, cobra.ShellCompDirective) {
	// Initialize paths
	p, err := initPaths()
	if err != nil {
		return nil, cobra.ShellCompDirectiveError
	}

	// Get list of packs
	result, err := commands.ListPacks(commands.ListPacksOptions{
		DotfilesRoot: p.DotfilesRoot(),
	})
	if err != nil {
		return nil, cobra.ShellCompDirectiveError
	}

	// Extract pack names
	var packNames []string
	for _, pack := range result.Packs {
		packNames = append(packNames, pack.Name)
	}

	// Filter out already specified packs
	var availablePacks []string
	for _, pack := range packNames {
		found := false
		for _, arg := range args {
			if arg == pack {
				found = true
				break
			}
		}
		if !found {
			availablePacks = append(availablePacks, pack)
		}
	}

	return availablePacks, cobra.ShellCompDirectiveNoFileComp
}

func newDeployCmd() *cobra.Command {
	return &cobra.Command{
		Use:               "deploy [packs...]",
		Short:             MsgDeployShort,
		Long:              MsgDeployLong,
		Example:           MsgDeployExample,
		GroupID:           "core",
		ValidArgsFunction: packNamesCompletion,
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

			// Deploy packs using the new implementation
			ctx, err := commands.DeployPacks(commands.DeployPacksOptions{
				DotfilesRoot:       p.DotfilesRoot(),
				PackNames:          args,
				DryRun:             dryRun,
				EnableHomeSymlinks: true,
			})
			if err != nil {
				// Check if this is a pack not found error and provide detailed help
				var dodotErr *doerrors.DodotError
				if errors.As(err, &dodotErr) && dodotErr.Code == doerrors.ErrPackNotFound {
					return handlePackNotFoundError(dodotErr, p, "deployment")
				}
				return fmt.Errorf(MsgErrDeployPacks, err)
			}

			// Display results using the new output renderer
			// The renderer will automatically detect NO_COLOR environment variable
			// through lipgloss/termenv
			renderer, err := output.NewRenderer(os.Stdout, false)
			if err != nil {
				return fmt.Errorf("failed to create renderer: %w", err)
			}
			if err := renderer.RenderExecutionContext(ctx); err != nil {
				return fmt.Errorf("failed to render results: %w", err)
			}

			return nil
		},
	}
}

func newInstallCmd() *cobra.Command {
	return &cobra.Command{
		Use:               "install [packs...]",
		Short:             MsgInstallShort,
		Long:              MsgInstallLong,
		Example:           MsgInstallExample,
		GroupID:           "core",
		ValidArgsFunction: packNamesCompletion,
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

			// Install packs using the new implementation
			ctx, err := commands.InstallPacks(commands.InstallPacksOptions{
				DotfilesRoot:       p.DotfilesRoot(),
				PackNames:          args,
				DryRun:             dryRun,
				Force:              force,
				EnableHomeSymlinks: true,
			})
			if err != nil {
				// Check if this is a pack not found error and provide detailed help
				var dodotErr *doerrors.DodotError
				if errors.As(err, &dodotErr) && dodotErr.Code == doerrors.ErrPackNotFound {
					return handlePackNotFoundError(dodotErr, p, "installation")
				}
				return fmt.Errorf(MsgErrInstallPacks, err)
			}

			// Display results using the new output renderer
			// The renderer will automatically detect NO_COLOR environment variable
			// through lipgloss/termenv
			renderer, err := output.NewRenderer(os.Stdout, false)
			if err != nil {
				return fmt.Errorf("failed to create renderer: %w", err)
			}
			if err := renderer.RenderExecutionContext(ctx); err != nil {
				return fmt.Errorf("failed to render results: %w", err)
			}

			return nil
		},
	}
}

func newListCmd() *cobra.Command {
	return &cobra.Command{
		Use:     "list",
		Short:   MsgListShort,
		Long:    MsgListLong,
		Example: MsgListExample,
		GroupID: "core",
		RunE: func(cmd *cobra.Command, args []string) error {
			// Initialize paths (will show warning if using fallback)
			p, err := initPaths()
			if err != nil {
				return err
			}

			log.Info().Str("dotfiles_root", p.DotfilesRoot()).Msg("Listing packs from dotfiles root")

			// Use the actual ListPacks implementation
			result, err := commands.ListPacks(commands.ListPacksOptions{
				DotfilesRoot: p.DotfilesRoot(),
			})
			if err != nil {
				return fmt.Errorf(MsgErrListPacks, err)
			}

			// Display the packs in a simple format
			if len(result.Packs) == 0 {
				fmt.Println("No packs found")
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
		Use:               "status [packs...]",
		Short:             MsgStatusShort,
		Long:              MsgStatusLong,
		Example:           MsgStatusExample,
		GroupID:           "core",
		ValidArgsFunction: packNamesCompletion,
		RunE: func(cmd *cobra.Command, args []string) error {
			// Initialize paths (will show warning if using fallback)
			p, err := initPaths()
			if err != nil {
				return err
			}

			log.Info().
				Str("dotfiles_root", p.DotfilesRoot()).
				Strs("packs", args).
				Msg("Checking pack status")

			// Run status command
			result, err := commands.StatusPacks(commands.StatusPacksOptions{
				DotfilesRoot: p.DotfilesRoot(),
				PackNames:    args,
				Paths:        p,
			})
			if err != nil {
				// Check if this is a pack not found error and provide detailed help
				var dodotErr *doerrors.DodotError
				if errors.As(err, &dodotErr) && dodotErr.Code == doerrors.ErrPackNotFound {
					return handlePackNotFoundError(dodotErr, p, "status check")
				}
				return fmt.Errorf(MsgErrStatusPacks, err)
			}

			// Display results using the new output renderer
			// The renderer will automatically detect NO_COLOR environment variable
			// through lipgloss/termenv
			renderer, err := output.NewRenderer(os.Stdout, false)
			if err != nil {
				return fmt.Errorf("failed to create renderer: %w", err)
			}
			if err := renderer.Render(result); err != nil {
				return fmt.Errorf("failed to display status: %w", err)
			}

			return nil
		},
	}
}

func newInitCmd() *cobra.Command {
	cmd := &cobra.Command{
		Use:     "init <pack-name>",
		Short:   MsgInitShort,
		Long:    MsgInitLong,
		Args:    cobra.ExactArgs(1),
		Example: MsgInitExample,
		GroupID: "core",
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
			result, err := commands.InitPack(commands.InitPackOptions{
				DotfilesRoot: p.DotfilesRoot(),
				PackName:     packName,
			})
			if err != nil {
				return fmt.Errorf(MsgErrInitPack, err)
			}

			// Operations are already executed by the command
			// No need to execute them again

			// Display results
			fmt.Printf(MsgPackCreatedFormat, packName)
			for _, file := range result.FilesCreated {
				fmt.Printf(MsgOperationItem, file)
			}

			return nil
		},
	}

	cmd.Flags().StringP("type", "t", "basic", MsgFlagType)

	return cmd
}

func newFillCmd() *cobra.Command {
	return &cobra.Command{
		Use:     "fill <pack-name>",
		Short:   MsgFillShort,
		Long:    MsgFillLong,
		Args:    cobra.ExactArgs(1),
		Example: MsgFillExample,
		GroupID: "core",
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
			result, err := commands.FillPack(commands.FillPackOptions{
				DotfilesRoot: p.DotfilesRoot(),
				PackName:     packName,
			})
			if err != nil {
				return fmt.Errorf(MsgErrFillPack, err)
			}

			// Operations are already executed by the command
			// No need to execute them again

			// Display results
			if len(result.FilesCreated) == 0 {
				fmt.Printf(MsgPackHasAllFiles, packName)
			} else {
				fmt.Printf(MsgPackFilledFormat, packName)
				for _, file := range result.FilesCreated {
					fmt.Printf(MsgOperationItem, file)
				}
			}

			return nil
		},
	}
}

func newAddIgnoreCmd() *cobra.Command {
	return &cobra.Command{
		Use:               "add-ignore <pack-name>",
		Short:             MsgAddIgnoreShort,
		Long:              MsgAddIgnoreLong,
		Args:              cobra.ExactArgs(1),
		Example:           MsgAddIgnoreExample,
		GroupID:           "core",
		ValidArgsFunction: packNamesCompletion,
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
				Msg("Adding ignore file to pack")

			// Use the actual AddIgnore implementation
			result, err := commands.AddIgnore(commands.AddIgnoreOptions{
				DotfilesRoot: p.DotfilesRoot(),
				PackName:     packName,
			})
			if err != nil {
				// Check if this is a pack not found error and provide detailed help
				var dodotErr *doerrors.DodotError
				if errors.As(err, &dodotErr) && dodotErr.Code == doerrors.ErrPackNotFound {
					return handlePackNotFoundError(dodotErr, p, "add-ignore")
				}
				return fmt.Errorf(MsgErrAddIgnore, err)
			}

			// Display results
			if result.AlreadyExisted {
				fmt.Printf(MsgIgnoreFileExists, packName)
			} else {
				fmt.Printf(MsgIgnoreFileCreated, packName)
			}

			return nil
		},
	}
}

func newTopicsCmd() *cobra.Command {
	return &cobra.Command{
		Use:     "topics",
		Short:   MsgTopicsShort,
		Long:    MsgTopicsLong,
		GroupID: "misc",
		RunE: func(cmd *cobra.Command, args []string) error {
			// Find the help command and execute it with "topics" argument
			if helpCmd, _, err := cmd.Root().Find([]string{"help"}); err == nil {
				if helpCmd.RunE != nil {
					return helpCmd.RunE(helpCmd, []string{"topics"})
				} else if helpCmd.Run != nil {
					helpCmd.Run(helpCmd, []string{"topics"})
					return nil
				}
			}
			return fmt.Errorf("help command not found")
		},
	}
}

func newSnippetCmd() *cobra.Command {
	cmd := &cobra.Command{
		Use:     "snippet",
		Short:   MsgSnippetShort,
		Long:    MsgSnippetLong,
		Example: MsgSnippetExample,
		GroupID: "misc",
		RunE: func(cmd *cobra.Command, args []string) error {
			shell, _ := cmd.Flags().GetString("shell")
			install, _ := cmd.Flags().GetBool("install")

			// Initialize paths to get custom data directory if set
			p, err := initPaths()
			if err != nil {
				return err
			}

			// Always use the actual data directory for the snippet
			dataDir := p.DataDir()

			// Install shell scripts if requested
			if install {
				if err := shellpkg.InstallShellIntegration(dataDir); err != nil {
					return fmt.Errorf("failed to install shell integration: %w", err)
				}
				fmt.Fprintf(os.Stderr, "Shell integration scripts installed to %s/shell/\n", dataDir)
			}

			// Get the appropriate snippet for the shell using the actual data directory
			snippet := types.GetShellIntegrationSnippet(shell, dataDir)

			// Output the snippet with a line break and comment
			fmt.Println()
			fmt.Println("# Run the dodot initialization script if it exists")
			fmt.Print(snippet)

			return nil
		},
	}

	cmd.Flags().StringP("shell", "s", "bash", "Shell type (bash, zsh, fish)")
	cmd.Flags().Bool("install", false, "Install shell integration scripts to data directory")

	return cmd
}

// Completion command removed - use dodot-completions tool to generate shell completions
// The dodot-completions binary is built separately and used during the release process
