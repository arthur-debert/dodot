// Package constants provides shared constants used across the dodot codebase.
// This package has no dependencies to avoid circular imports.
package constants

// CoreUnixExceptions defines files/dirs that always deploy to $HOME.
// These are typically security-critical or shell-expected locations.
// The key is the first path segment in the pack (e.g., "ssh" for "ssh/config").
// Release C: Layer 2 - Exception List
var CoreUnixExceptions = map[string]bool{
	"ssh":       true, // .ssh/ - security critical, expects $HOME
	"gnupg":     true, // .gnupg/ - security critical, expects $HOME
	"aws":       true, // .aws/ - credentials, expects $HOME
	"kube":      true, // .kube/ - kubernetes config
	"docker":    true, // .docker/ - docker config
	"gitconfig": true, // .gitconfig - git expects in $HOME
	"bashrc":    true, // .bashrc - shell expects in $HOME
	"zshrc":     true, // .zshrc - shell expects in $HOME
	"profile":   true, // .profile - shell expects in $HOME
}
