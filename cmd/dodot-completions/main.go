package main

import (
	"fmt"
	"os"

	"github.com/arthur-debert/dodot/internal/commands"

	// Import packages to ensure their init() functions are called for registration
	_ "github.com/arthur-debert/dodot/pkg/powerups"
	_ "github.com/arthur-debert/dodot/pkg/triggers"
)

func main() {
	if len(os.Args) < 2 {
		fmt.Fprintf(os.Stderr, "Usage: %s <bash|zsh|fish|powershell>\n", os.Args[0])
		os.Exit(1)
	}

	shell := os.Args[1]
	rootCmd := commands.NewRootCmd()

	var err error
	switch shell {
	case "bash":
		err = rootCmd.GenBashCompletionV2(os.Stdout, true)
	case "zsh":
		err = rootCmd.GenZshCompletion(os.Stdout)
	case "fish":
		err = rootCmd.GenFishCompletion(os.Stdout, true)
	case "powershell":
		err = rootCmd.GenPowerShellCompletionWithDesc(os.Stdout)
	default:
		fmt.Fprintf(os.Stderr, "Unknown shell: %s\n", shell)
		fmt.Fprintf(os.Stderr, "Supported shells: bash, zsh, fish, powershell\n")
		os.Exit(1)
	}

	if err != nil {
		fmt.Fprintf(os.Stderr, "Error generating %s completion: %v\n", shell, err)
		os.Exit(1)
	}
}
