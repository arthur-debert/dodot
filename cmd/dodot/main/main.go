package main

import (
	"fmt"
	"os"

	"github.com/arthur-debert/dodot/cmd/dodot"
	"github.com/arthur-debert/dodot/pkg/output/styles"

	// Import packages to ensure their init() functions are called for registration
	_ "github.com/arthur-debert/dodot/pkg/handlers"
	_ "github.com/arthur-debert/dodot/pkg/triggers"
)

func main() {
	rootCmd := dodot.NewRootCmd()
	if err := rootCmd.Execute(); err != nil {
		// Print the error in red
		errorStyle := styles.GetStyle("Error")
		fmt.Fprintln(os.Stderr, errorStyle.Render(fmt.Sprintf("Error: %v", err)))

		// Show the full help for the command that failed
		// ExecuteContext sets cmd.Context() to the command that failed
		fmt.Fprintln(os.Stderr)
		_ = rootCmd.Help()

		os.Exit(1)
	}
}
