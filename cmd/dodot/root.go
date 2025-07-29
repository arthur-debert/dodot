package main

import (
	"fmt"
	"os"

	"github.com/arthur-debert/dodot/pkg/core"
	"github.com/arthur-debert/dodot/pkg/errors"
	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/rs/zerolog/log"
	"github.com/spf13/cobra"
	"github.com/spf13/cobra/doc"
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
	
	// Add deploy command
	rootCmd.AddCommand(deployCmd)
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

		// Run the pipeline
		candidates, err := core.GetPackCandidates(dotfilesRoot)
		if err != nil {
			return err
		}

		packs, err := core.GetPacks(candidates)
		if err != nil {
			return err
		}

		// The rest of the pipeline will be called here once implemented:
		// 1. GetFiringTriggers(packs) - scan files and match triggers
		// 2. GetActions(matches) - process matches through matchers and power-ups
		// 3. FilterRunOnceActions(actions, force) - filter out already-executed run-once actions
		// 4. GetFsOps(actions) - convert actions to filesystem operations
		// 5. Execute operations through synthfs
		
		// For now, just log the packs that were found
		for _, pack := range packs {
			logger.Info().
				Str("pack", pack.Name).
				Int("priority", pack.Priority).
				Msg("Loaded pack")
		}

		logger.Info().Msg("Deploy command finished")
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