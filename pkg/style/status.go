package style

import (
	"fmt"
	"strings"

	"github.com/pterm/pterm"
)

// Status types for files and packs
type Status string

const (
	StatusSuccess Status = "success" // Deployed/installed successfully
	StatusError   Status = "error"   // Deploy/install failed
	StatusQueue   Status = "queue"   // To be deployed/installed
	StatusAlert   Status = "alert"   // Pack has errors (pack-level only)
	StatusIgnored Status = "ignored" // Directory is ignored
	StatusConfig  Status = "config"  // Config file found
)

// PowerUpVerbs defines past and future tense verbs for each power-up type
var PowerUpVerbs = map[string]struct {
	Past   string
	Future string
}{
	"symlink":  {Past: "linked to", Future: "will be linked to"},
	"profile":  {Past: "included in", Future: "to be included in"},
	"homebrew": {Past: "executed on", Future: "to be installed"},
	"path":     {Past: "added to", Future: "to be added to"},
	"install":  {Past: "executed during installation on", Future: "to be executed"},
	"config":   {Past: "found", Future: "found"}, // Config is always present tense
}

// StatusStyle returns the appropriate pterm style for a status
func StatusStyle(status Status) *pterm.Style {
	switch status {
	case StatusSuccess:
		return pterm.NewStyle(pterm.BgGreen, pterm.FgWhite)
	case StatusError:
		return pterm.NewStyle(pterm.BgRed, pterm.FgWhite)
	case StatusQueue:
		return pterm.NewStyle(pterm.BgYellow, pterm.FgBlack)
	case StatusAlert:
		return pterm.NewStyle(pterm.BgRed, pterm.FgWhite, pterm.Bold)
	case StatusConfig:
		return pterm.NewStyle(pterm.FgCyan)
	default:
		return pterm.NewStyle(pterm.FgGray)
	}
}

// FileStatus represents the status of a single file in a pack
type FileStatus struct {
	PowerUp    string // Power-up type (symlink, profile, etc.)
	FilePath   string // File path relative to pack
	Status     Status // Current status
	Target     string // Target path (for symlinks, etc.)
	Date       string // Date of execution (for past tense)
	IsOverride bool   // True if filename was overridden in config
}

// PackStatus represents the status of an entire pack
type PackStatus struct {
	Name      string
	Status    Status // Aggregated status
	HasConfig bool
	IsIgnored bool
	Files     []FileStatus
}

// RenderFileStatus renders a single file status line
func RenderFileStatus(fs FileStatus) string {
	// Format power-up name with appropriate width
	powerUpName := fmt.Sprintf("%-10s", fs.PowerUp)

	// Apply status color to power-up name
	styledPowerUp := StatusStyle(fs.Status).Sprint(powerUpName)

	// Format file path (with asterisk if overridden)
	filePath := fs.FilePath
	if fs.IsOverride {
		filePath = "*" + filePath
	}
	filePath = fmt.Sprintf("%-15s", filePath)

	// Build status message
	var statusMsg string
	if fs.PowerUp == "config" {
		statusMsg = "dodot config file found"
	} else if verbs, ok := PowerUpVerbs[fs.PowerUp]; ok {
		switch fs.Status {
		case StatusSuccess:
			// Past tense with date if available
			statusMsg = fmt.Sprintf("%s %s", verbs.Past, fs.Target)
			if fs.Date != "" {
				statusMsg += fmt.Sprintf(" (%s)", fs.Date)
			}
		case StatusQueue:
			// Future tense
			statusMsg = fmt.Sprintf("%s %s", verbs.Future, fs.Target)
		case StatusError:
			// Error state
			statusMsg = fmt.Sprintf("failed to %s %s", strings.ToLower(fs.PowerUp), fs.Target)
		}
	}

	return fmt.Sprintf("    %s : %s : %s", styledPowerUp, filePath, statusMsg)
}

// RenderPackStatus renders a complete pack status
func RenderPackStatus(ps PackStatus) string {
	var result strings.Builder

	// Pack header with status
	packHeader := ps.Name + ":"
	switch ps.Status {
	case StatusAlert:
		packHeader = StatusStyle(StatusAlert).Sprint(packHeader)
	case StatusIgnored:
		packHeader = MutedStyle.Sprint(packHeader)
	}
	result.WriteString(packHeader + "\n")

	// Special case: ignored directory
	if ps.IsIgnored {
		result.WriteString("    .dodotignore : dodot is ignoring this dir\n")
		return result.String()
	}

	// Config file if present
	if ps.HasConfig {
		configStatus := FileStatus{
			PowerUp:  "config",
			FilePath: ".dodot.toml",
			Status:   StatusConfig,
		}
		result.WriteString(RenderFileStatus(configStatus) + "\n")
	}

	// File statuses
	for _, fs := range ps.Files {
		result.WriteString(RenderFileStatus(fs) + "\n")
	}

	return strings.TrimRight(result.String(), "\n")
}

// AggregatePackStatus determines the overall status of a pack based on its files
func AggregatePackStatus(files []FileStatus) Status {
	hasError := false
	allSuccess := true
	allQueue := true

	for _, f := range files {
		switch f.Status {
		case StatusError:
			hasError = true
			allSuccess = false
			allQueue = false
		case StatusQueue:
			allSuccess = false
		case StatusSuccess:
			allQueue = false
		}
	}

	if hasError {
		return StatusAlert
	} else if allSuccess && len(files) > 0 {
		return StatusSuccess
	} else if allQueue && len(files) > 0 {
		return StatusQueue
	}

	// Mixed state defaults to queue
	return StatusQueue
}
