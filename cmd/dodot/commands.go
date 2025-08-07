package dodot

import (
	"fmt"
	"os"
	"path/filepath"

	"github.com/arthur-debert/dodot/internal/version"
	"github.com/arthur-debert/dodot/pkg/cobrax/topics"
	"github.com/arthur-debert/dodot/pkg/commands"
	"github.com/arthur-debert/dodot/pkg/commands/execution"
	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/paths"
	"github.com/arthur-debert/dodot/pkg/style"
	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/rs/zerolog/log"
	"github.com/spf13/cobra"
)

// NewRootCmd creates and returns the root command
func NewRootCmd() *cobra.Command {
	// Initialize custom template formatting functions
	initTemplateFormatting()

	var (
		verbosity int
		dryRun    bool
		force     bool
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
	rootCmd.AddCommand(newSnippetCmd())
	rootCmd.AddCommand(newTopicsCmd())
	rootCmd.AddCommand(newCompletionCmd())

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

			// Use the new DeployPacksDirect implementation with DirectExecutor
			context, err := commands.DeployPacksDirect(commands.DeployPacksOptions{
				DotfilesRoot:       p.DotfilesRoot(),
				PackNames:          args,
				DryRun:             dryRun,
				EnableHomeSymlinks: true,
			})
			if err != nil {
				return fmt.Errorf(MsgErrDeployPacks, err)
			}

			// Display results using rich output
			if dryRun {
				fmt.Println(MsgDryRunNotice)
			}

			// Convert execution context to operations for display
			var operations []types.Operation
			for _, packResult := range context.PackResults {
				for _, opResult := range packResult.Operations {
					if opResult.Operation != nil {
						operations = append(operations, *opResult.Operation)
					}
				}
			}

			renderer := style.NewTerminalRenderer()
			fmt.Println(renderer.RenderOperations(operations))

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

			// Use the new InstallPacksDirect implementation with DirectExecutor
			context, err := commands.InstallPacksDirect(commands.InstallPacksOptions{
				DotfilesRoot:       p.DotfilesRoot(),
				PackNames:          args,
				DryRun:             dryRun,
				Force:              force,
				EnableHomeSymlinks: true,
			})
			if err != nil {
				return fmt.Errorf(MsgErrInstallPacks, err)
			}

			// Display results using rich output
			if dryRun {
				fmt.Println(MsgDryRunNotice)
			}

			// Convert execution context to operations for display
			var operations []types.Operation
			for _, packResult := range context.PackResults {
				for _, opResult := range packResult.Operations {
					if opResult.Operation != nil {
						operations = append(operations, *opResult.Operation)
					}
				}
			}

			renderer := style.NewTerminalRenderer()
			fmt.Println(renderer.RenderOperations(operations))

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

			// Display the packs using rich output
			renderer := style.NewTerminalRenderer()
			fmt.Println(renderer.RenderPackList(result.Packs))

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

			log.Info().Str("dotfiles_root", p.DotfilesRoot()).Msg("Checking status from dotfiles root")

			// Use the new StatusPacksDirect implementation
			result, err := commands.StatusPacksDirect(commands.StatusPacksOptions{
				DotfilesRoot: p.DotfilesRoot(),
				PackNames:    args,
			})
			if err != nil {
				return fmt.Errorf(MsgErrStatusPacks, err)
			}

			// Convert and display status using rich output
			renderer := style.NewTerminalRenderer()
			var packStatuses []style.PackStatus

			for _, ps := range result.Packs {
				packStatuses = append(packStatuses, style.ConvertDisplayPackToPackStatus(ps))
			}

			fmt.Println(renderer.RenderPackStatuses(packStatuses))

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

			// Execute operations
			opResults, err := execution.ExecuteOperations(execution.ExecuteOperationsOptions{
				Operations:          result.Operations,
				DryRun:              false,
				EnableHomeSymlinks:  false,
				UseCombinedExecutor: false,
			})
			if err != nil {
				return err
			}
			// TODO: Use opResults for better display/tracking
			_ = opResults

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

			// Execute operations
			opResults, err := execution.ExecuteOperations(execution.ExecuteOperationsOptions{
				Operations:          result.Operations,
				DryRun:              false,
				EnableHomeSymlinks:  false,
				UseCombinedExecutor: false,
			})
			if err != nil {
				return err
			}
			// TODO: Use opResults for better display/tracking
			_ = opResults

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
				if err := commands.InstallShellIntegration(dataDir); err != nil {
					return fmt.Errorf("failed to install shell integration: %w", err)
				}
				fmt.Fprintf(os.Stderr, "Shell integration scripts installed to %s/shell/\n", dataDir)
			}

			// Get the appropriate snippet for the shell using the actual data directory
			snippet := types.GetShellIntegrationSnippet(shell, dataDir)

			// Output the snippet
			fmt.Print(snippet)

			return nil
		},
	}

	cmd.Flags().StringP("shell", "s", "bash", "Shell type (bash, zsh, fish)")
	cmd.Flags().Bool("install", false, "Install shell integration scripts to data directory")

	return cmd
}

func newCompletionCmd() *cobra.Command {
	return &cobra.Command{
		Use:                   "completion [bash|zsh|fish|powershell]",
		Short:                 MsgCompletionShort,
		Long:                  MsgCompletionLong,
		DisableFlagsInUseLine: true,
		ValidArgs:             []string{"bash", "zsh", "fish", "powershell"},
		Args:                  cobra.MatchAll(cobra.ExactArgs(1), cobra.OnlyValidArgs),
		GroupID:               "misc",
		RunE: func(cmd *cobra.Command, args []string) error {
			switch args[0] {
			case "bash":
				return cmd.Root().GenBashCompletion(os.Stdout)
			case "zsh":
				return cmd.Root().GenZshCompletion(os.Stdout)
			case "fish":
				return cmd.Root().GenFishCompletion(os.Stdout, true)
			case "powershell":
				return cmd.Root().GenPowerShellCompletionWithDesc(os.Stdout)
			}
			return nil
		},
	}
}
