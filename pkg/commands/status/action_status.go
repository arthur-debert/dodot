package status

import (
	"fmt"
	"os"
	"path/filepath"

	"github.com/arthur-debert/dodot/pkg/types"
)

// getActionStatus checks the deployment status for a specific Action
func getActionStatus(action types.Action, dataStore types.DataStore) (types.Status, error) {
	switch a := action.(type) {
	case *types.LinkAction:
		return dataStore.GetSymlinkStatus(a.PackName, a.SourceFile)

	case *types.AddToPathAction:
		return dataStore.GetPathStatus(a.PackName, a.DirPath)

	case *types.AddToShellProfileAction:
		return dataStore.GetShellProfileStatus(a.PackName, a.ScriptPath)

	case *types.RunScriptAction:
		return dataStore.GetProvisioningStatus(a.PackName, a.SentinelName, a.Checksum)

	case *types.BrewAction:
		return dataStore.GetBrewStatus(a.PackName, a.BrewfilePath, a.Checksum)

	default:
		return types.Status{
			State:   types.StatusStateUnknown,
			Message: fmt.Sprintf("unknown action type: %T", action),
		}, nil
	}
}

// getActionFilePath extracts the file path from an Action for display
func getActionFilePath(action types.Action) string {
	switch a := action.(type) {
	case *types.LinkAction:
		return filepath.Base(a.SourceFile)
	case *types.AddToPathAction:
		return filepath.Base(a.DirPath)
	case *types.AddToShellProfileAction:
		return filepath.Base(a.ScriptPath)
	case *types.RunScriptAction:
		return filepath.Base(a.ScriptPath)
	case *types.BrewAction:
		return filepath.Base(a.BrewfilePath)
	default:
		return "unknown"
	}
}

// getActionHandler returns the handler name for an Action
func getActionHandler(action types.Action) string {
	switch action.(type) {
	case *types.LinkAction:
		return "symlink"
	case *types.AddToPathAction:
		return "path"
	case *types.AddToShellProfileAction:
		return "shell_profile"
	case *types.RunScriptAction:
		return "provision"
	case *types.BrewAction:
		return "homebrew"
	default:
		return "unknown"
	}
}

// getActionAdditionalInfo extracts additional display information from an Action
func getActionAdditionalInfo(action types.Action) string {
	switch a := action.(type) {
	case *types.LinkAction:
		// For symlinks, show the target path formatted with ~ for home
		homeDir := os.Getenv("HOME")
		return types.FormatSymlinkForDisplay(a.TargetFile, homeDir, 46)
	case *types.AddToPathAction:
		return "add to $PATH"
	case *types.AddToShellProfileAction:
		return "shell source"
	case *types.RunScriptAction:
		return "run script"
	case *types.BrewAction:
		return "brew install"
	default:
		return ""
	}
}

// statusStateToDisplayStatus converts internal status states to display status strings
func statusStateToDisplayStatus(state types.StatusState) string {
	switch state {
	case types.StatusStateReady, types.StatusStateSuccess:
		return "success"
	case types.StatusStateMissing:
		return "queue"
	case types.StatusStatePending:
		return "queue"
	case types.StatusStateError:
		return "error"
	case types.StatusStateIgnored:
		return "ignored"
	case types.StatusStateConfig:
		return "config"
	default:
		return "unknown"
	}
}
