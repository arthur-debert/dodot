package version

// Build information set by ldflags
var (
	Version = "dev"     // Set by goreleaser: -X github.com/arthur-debert/dodot/internal/version.Version={{.Version}}
	Commit  = "unknown" // Set by goreleaser: -X github.com/arthur-debert/dodot/internal/version.Commit={{.Commit}}
	Date    = "unknown" // Set by goreleaser: -X github.com/arthur-debert/dodot/internal/version.Date={{.Date}}
)
