package dodot

import (
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"sort"
	"strings"

	"github.com/arthur-debert/dodot/cmd/dodot/commands/addignore"
	"github.com/arthur-debert/dodot/cmd/dodot/commands/adopt"
	"github.com/arthur-debert/dodot/cmd/dodot/commands/fill"
	"github.com/arthur-debert/dodot/cmd/dodot/commands/genconfig"
	initcmd "github.com/arthur-debert/dodot/cmd/dodot/commands/init"
	"github.com/arthur-debert/dodot/cmd/dodot/commands/off"
	"github.com/arthur-debert/dodot/cmd/dodot/commands/on"
	"github.com/arthur-debert/dodot/cmd/dodot/commands/snippet"
	"github.com/arthur-debert/dodot/cmd/dodot/commands/status"
	"github.com/arthur-debert/dodot/cmd/dodot/commands/topics"
	"github.com/arthur-debert/dodot/internal/version"
	topicspkg "github.com/arthur-debert/dodot/pkg/cobrax/topics"
	"github.com/arthur-debert/dodot/pkg/commands"
	offpkg "github.com/arthur-debert/dodot/pkg/commands/off"
	onpkg "github.com/arthur-debert/dodot/pkg/commands/on"
	"github.com/arthur-debert/dodot/pkg/core"
	doerrors "github.com/arthur-debert/dodot/pkg/errors"
	"github.com/arthur-debert/dodot/pkg/logging"
	"github.com/arthur-debert/dodot/pkg/packs"
	"github.com/arthur-debert/dodot/pkg/paths"
	shellpkg "github.com/arthur-debert/dodot/pkg/shell"
	"github.com/arthur-debert/dodot/pkg/types"
	"github.com/arthur-debert/dodot/pkg/ui"
	"github.com/arthur-debert/dodot/pkg/ui/output"
	"github.com/rs/zerolog/log"
	"github.com/spf13/cobra"
)

// createRenderer is a helper function to create a renderer based on the format flag
func createRenderer(cmd *cobra.Command) (ui.Renderer, error) {
	formatStr, _ := cmd.Root().PersistentFlags().GetString("format")
	format, err := ui.ParseFormat(formatStr)
	if err != nil {
		return nil, fmt.Errorf("invalid format: %w", err)
	}

	renderer, err := ui.NewRenderer(format, os.Stdout)
	if err != nil {
		return nil, fmt.Errorf("failed to create renderer: %w", err)
	}

	return renderer, nil
}

// NewRootCmd creates and returns the root command
func NewRootCmd() *cobra.Command {
	// Initialize custom template formatting functions
	initTemplateFormatting()

	var (
		verbosity  int
		dryRun     bool
		force      bool
		configFile string
		formatStr  string
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
		SilenceUsage:      true, // We'll show help manually
		SilenceErrors:     true, // We'll handle error display ourselves
		DisableAutoGenTag: true,
		CompletionOptions: cobra.CompletionOptions{DisableDefaultCmd: true}, // Remove completion command
	}

	// Global flags
	rootCmd.PersistentFlags().CountVarP(&verbosity, "verbose", "v", MsgFlagVerbose)
	rootCmd.PersistentFlags().BoolVar(&dryRun, "dry-run", false, MsgFlagDryRun)
	rootCmd.PersistentFlags().BoolVar(&force, "force", false, MsgFlagForce)
	rootCmd.PersistentFlags().StringVar(&configFile, "config", "", "Path to custom styles configuration file")
	rootCmd.PersistentFlags().StringVar(&formatStr, "format", "auto", "Output format (auto|term|text|json)")

	// Disable automatic help command (we'll use our custom one from topics)
	rootCmd.SetHelpCommand(&cobra.Command{Hidden: true})

	// Define command groups
	rootCmd.AddGroup(&cobra.Group{
		ID:    "core",
		Title: "CORE:",
	})
	rootCmd.AddGroup(&cobra.Group{
		ID:    "single-pack",
		Title: "SINGLE PACK CONVENIENCE:",
	})
	rootCmd.AddGroup(&cobra.Group{
		ID:    "config",
		Title: "CONFIG HELPERS:",
	})

	// Set custom help template
	rootCmd.SetUsageTemplate(MsgUsageTemplate)

	// Add all commands in the desired order
	// Core commands
	rootCmd.AddCommand(newOnCmd())
	rootCmd.AddCommand(newOffCmd())
	rootCmd.AddCommand(newStatusCmd())
	// Single pack convenience commands
	rootCmd.AddCommand(newAddIgnoreCmd())
	rootCmd.AddCommand(newInitCmd())
	rootCmd.AddCommand(newFillCmd())
	rootCmd.AddCommand(newAdoptCmd())
	// Config helpers
	rootCmd.AddCommand(newSnippetCmd())
	rootCmd.AddCommand(newGenConfigCmd())
	// Additional commands
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
				opts := topicspkg.Options{
					Extensions: []string{".txt", ".md", ".txxt"},
					// Always use Glamour renderer for markdown files
					Renderer: topicspkg.NewGlamourRenderer(),
				}

				if err := topicspkg.InitializeWithOptions(rootCmd, helpPath, opts); err == nil {
					break
				}
			}
		}
	}

	return rootCmd
}

// initPaths initializes the paths instance and shows a warning if using fallback
func initPaths() (paths.Paths, error) {
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
// It returns both discovered pack names and allows directory completion
// since users often use shell completion to navigate directories
func packNamesCompletion(cmd *cobra.Command, args []string, toComplete string) ([]string, cobra.ShellCompDirective) {
	// Initialize paths
	p, err := initPaths()
	if err != nil {
		// Even if we can't find dotfiles root, allow directory completion
		return nil, cobra.ShellCompDirectiveFilterDirs
	}

	// Get list of packs using core.DiscoverAndSelectPacks
	allPacks, err := core.DiscoverAndSelectPacks(p.DotfilesRoot(), nil)
	if err != nil {
		// If listing fails, still allow directory completion
		return nil, cobra.ShellCompDirectiveFilterDirs
	}

	// Extract pack names
	packNames := packs.GetPackNames(allPacks)

	// Filter out already specified packs
	var availablePacks []string
	for _, pack := range packNames {
		found := false
		for _, arg := range args {
			// Normalize for comparison (remove trailing slashes)
			normalizedArg := strings.TrimRight(arg, "/")
			if normalizedArg == pack {
				found = true
				break
			}
		}
		if !found {
			availablePacks = append(availablePacks, pack)
		}
	}

	// Return pack names and allow directory completion
	// This lets users navigate the filesystem and also see available packs
	return availablePacks, cobra.ShellCompDirectiveFilterDirs
}

// Command creation functions that wrap the imported commands and add the logic

func newStatusCmd() *cobra.Command {
	cmd := status.NewCommand()
	cmd.ValidArgsFunction = packNamesCompletion
	cmd.RunE = func(cmd *cobra.Command, args []string) error {
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
			return fmt.Errorf(status.MsgErrStatusPacks, err)
		}

		// Create renderer and display results
		renderer, err := createRenderer(cmd)
		if err != nil {
			return err
		}

		if err := renderer.RenderResult(result); err != nil {
			return fmt.Errorf("failed to display status: %w", err)
		}

		return nil
	}
	return cmd
}

func newInitCmd() *cobra.Command {
	cmd := initcmd.NewCommand()
	cmd.RunE = func(cmd *cobra.Command, args []string) error {
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
			return fmt.Errorf(initcmd.MsgErrInitPack, err)
		}

		// Create renderer and display results
		renderer, err := createRenderer(cmd)
		if err != nil {
			return err
		}

		// Get the status of the newly created pack
		statusResult, err := commands.StatusPacks(commands.StatusPacksOptions{
			DotfilesRoot: p.DotfilesRoot(),
			PackNames:    []string{packName},
			Paths:        p,
		})
		if err != nil {
			return fmt.Errorf("failed to get pack status: %w", err)
		}

		// Update command name to reflect init action
		statusResult.Command = "init"
		statusResult.DryRun = false

		// Create message based on what was created
		message := fmt.Sprintf("The pack %s has been initialized with %d files.", packName, len(result.FilesCreated))

		// Create CommandResult with appropriate message
		cmdResult := &types.CommandResult{
			Message: message,
			Result:  statusResult,
		}

		if err := renderer.RenderResult(cmdResult); err != nil {
			return fmt.Errorf("failed to render results: %w", err)
		}

		return nil
	}
	return cmd
}

func newFillCmd() *cobra.Command {
	cmd := fill.NewCommand()
	cmd.RunE = func(cmd *cobra.Command, args []string) error {
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
			return fmt.Errorf(fill.MsgErrFillPack, err)
		}

		// Create renderer and display results
		renderer, err := createRenderer(cmd)
		if err != nil {
			return err
		}

		// Get the status of the filled pack
		statusResult, err := commands.StatusPacks(commands.StatusPacksOptions{
			DotfilesRoot: p.DotfilesRoot(),
			PackNames:    []string{packName},
			Paths:        p,
		})
		if err != nil {
			return fmt.Errorf("failed to get pack status: %w", err)
		}

		// Update command name to reflect fill action
		statusResult.Command = "fill"
		statusResult.DryRun = false

		// Create message based on what was created
		var message string
		if len(result.FilesCreated) == 0 {
			message = fmt.Sprintf("The pack %s already has all file types.", packName)
		} else {
			message = fmt.Sprintf("The pack %s has been filled with %d placeholder files.", packName, len(result.FilesCreated))
		}

		// Create CommandResult with appropriate message
		cmdResult := &types.CommandResult{
			Message: message,
			Result:  statusResult,
		}

		if err := renderer.RenderResult(cmdResult); err != nil {
			return fmt.Errorf("failed to render results: %w", err)
		}

		return nil
	}
	return cmd
}

func newAdoptCmd() *cobra.Command {
	cmd := adopt.NewCommand()
	cmd.ValidArgsFunction = adoptCompletion
	cmd.RunE = func(cmd *cobra.Command, args []string) error {
		// Initialize paths (will show warning if using fallback)
		p, err := initPaths()
		if err != nil {
			return err
		}

		// Get force flag value (it's a persistent flag)
		force, _ := cmd.Root().PersistentFlags().GetBool("force")

		packName := args[0]
		sourcePaths := args[1:]

		log.Info().
			Str("dotfiles_root", p.DotfilesRoot()).
			Str("pack", packName).
			Strs("source_paths", sourcePaths).
			Bool("force", force).
			Msg("Adopting files into pack")

		// Adopt files using the new implementation
		result, err := commands.AdoptFiles(commands.AdoptFilesOptions{
			DotfilesRoot: p.DotfilesRoot(),
			PackName:     packName,
			SourcePaths:  sourcePaths,
			Force:        force,
		})
		if err != nil {
			// Check if this is a pack not found error and provide detailed help
			var dodotErr *doerrors.DodotError
			if errors.As(err, &dodotErr) && dodotErr.Code == doerrors.ErrPackNotFound {
				return handlePackNotFoundError(dodotErr, p, "adopt")
			}
			return fmt.Errorf(adopt.MsgErrAdoptFiles, err)
		}

		// Create renderer and display results
		renderer, err := createRenderer(cmd)
		if err != nil {
			return err
		}

		// Get the status of the pack after adoption
		statusResult, err := commands.StatusPacks(commands.StatusPacksOptions{
			DotfilesRoot: p.DotfilesRoot(),
			PackNames:    []string{packName},
			Paths:        p,
		})
		if err != nil {
			return fmt.Errorf("failed to get pack status: %w", err)
		}

		// Update command name to reflect adopt action
		statusResult.Command = "adopt"
		statusResult.DryRun = false

		// Create message based on what was adopted
		var message string
		if len(result.AdoptedFiles) == 0 {
			message = fmt.Sprintf("No files were adopted into the pack %s.", packName)
		} else if len(result.AdoptedFiles) == 1 {
			message = fmt.Sprintf("The file %s has been adopted into the pack %s.",
				filepath.Base(result.AdoptedFiles[0].OriginalPath), packName)
		} else {
			message = fmt.Sprintf("%d files have been adopted into the pack %s.",
				len(result.AdoptedFiles), packName)
		}

		// Create CommandResult with appropriate message
		cmdResult := &types.CommandResult{
			Message: message,
			Result:  statusResult,
		}

		if err := renderer.RenderResult(cmdResult); err != nil {
			return fmt.Errorf("failed to render results: %w", err)
		}

		return nil
	}
	return cmd
}

// adoptCompletion provides shell completion for the adopt command
func adoptCompletion(cmd *cobra.Command, args []string, toComplete string) ([]string, cobra.ShellCompDirective) {
	// First argument is the pack name
	if len(args) == 0 {
		return packNamesCompletion(cmd, args, toComplete)
	}
	// Subsequent arguments are file paths
	return nil, cobra.ShellCompDirectiveDefault
}

func newAddIgnoreCmd() *cobra.Command {
	cmd := addignore.NewCommand()
	cmd.ValidArgsFunction = packNamesCompletion
	cmd.RunE = func(cmd *cobra.Command, args []string) error {
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
			return fmt.Errorf(addignore.MsgErrAddIgnore, err)
		}

		// Create renderer and display results
		renderer, err := createRenderer(cmd)
		if err != nil {
			return err
		}

		// Get the status of the pack after adding ignore
		statusResult, err := commands.StatusPacks(commands.StatusPacksOptions{
			DotfilesRoot: p.DotfilesRoot(),
			PackNames:    []string{packName},
			Paths:        p,
		})
		if err != nil {
			return fmt.Errorf("failed to get pack status: %w", err)
		}

		// Update command name to reflect add-ignore action
		statusResult.Command = "add-ignore"
		statusResult.DryRun = false

		// Create message based on the result
		var message string
		if result.AlreadyExisted {
			message = fmt.Sprintf("The pack %s already has a .dodotignore file.", packName)
		} else {
			message = fmt.Sprintf("A .dodotignore file has been added to the pack %s.", packName)
		}

		// Create CommandResult with appropriate message
		cmdResult := &types.CommandResult{
			Message: message,
			Result:  statusResult,
		}

		if err := renderer.RenderResult(cmdResult); err != nil {
			return fmt.Errorf("failed to render results: %w", err)
		}

		return nil
	}
	return cmd
}

func newTopicsCmd() *cobra.Command {
	cmd := topics.NewCommand()
	cmd.RunE = func(cmd *cobra.Command, args []string) error {
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
	}
	return cmd
}

func newSnippetCmd() *cobra.Command {
	cmd := snippet.NewCommand()
	cmd.RunE = func(cmd *cobra.Command, args []string) error {
		shell, _ := cmd.Flags().GetString("shell")
		provision, _ := cmd.Flags().GetBool("provision")

		// Initialize paths to get custom data directory if set
		p, err := initPaths()
		if err != nil {
			return err
		}

		// Always use the actual data directory for the snippet
		dataDir := p.DataDir()

		// Create renderer for output
		renderer, err := createRenderer(cmd)
		if err != nil {
			return err
		}

		// Install shell scripts if requested
		var installMessage string
		if provision {
			if err := shellpkg.InstallShellIntegration(dataDir); err != nil {
				return fmt.Errorf("failed to install shell integration: %w", err)
			}
			installMessage = fmt.Sprintf("Shell integration scripts installed to %s/shell/", dataDir)
		}

		// Get the appropriate snippet for the shell using the actual data directory
		snippetText := types.GetShellIntegrationSnippet(shell, dataDir)

		// Create a result structure for the snippet
		snippetResult := struct {
			Shell          string `json:"shell"`
			DataDir        string `json:"dataDir"`
			Snippet        string `json:"snippet"`
			Installed      bool   `json:"installed"`
			InstallMessage string `json:"installMessage,omitempty"`
		}{
			Shell:          shell,
			DataDir:        dataDir,
			Snippet:        snippetText,
			Installed:      provision,
			InstallMessage: installMessage,
		}

		// For text/terminal output, we want just the snippet with optional message
		// For JSON output, we want structured data
		format, _ := cmd.Root().PersistentFlags().GetString("format")
		parsedFormat, _ := ui.ParseFormat(format)

		if parsedFormat == ui.FormatJSON || (parsedFormat == ui.FormatAuto && ui.DetectFormat(os.Stdout) == ui.FormatJSON) {
			// JSON format - return structured data
			if err := renderer.RenderResult(snippetResult); err != nil {
				return fmt.Errorf("failed to render snippet: %w", err)
			}
		} else {
			// Text/Terminal format - output the snippet directly with header comment
			if installMessage != "" {
				if err := renderer.RenderMessage(installMessage); err != nil {
					return err
				}
				if err := renderer.RenderMessage(""); err != nil { // blank line
					return err
				}
			}

			// Output the snippet with comment header
			fullSnippet := "\n# Run the dodot initialization script if it exists\n" + snippetText
			if err := renderer.RenderMessage(fullSnippet); err != nil {
				return fmt.Errorf("failed to render snippet: %w", err)
			}
		}

		return nil
	}
	return cmd
}

func newGenConfigCmd() *cobra.Command {
	cmd := genconfig.NewCommand()
	cmd.RunE = func(cmd *cobra.Command, args []string) error {
		write, _ := cmd.Flags().GetBool("write")

		// Initialize paths if writing to packs
		var dotfilesRoot string
		if write && len(args) > 0 {
			p, err := initPaths()
			if err != nil {
				return err
			}
			dotfilesRoot = p.DotfilesRoot()
		}

		result, err := commands.GenConfig(commands.GenConfigOptions{
			DotfilesRoot: dotfilesRoot,
			PackNames:    args,
			Write:        write,
		})
		if err != nil {
			return fmt.Errorf("failed to generate config: %w", err)
		}

		// If not writing, output to stdout
		if !write {
			fmt.Print(result.ConfigContent)
			return nil
		}

		// Create renderer for write result
		renderer, err := createRenderer(cmd)
		if err != nil {
			return err
		}

		// Create command result
		var message string
		if len(result.FilesWritten) == 0 {
			message = "No configuration files were written (files may already exist)."
		} else if len(result.FilesWritten) == 1 {
			message = fmt.Sprintf("Configuration written to %s", result.FilesWritten[0])
		} else {
			message = fmt.Sprintf("Configuration written to %d files", len(result.FilesWritten))
		}

		cmdResult := &types.CommandResult{
			Message: message,
		}

		if err := renderer.RenderResult(cmdResult); err != nil {
			return fmt.Errorf("failed to render results: %w", err)
		}

		return nil
	}
	return cmd
}

func newOffCmd() *cobra.Command {
	cmd := off.NewCommand()
	cmd.ValidArgsFunction = packNamesCompletion
	cmd.RunE = func(cmd *cobra.Command, args []string) error {
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
			Strs("packs", args).
			Msg("Turning off packs")

		// Turn off packs using the off command
		result, err := offpkg.OffPacks(offpkg.OffPacksOptions{
			DotfilesRoot: p.DotfilesRoot(),
			PackNames:    args,
			DryRun:       dryRun,
		})
		if err != nil {
			// Check if this is a pack not found error and provide detailed help
			var dodotErr *doerrors.DodotError
			if errors.As(err, &dodotErr) && dodotErr.Code == doerrors.ErrPackNotFound {
				return handlePackNotFoundError(dodotErr, p, "off")
			}
			return fmt.Errorf("failed to turn off packs: %w", err)
		}

		// Create renderer and display results
		renderer, err := createRenderer(cmd)
		if err != nil {
			return err
		}

		// After off operation, run status to get the current state
		statusResult, err := commands.StatusPacks(commands.StatusPacksOptions{
			DotfilesRoot: p.DotfilesRoot(),
			PackNames:    args,
			Paths:        p,
		})
		if err != nil {
			return fmt.Errorf("failed to get pack status: %w", err)
		}

		// Update command name to reflect off action
		statusResult.Command = "off"
		statusResult.DryRun = dryRun

		// Get pack names for the message
		// For off command, we want to list packs that had items removed
		packNames := make([]string, 0)
		for _, pack := range result.Packs {
			if len(pack.RemovedItems) > 0 {
				packNames = append(packNames, pack.Name)
			}
		}

		// Sort pack names for consistent output
		sort.Strings(packNames)

		// Create CommandResult with appropriate message
		var message string
		if result.TotalCleared == 0 {
			message = "" // No message if nothing was cleared
		} else {
			message = types.FormatCommandMessage("turned off", packNames)
		}

		cmdResult := &types.CommandResult{
			Message: message,
			Result:  statusResult,
		}

		if err := renderer.RenderResult(cmdResult); err != nil {
			return fmt.Errorf("failed to render results: %w", err)
		}

		// Display any errors encountered
		if len(result.Errors) > 0 {
			fmt.Println("\nErrors encountered:")
			for _, err := range result.Errors {
				fmt.Printf("  - %v\n", err)
			}
		}

		return nil
	}
	return cmd
}

func newOnCmd() *cobra.Command {
	cmd := on.NewCommand()
	cmd.ValidArgsFunction = packNamesCompletion
	cmd.RunE = func(cmd *cobra.Command, args []string) error {
		// Initialize paths (will show warning if using fallback)
		p, err := initPaths()
		if err != nil {
			return err
		}

		// Get flags
		dryRun, _ := cmd.Root().PersistentFlags().GetBool("dry-run")
		force, _ := cmd.Root().PersistentFlags().GetBool("force")
		noProvision, _ := cmd.Flags().GetBool("no-provision")
		provisionRerun, _ := cmd.Flags().GetBool("provision-rerun")

		log.Info().
			Str("dotfiles_root", p.DotfilesRoot()).
			Bool("dry_run", dryRun).
			Bool("force", force).
			Strs("packs", args).
			Msg("Turning on packs")

		// Turn on packs using the on command
		result, err := onpkg.OnPacks(onpkg.OnPacksOptions{
			DotfilesRoot:   p.DotfilesRoot(),
			PackNames:      args,
			DryRun:         dryRun,
			Force:          force,
			NoProvision:    noProvision,
			ProvisionRerun: provisionRerun,
		})
		if err != nil {
			// Check if this is a pack not found error and provide detailed help
			var dodotErr *doerrors.DodotError
			if errors.As(err, &dodotErr) && dodotErr.Code == doerrors.ErrPackNotFound {
				return handlePackNotFoundError(dodotErr, p, "on")
			}
			return fmt.Errorf("failed to turn on packs: %w", err)
		}

		// Create renderer and display results
		renderer, err := createRenderer(cmd)
		if err != nil {
			return err
		}

		// After on operation, run status to get the current state
		statusResult, err := commands.StatusPacks(commands.StatusPacksOptions{
			DotfilesRoot: p.DotfilesRoot(),
			PackNames:    args,
			Paths:        p,
		})
		if err != nil {
			return fmt.Errorf("failed to get pack status: %w", err)
		}

		// Update command name to reflect on action
		statusResult.Command = "on"
		statusResult.DryRun = dryRun

		// Get pack names for the message
		// For on command, we want to list packs that were actually processed
		packNames := make([]string, 0)
		processedPacks := make(map[string]bool)

		if result.LinkResult != nil {
			for packName := range result.LinkResult.PackResults {
				processedPacks[packName] = true
			}
		}
		if result.ProvisionResult != nil {
			for packName := range result.ProvisionResult.PackResults {
				processedPacks[packName] = true
			}
		}

		for packName := range processedPacks {
			packNames = append(packNames, packName)
		}

		// Sort pack names for consistent output
		sort.Strings(packNames)

		// Create CommandResult with appropriate message
		var message string
		if result.TotalDeployed == 0 {
			message = "" // No message if nothing was deployed
		} else {
			message = types.FormatCommandMessage("turned on", packNames)
		}

		cmdResult := &types.CommandResult{
			Message: message,
			Result:  statusResult,
		}

		if err := renderer.RenderResult(cmdResult); err != nil {
			return fmt.Errorf("failed to render results: %w", err)
		}

		// Display any errors encountered
		if len(result.Errors) > 0 {
			fmt.Println("\nErrors encountered:")
			for _, err := range result.Errors {
				fmt.Printf("  - %v\n", err)
			}
		}

		return nil
	}
	return cmd
}

// Completion command removed - use dodot-completions tool to generate shell completions
// The dodot-completions binary is built separately and used during the release process
