package main

import (
	"fmt"
	"os"
	"strings"

	"github.com/arthur-debert/dodot/pkg/core"
	"github.com/arthur-debert/dodot/pkg/errors"
	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/rs/zerolog/log"
	"github.com/spf13/cobra"
	"github.com/spf13/cobra/doc"

	// Import packages to ensure their init() functions are called for registration
	_ "github.com/arthur-debert/dodot/pkg/powerups"
	_ "github.com/arthur-debert/dodot/pkg/triggers"
)

var (
	verbosity int
	dryRun    bool
	force     bool

	rootCmd = &cobra.Command{
		Use:   "dodot",
		Short: "A stateless dotfiles manager",
		Long: `dodot is a stateless dotfiles manager that helps you organize and deploy
your configuration files in a structured, safe way while letting git handle
versioning and history.`,
		PersistentPreRun: func(cmd *cobra.Command, args []string) {
			// Setup logging based on verbosity
			logging.SetupLogger(verbosity)
			log.Debug().Str("command", cmd.Name()).Msg("Command started")
		},
		SilenceUsage:  true,
		SilenceErrors: true,
		// Uncomment the following line if your bare application
		// has an action associated with it:
		// Run: func(cmd *cobra.Command, args []string) { },
	}
)

// Execute adds all child commands to the root command and sets flags appropriately.
// This is called by main.main(). It only needs to happen once to the rootCmd.
func Execute() error {
	return rootCmd.Execute()
}

func init() {
	// Here you will define your flags and configuration settings.
	// Cobra supports persistent flags, which, if defined here,
	// will be global for your application.

	// Verbosity flag for logging
	rootCmd.PersistentFlags().CountVarP(&verbosity, "verbose", "v", "Increase verbosity (-v INFO, -vv DEBUG, -vvv TRACE)")

	// Dry-run flag
	rootCmd.PersistentFlags().BoolVar(&dryRun, "dry-run", false, "Preview changes without executing them")

	// Force flag
	rootCmd.PersistentFlags().BoolVar(&force, "force", false, "Force execution of run-once power-ups even if already executed")

	// rootCmd.PersistentFlags().StringVar(&cfgFile, "config", "", "config file (default is $HOME/.config/dodot/config.toml)")

	// Remove unused toggle flag

	// Add version command
	rootCmd.AddCommand(versionCmd)

	// Add completion command
	rootCmd.AddCommand(completionCmd)

	// Add man page generation command
	rootCmd.AddCommand(manCmd)

	// Add all commands
	rootCmd.AddCommand(deployCmd)
	rootCmd.AddCommand(installCmd)
	rootCmd.AddCommand(listCmd)
	rootCmd.AddCommand(statusCmd)
	rootCmd.AddCommand(initCmd)
	rootCmd.AddCommand(fillCmd)
}

var versionCmd = &cobra.Command{
	Use:   "version",
	Short: "Print version information",
	Long:  `Print version information for dodot`,
	Run: func(cmd *cobra.Command, args []string) {
		fmt.Printf("dodot version %s\n", version)
		fmt.Printf("  commit: %s\n", commit)
		fmt.Printf("  built:  %s\n", date)
	},
}

var completionCmd = &cobra.Command{
	Use:   "completion [bash|zsh|fish|powershell]",
	Short: "Generate shell completion script",
	Long: `To load completions:

Bash:
  $ source <(dodot completion bash)
  # To load completions for each session, execute once:
  # Linux:
  $ dodot completion bash > /etc/bash_completion.d/dodot
  # macOS:
  $ dodot completion bash > /usr/local/etc/bash_completion.d/dodot

Zsh:
  # If shell completion is not already enabled in your environment,
  # you will need to enable it.  You can execute the following once:
  $ echo "autoload -U compinit; compinit" >> ~/.zshrc
  # To load completions for each session, execute once:
  $ dodot completion zsh > "${fpath[1]}/_dodot"
  # You will need to start a new shell for this setup to take effect.

Fish:
  $ dodot completion fish | source
  # To load completions for each session, execute once:
  $ dodot completion fish > ~/.config/fish/completions/dodot.fish

PowerShell:
  PS> dodot completion powershell | Out-String | Invoke-Expression
  # To load completions for every new session, run:
  PS> dodot completion powershell > dodot.ps1
  # and source this file from your PowerShell profile.
`,
	DisableFlagsInUseLine: true,
	ValidArgs:             []string{"bash", "zsh", "fish", "powershell"},
	Args:                  cobra.MatchAll(cobra.ExactArgs(1), cobra.OnlyValidArgs),
	Run: func(cmd *cobra.Command, args []string) {
		switch args[0] {
		case "bash":
			if err := cmd.Root().GenBashCompletion(cmd.OutOrStdout()); err != nil {
				log.Error().Err(err).Msg("Failed to generate bash completion")
			}
		case "zsh":
			if err := cmd.Root().GenZshCompletion(cmd.OutOrStdout()); err != nil {
				log.Error().Err(err).Msg("Failed to generate zsh completion")
			}
		case "fish":
			if err := cmd.Root().GenFishCompletion(cmd.OutOrStdout(), true); err != nil {
				log.Error().Err(err).Msg("Failed to generate fish completion")
			}
		case "powershell":
			if err := cmd.Root().GenPowerShellCompletionWithDesc(cmd.OutOrStdout()); err != nil {
				log.Error().Err(err).Msg("Failed to generate powershell completion")
			}
		}
	},
}

var manCmd = &cobra.Command{
	Use:   "man",
	Short: "Generate man page",
	Long:  `Generate man page for dodot`,
	RunE: func(cmd *cobra.Command, args []string) error {
		header := &doc.GenManHeader{
			Title:   "DODOT",
			Section: "1",
		}
		return doc.GenManTree(rootCmd, header, "/tmp")
	},
}

var deployCmd = &cobra.Command{
	Use:   "deploy [packs...]",
	Short: "Deploy dotfiles to the system",
	Long: `Deploy processes all packs in your dotfiles directory and creates
the necessary symlinks, installs packages, and performs other configured actions.

If no packs are specified, all packs in the DOTFILES_ROOT will be deployed.`,
	RunE: func(cmd *cobra.Command, args []string) error {
		logger := logging.GetLogger("cmd.deploy")
		logger.Info().
			Bool("dryRun", dryRun).
			Bool("force", force).
			Strs("packs", args).
			Msg("Starting deploy")

		dotfilesRoot, err := getDotfilesRoot()
		if err != nil {
			return err
		}

		// Execute deployment pipeline
		result, err := core.DeployPacks(core.DeployPacksOptions{
			DotfilesRoot: dotfilesRoot,
			PackNames:    args,
			DryRun:       dryRun,
		})
		if err != nil {
			return err
		}

		// Log execution results
		logger.Info().
			Int("packs", len(result.Packs)).
			Int("operations", len(result.Operations)).
			Bool("dryRun", result.DryRun).
			Msg("Deployment pipeline completed")

		// Execute operations if not in dry-run mode
		if !dryRun && len(result.Operations) > 0 {
			logger.Info().Msg("Executing operations through synthfs")

			executor := core.NewSynthfsExecutor(dryRun)

			// Check if any operations are symlinks targeting home directory
			// If so, enable home symlinks with backup
			if hasHomeSymlinks(result.Operations) {
				logger.Info().Msg("Detected symlinks targeting home directory, enabling home symlink mode")
				executor.EnableHomeSymlinks(true)
			}

			if err := executor.ExecuteOperations(result.Operations); err != nil {
				return errors.Wrap(err, errors.ErrActionExecute,
					"failed to execute operations")
			}

			logger.Info().Msg("All operations executed successfully")
		} else if dryRun {
			logger.Info().Msg("Dry run mode - no operations were executed")
		} else {
			logger.Info().Msg("No operations to execute")
		}

		// Output user-friendly results
		if !dryRun && len(result.Operations) > 0 {
			fmt.Printf(msgPacksDeployed+"\n", len(result.Packs))
			fmt.Printf(msgOperationsExecuted+"\n", len(result.Operations))
		} else if dryRun {
			fmt.Println(msgDryRunMode)
			if len(result.Operations) > 0 {
				fmt.Printf("Would execute %d operation(s)\n", len(result.Operations))
			}
		} else {
			fmt.Println(msgNoOperationsNeeded)
		}

		return nil
	},
}

func getDotfilesRoot() (string, error) {
	root := os.Getenv("DOTFILES_ROOT")
	if root == "" {
		return "", errors.New(errors.ErrInvalidInput, "DOTFILES_ROOT environment variable not set")
	}
	return root, nil
}

// hasHomeSymlinks checks if any operations are symlinks targeting the home directory
func hasHomeSymlinks(ops []types.Operation) bool {
	homeDir, err := os.UserHomeDir()
	if err != nil {
		return false
	}

	for _, op := range ops {
		if op.Type == types.OperationCreateSymlink && op.Target != "" {
			// Check if target is in home directory
			if strings.HasPrefix(op.Target, homeDir) {
				return true
			}
			// Also check for ~ prefix
			if strings.HasPrefix(op.Target, "~/") {
				return true
			}
		}
	}

	return false
}

// CLI command constants
const (
	msgPacksDeployed      = "Successfully deployed %d pack(s)"
	msgPacksInstalled     = "Successfully installed %d pack(s)"
	msgNoPacksFound       = "No packs found in %s"
	msgDryRunMode         = "DRY RUN MODE - No changes were made"
	msgOperationsExecuted = "Executed %d operation(s)"
	msgNoOperationsNeeded = "No operations needed"
)

var installCmd = &cobra.Command{
	Use:   "install [packs...]",
	Short: "Install and deploy dotfiles to the system",
	Long: `Install processes all packs in your dotfiles directory, runs installation
scripts and Brewfiles, then creates the necessary symlinks and performs other
configured actions.

If no packs are specified, all packs in the DOTFILES_ROOT will be installed.`,
	RunE: func(cmd *cobra.Command, args []string) error {
		logger := logging.GetLogger("cmd.install")
		logger.Info().
			Bool("dryRun", dryRun).
			Bool("force", force).
			Strs("packs", args).
			Msg("Starting install")

		dotfilesRoot, err := getDotfilesRoot()
		if err != nil {
			return err
		}

		// Execute installation pipeline
		result, err := core.InstallPacks(core.InstallPacksOptions{
			DotfilesRoot: dotfilesRoot,
			PackNames:    args,
			DryRun:       dryRun,
			Force:        force,
		})
		if err != nil {
			return err
		}

		// Log execution results
		logger.Info().
			Int("packs", len(result.Packs)).
			Int("operations", len(result.Operations)).
			Bool("dryRun", result.DryRun).
			Msg("Installation pipeline completed")

		// Execute operations if not in dry-run mode
		if !dryRun && len(result.Operations) > 0 {
			logger.Info().Msg("Executing operations through synthfs")

			executor := core.NewSynthfsExecutor(dryRun)

			// Check if any operations are symlinks targeting home directory
			if hasHomeSymlinks(result.Operations) {
				logger.Info().Msg("Detected symlinks targeting home directory, enabling home symlink mode")
				executor.EnableHomeSymlinks(true)
			}

			if err := executor.ExecuteOperations(result.Operations); err != nil {
				return errors.Wrap(err, errors.ErrActionExecute,
					"failed to execute operations")
			}

			fmt.Printf(msgPacksInstalled+"\n", len(result.Packs))
			fmt.Printf(msgOperationsExecuted+"\n", len(result.Operations))
		} else if dryRun {
			fmt.Println(msgDryRunMode)
			if len(result.Operations) > 0 {
				fmt.Printf("Would execute %d operation(s)\n", len(result.Operations))
			}
		} else {
			fmt.Println(msgNoOperationsNeeded)
		}

		return nil
	},
}

var listCmd = &cobra.Command{
	Use:   "list",
	Short: "List all available packs",
	Long: `List shows all available packs in your DOTFILES_ROOT directory.
This includes any directory that contains dotfiles or configuration files.`,
	RunE: func(cmd *cobra.Command, args []string) error {
		logger := logging.GetLogger("cmd.list")
		logger.Debug().Msg("Starting list")

		dotfilesRoot, err := getDotfilesRoot()
		if err != nil {
			return err
		}

		result, err := core.ListPacks(core.ListPacksOptions{
			DotfilesRoot: dotfilesRoot,
		})
		if err != nil {
			return err
		}

		if len(result.Packs) == 0 {
			fmt.Printf(msgNoPacksFound+"\n", dotfilesRoot)
			return nil
		}

		fmt.Println("Available packs:")
		for _, pack := range result.Packs {
			fmt.Printf("  %s\n", pack.Name)
		}

		return nil
	},
}

var statusCmd = &cobra.Command{
	Use:   "status [packs...]",
	Short: "Show deployment status of packs",
	Long: `Status shows the current deployment status of the specified packs,
including information about installed components, symlinks, and shell modifications.

If no packs are specified, status for all packs will be shown.`,
	RunE: func(cmd *cobra.Command, args []string) error {
		logger := logging.GetLogger("cmd.status")
		logger.Debug().Strs("packs", args).Msg("Starting status")

		dotfilesRoot, err := getDotfilesRoot()
		if err != nil {
			return err
		}

		result, err := core.StatusPacks(core.StatusPacksOptions{
			DotfilesRoot: dotfilesRoot,
			PackNames:    args,
		})
		if err != nil {
			return err
		}

		if len(result.Packs) == 0 {
			fmt.Printf(msgNoPacksFound+"\n", dotfilesRoot)
			return nil
		}

		for _, pack := range result.Packs {
			fmt.Printf("\n%s pack:\n", pack.Name)
			for _, powerUp := range pack.PowerUpState {
				fmt.Printf("  %s: %s - %s\n", powerUp.Name, powerUp.State, powerUp.Description)
			}
		}

		return nil
	},
}

var initCmd = &cobra.Command{
	Use:   "init <pack-name>",
	Short: "Create a new pack with template files",
	Long: `Init creates a new pack directory with placeholder files and configuration.
This includes template files for all available power-ups and a basic .dodot.toml
configuration file.`,
	Args: cobra.ExactArgs(1),
	RunE: func(cmd *cobra.Command, args []string) error {
		logger := logging.GetLogger("cmd.init")
		packName := args[0]
		logger.Debug().Str("pack", packName).Msg("Starting init")

		dotfilesRoot, err := getDotfilesRoot()
		if err != nil {
			return err
		}

		result, err := core.InitPack(core.InitPackOptions{
			DotfilesRoot: dotfilesRoot,
			PackName:     packName,
		})
		if err != nil {
			return err
		}

		fmt.Printf("Created new pack '%s' at %s\n", result.PackName, result.Path)
		fmt.Printf("Files created:\n")
		for _, file := range result.FilesCreated {
			fmt.Printf("  %s\n", file)
		}
		fmt.Printf("\nNext steps:\n")
		fmt.Printf("1. Add your configuration files to %s\n", result.Path)
		fmt.Printf("2. Edit the template files as needed\n")
		fmt.Printf("3. Run 'dodot install %s' to install this pack\n", packName)

		return nil
	},
}

var fillCmd = &cobra.Command{
	Use:   "fill <pack-name>",
	Short: "Add placeholder files to an existing pack",
	Long: `Fill adds placeholder files for each power-up to an existing pack.
This is useful for adding missing power-up files to packs that were created manually.`,
	Args: cobra.ExactArgs(1),
	RunE: func(cmd *cobra.Command, args []string) error {
		logger := logging.GetLogger("cmd.fill")
		packName := args[0]
		logger.Debug().Str("pack", packName).Msg("Starting fill")

		dotfilesRoot, err := getDotfilesRoot()
		if err != nil {
			return err
		}

		result, err := core.FillPack(core.FillPackOptions{
			DotfilesRoot: dotfilesRoot,
			PackName:     packName,
		})
		if err != nil {
			return err
		}

		if len(result.FilesCreated) == 0 {
			fmt.Printf("No new files needed for pack '%s' - all template files already exist\n", packName)
		} else {
			fmt.Printf("Added template files to pack '%s':\n", packName)
			for _, file := range result.FilesCreated {
				fmt.Printf("  %s\n", file)
			}
		}

		return nil
	},
}
