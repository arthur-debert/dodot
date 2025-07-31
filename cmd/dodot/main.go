package main

import (
	"os"

	"github.com/arthur-debert/dodot/cmd/dodot/commands"

	// Import packages to ensure their init() functions are called for registration
	_ "github.com/arthur-debert/dodot/pkg/powerups"
	_ "github.com/arthur-debert/dodot/pkg/triggers"
)

func main() {
	rootCmd := commands.NewRootCmd()
	if err := rootCmd.Execute(); err != nil {
		os.Exit(1)
	}
}
