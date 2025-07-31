package main

import (
	"fmt"
	"os"

	"github.com/spf13/cobra/doc"

	"github.com/arthur-debert/dodot/cmd/dodot/commands"
	"github.com/arthur-debert/dodot/internal/version"

	// Import packages to ensure their init() functions are called for registration
	_ "github.com/arthur-debert/dodot/pkg/powerups"
	_ "github.com/arthur-debert/dodot/pkg/triggers"
)

func main() {
	rootCmd := commands.NewRootCmd()

	header := &doc.GenManHeader{
		Title:   "DODOT",
		Section: "1",
		Source:  "dodot " + version.Version,
		Manual:  "dodot manual",
	}

	err := doc.GenMan(rootCmd, header, os.Stdout)
	if err != nil {
		fmt.Fprintf(os.Stderr, "Error generating man page: %v\n", err)
		os.Exit(1)
	}
}
