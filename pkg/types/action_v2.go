package types

import (
	"fmt"
)

// DataStore is a minimal interface to avoid circular dependencies.
// The actual implementation is in pkg/datastore.
type DataStore interface {
	Link(pack, sourceFile string) (intermediateLinkPath string, err error)
	Unlink(pack, sourceFile string) error
	AddToPath(pack, dirPath string) error
	AddToShellProfile(pack, scriptPath string) error
	RecordProvisioning(pack, sentinelName, checksum string) error
	NeedsProvisioning(pack, sentinelName, checksum string) (bool, error)
	GetStatus(pack, sourceFile string) (Status, error)
	// Handler-specific status methods
	GetSymlinkStatus(pack, sourceFile string) (Status, error)
	GetPathStatus(pack, dirPath string) (Status, error)
	GetShellProfileStatus(pack, scriptPath string) (Status, error)
	GetProvisioningStatus(pack, sentinelName, currentChecksum string) (Status, error)
	GetBrewStatus(pack, brewfilePath, currentChecksum string) (Status, error)
}

// ActionV2 is the base interface for all actions in the new architecture.
// Each action is self-contained and knows how to execute itself.
type ActionV2 interface {
	// Execute performs the action using the provided datastore
	Execute(store DataStore) error

	// Description returns a human-readable description of the action
	Description() string

	// Pack returns the name of the pack this action belongs to
	Pack() string
}

// LinkingAction marker interface for actions that are idempotent and fast
type LinkingAction interface {
	ActionV2
	isLinkingAction()
}

// ProvisioningAction marker interface for actions that have side effects
type ProvisioningAction interface {
	ActionV2
	isProvisioningAction()
}

// LinkAction represents creating a symlink from a source file to a target location
type LinkAction struct {
	PackName   string
	SourceFile string // Path within the pack
	TargetFile string // Final destination (e.g., ~/.vimrc)
}

func (a *LinkAction) Execute(store DataStore) error {
	_, err := store.Link(a.PackName, a.SourceFile)
	if err != nil {
		return fmt.Errorf("failed to create intermediate link: %w", err)
	}
	return nil
}

func (a *LinkAction) Description() string {
	return fmt.Sprintf("Link %s to %s", a.SourceFile, a.TargetFile)
}

func (a *LinkAction) Pack() string {
	return a.PackName
}

func (a *LinkAction) isLinkingAction() {}

// UnlinkAction represents removing a symlink
type UnlinkAction struct {
	PackName   string
	SourceFile string
}

func (a *UnlinkAction) Execute(store DataStore) error {
	return store.Unlink(a.PackName, a.SourceFile)
}

func (a *UnlinkAction) Description() string {
	return fmt.Sprintf("Unlink %s", a.SourceFile)
}

func (a *UnlinkAction) Pack() string {
	return a.PackName
}

func (a *UnlinkAction) isLinkingAction() {}

// AddToPathAction represents adding a directory to the shell PATH
type AddToPathAction struct {
	PackName string
	DirPath  string
}

func (a *AddToPathAction) Execute(store DataStore) error {
	return store.AddToPath(a.PackName, a.DirPath)
}

func (a *AddToPathAction) Description() string {
	return fmt.Sprintf("Add %s to PATH", a.DirPath)
}

func (a *AddToPathAction) Pack() string {
	return a.PackName
}

func (a *AddToPathAction) isLinkingAction() {}

// AddToShellProfileAction represents adding a script to be sourced in the shell
type AddToShellProfileAction struct {
	PackName   string
	ScriptPath string
}

func (a *AddToShellProfileAction) Execute(store DataStore) error {
	return store.AddToShellProfile(a.PackName, a.ScriptPath)
}

func (a *AddToShellProfileAction) Description() string {
	return fmt.Sprintf("Add %s to shell profile", a.ScriptPath)
}

func (a *AddToShellProfileAction) Pack() string {
	return a.PackName
}

func (a *AddToShellProfileAction) isLinkingAction() {}

// RunScriptAction represents running a provisioning script
type RunScriptAction struct {
	PackName     string
	ScriptPath   string
	Checksum     string
	SentinelName string
}

func (a *RunScriptAction) Execute(store DataStore) error {
	// Check if we need to run this script
	needs, err := store.NeedsProvisioning(a.PackName, a.SentinelName, a.Checksum)
	if err != nil {
		return fmt.Errorf("failed to check provisioning status: %w", err)
	}

	if !needs {
		// Already provisioned with same checksum
		return nil
	}

	// The actual script execution is handled by the executor
	// The action just manages the sentinel file through the datastore

	// Record that we're about to run this
	// The executor will actually run the script and then we'll record completion
	return nil
}

func (a *RunScriptAction) Description() string {
	return fmt.Sprintf("Run provisioning script %s", a.ScriptPath)
}

func (a *RunScriptAction) Pack() string {
	return a.PackName
}

func (a *RunScriptAction) isProvisioningAction() {}

// RecordProvisioningAction represents marking a provisioning action as complete
type RecordProvisioningAction struct {
	PackName     string
	SentinelName string
	Checksum     string
}

func (a *RecordProvisioningAction) Execute(store DataStore) error {
	return store.RecordProvisioning(a.PackName, a.SentinelName, a.Checksum)
}

func (a *RecordProvisioningAction) Description() string {
	return fmt.Sprintf("Record provisioning complete for %s", a.SentinelName)
}

func (a *RecordProvisioningAction) Pack() string {
	return a.PackName
}

func (a *RecordProvisioningAction) isProvisioningAction() {}

// BrewAction represents running a Brewfile through Homebrew
type BrewAction struct {
	PackName     string
	BrewfilePath string
	Checksum     string
}

func (a *BrewAction) Execute(store DataStore) error {
	// Check if we need to run this Brewfile
	sentinelName := fmt.Sprintf("homebrew-%s.sentinel", a.PackName)
	needs, err := store.NeedsProvisioning(a.PackName, sentinelName, a.Checksum)
	if err != nil {
		return fmt.Errorf("failed to check provisioning status: %w", err)
	}

	if !needs {
		// Already provisioned with same checksum
		return nil
	}

	// The actual brew bundle execution is handled by the executor
	// The action just manages the sentinel file through the datastore
	return nil
}

func (a *BrewAction) Description() string {
	return fmt.Sprintf("Install Homebrew packages from %s", a.BrewfilePath)
}

func (a *BrewAction) Pack() string {
	return a.PackName
}

func (a *BrewAction) isProvisioningAction() {}
