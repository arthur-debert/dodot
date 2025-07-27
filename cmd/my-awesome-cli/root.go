package main

import (
	"fmt"

	"github.com/arthur-debert/my-awesome-cli/pkg/logging"
	"github.com/rs/zerolog/log"
	"github.com/spf13/cobra"
)

var (
	verbosity int

	rootCmd = &cobra.Command{
		Use:   "my-awesome-cli",
		Short: "Description of your CLI tool",
		Long:  `Description of your CLI tool`,
		PersistentPreRun: func(cmd *cobra.Command, args []string) {
			// Setup logging based on verbosity
			logging.SetupLogger(verbosity)
			log.Debug().Str("command", cmd.Name()).Msg("Command started")
		},
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
	rootCmd.PersistentFlags().CountVarP(&verbosity, "verbose", "v", "Increase verbosity (-v, -vv, -vvv)")

	// rootCmd.PersistentFlags().StringVar(&cfgFile, "config", "", "config file (default is $HOME/.my-awesome-cli.yaml)")

	// Cobra also supports local flags, which will only run
	// when this action is called directly.
	rootCmd.Flags().BoolP("toggle", "t", false, "Help message for toggle")
	
	// Add version command
	rootCmd.AddCommand(versionCmd)
	
	// Add completion command
	rootCmd.AddCommand(completionCmd)
	
	// Add man page generation command
	rootCmd.AddCommand(manCmd)
}

var versionCmd = &cobra.Command{
	Use:   "version",
	Short: "Print the version number",
	Long:  `Print the version number of my-awesome-cli`,
	Run: func(cmd *cobra.Command, args []string) {
		fmt.Printf("my-awesome-cli version %s (commit: %s, built: %s)\n", version, commit, date)
	},
}

var completionCmd = &cobra.Command{
	Use:   "completion [bash|zsh|fish|powershell]",
	Short: "Generate shell completion script",
	Long: `To load completions:

Bash:
  $ source <(my-awesome-cli completion bash)
  # To load completions for each session, execute once:
  # Linux:
  $ my-awesome-cli completion bash > /etc/bash_completion.d/my-awesome-cli
  # macOS:
  $ my-awesome-cli completion bash > /usr/local/etc/bash_completion.d/my-awesome-cli

Zsh:
  # If shell completion is not already enabled in your environment,
  # you will need to enable it.  You can execute the following once:
  $ echo "autoload -U compinit; compinit" >> ~/.zshrc
  # To load completions for each session, execute once:
  $ my-awesome-cli completion zsh > "${fpath[1]}/_my-awesome-cli"
  # You will need to start a new shell for this setup to take effect.

Fish:
  $ my-awesome-cli completion fish | source
  # To load completions for each session, execute once:
  $ my-awesome-cli completion fish > ~/.config/fish/completions/my-awesome-cli.fish

PowerShell:
  PS> my-awesome-cli completion powershell | Out-String | Invoke-Expression
  # To load completions for every new session, run:
  PS> my-awesome-cli completion powershell > my-awesome-cli.ps1
  # and source this file from your PowerShell profile.
`,
	DisableFlagsInUseLine: true,
	ValidArgs:             []string{"bash", "zsh", "fish", "powershell"},
	Args:                  cobra.MatchAll(cobra.ExactArgs(1), cobra.OnlyValidArgs),
	Run: func(cmd *cobra.Command, args []string) {
		switch args[0] {
		case "bash":
			cmd.Root().GenBashCompletion(cmd.OutOrStdout())
		case "zsh":
			cmd.Root().GenZshCompletion(cmd.OutOrStdout())
		case "fish":
			cmd.Root().GenFishCompletion(cmd.OutOrStdout(), true)
		case "powershell":
			cmd.Root().GenPowerShellCompletionWithDesc(cmd.OutOrStdout())
		}
	},
}

var manCmd = &cobra.Command{
	Use:   "man",
	Short: "Generate man page",
	Long:  `Generate man page for my-awesome-cli`,
	Run: func(cmd *cobra.Command, args []string) {
		log.Error().Msg("Man page generation not yet implemented")
	},
} 