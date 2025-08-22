package types

import (
	"fmt"
	"path/filepath"
	"strings"
)

// SentinelInfo contains information about a sentinel file
type SentinelInfo struct {
	// Path is the full path to the sentinel file
	Path string
	// Name is just the filename of the sentinel
	Name string
	// Dir is the directory containing the sentinel
	Dir string
}

// GetSentinelInfo returns standardized sentinel file information for an action
func (a *Action) GetSentinelInfo(paths Pather) (*SentinelInfo, error) {
	switch a.Type {
	case ActionTypeInstall:
		return a.getInstallSentinel(paths), nil
	case ActionTypeBrew:
		return a.getBrewSentinel(paths), nil
	default:
		return nil, fmt.Errorf("action type %s does not use sentinels", a.Type)
	}
}

// getInstallSentinel returns sentinel info for install scripts
func (a *Action) getInstallSentinel(paths Pather) *SentinelInfo {
	dir := filepath.Join(paths.DataDir(), "provision")

	// Standardized naming: pack_scriptname.sentinel
	scriptName := filepath.Base(a.Source)
	name := fmt.Sprintf("%s_%s.sentinel", a.Pack, scriptName)

	return &SentinelInfo{
		Dir:  dir,
		Name: name,
		Path: filepath.Join(dir, name),
	}
}

// getBrewSentinel returns sentinel info for Brewfiles
func (a *Action) getBrewSentinel(paths Pather) *SentinelInfo {
	dir := filepath.Join(paths.DataDir(), "homebrew")

	// Standardized naming: pack_Brewfile.sentinel
	// Always use "Brewfile" regardless of actual filename for consistency
	name := fmt.Sprintf("%s_Brewfile.sentinel", a.Pack)

	return &SentinelInfo{
		Dir:  dir,
		Name: name,
		Path: filepath.Join(dir, name),
	}
}

// GetDeployedSymlinkPath returns the path to the intermediate symlink for a link action
func (a *Action) GetDeployedSymlinkPath(paths Pather) (string, error) {
	if a.Type != ActionTypeLink {
		return "", fmt.Errorf("action type %s does not use deployed symlinks", a.Type)
	}

	// Symlinks go in deployed/symlink/ with the target basename
	return filepath.Join(paths.DataDir(), "deployed", "symlink", filepath.Base(a.Target)), nil
}

// GetDeployedPathPath returns the path to the deployed PATH directory symlink
func (a *Action) GetDeployedPathPath(paths Pather) (string, error) {
	if a.Type != ActionTypePathAdd {
		return "", fmt.Errorf("action type %s does not use deployed paths", a.Type)
	}

	// PATH entries go in deployed/path/ with pack_dirname format
	dirName := filepath.Base(a.Source)
	linkName := fmt.Sprintf("%s_%s", a.Pack, dirName)
	return filepath.Join(paths.DataDir(), "deployed", "path", linkName), nil
}

// GetDeployedShellProfilePath returns the path to the deployed shell profile symlink
func (a *Action) GetDeployedShellProfilePath(paths Pather) (string, error) {
	if a.Type != ActionTypeShellSource {
		return "", fmt.Errorf("action type %s does not use deployed shell profiles", a.Type)
	}

	// Shell profiles go in deployed/shell_profile/ with pack_scriptname.sh format
	scriptBase := strings.TrimSuffix(filepath.Base(a.Source), filepath.Ext(a.Source))
	linkName := fmt.Sprintf("%s_%s.sh", a.Pack, scriptBase)
	return filepath.Join(paths.DataDir(), "deployed", "shell_profile", linkName), nil
}
