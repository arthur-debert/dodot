package operations

// Handler name constants to avoid hardcoded strings throughout the codebase.
// These constants centralize handler identification and reduce coupling.
// They will be used during the migration phases and can be removed once
// all handlers are migrated to the new architecture.
const (
	// Configuration handlers
	HandlerSymlink      = "symlink"
	HandlerPath         = "path"
	HandlerShell        = "shell"
	HandlerShellProfile = "shell_profile"

	// Provisioning handlers
	HandlerInstall  = "install"
	HandlerHomebrew = "homebrew"
)

// handlerNames provides a complete list of all handler names.
// This can be used for validation and iteration.
var handlerNames = []string{
	HandlerSymlink,
	HandlerPath,
	HandlerShell,
	HandlerShellProfile,
	HandlerInstall,
	HandlerHomebrew,
}

// IsValidHandlerName checks if a handler name is recognized.
func IsValidHandlerName(name string) bool {
	for _, h := range handlerNames {
		if h == name {
			return true
		}
	}
	return false
}
