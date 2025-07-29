package main

import (
	"os"
)

// Version information - set by ldflags during build
var (
	version = "dev"      // Set by goreleaser: -X main.version={{.Version}}
	commit  = "unknown"  // Set by goreleaser: -X main.commit={{.Commit}}
	date    = "unknown"  // Set by goreleaser: -X main.date={{.Date}}
)

func main() {
	if err := Execute(); err != nil {
		os.Exit(1)
	}
} 