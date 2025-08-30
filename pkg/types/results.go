package types

import (
	"strings"
	"time"
)

// OperationStatus defines the state of an operation execution
type OperationStatus string

const (
	StatusReady    OperationStatus = "ready"
	StatusSkipped  OperationStatus = "skipped"
	StatusConflict OperationStatus = "conflict"
	StatusError    OperationStatus = "error"
	StatusUnknown  OperationStatus = "unknown"
)

// ListPacksResult holds the result of the 'list' command.
type ListPacksResult struct {
	Packs []PackInfo `json:"packs"`
}

// PackInfo contains summary information about a single pack.
type PackInfo struct {
	Name string `json:"name"`
	Path string `json:"path"`
}

// DisplayResult is the top-level structure for commands that produce rich output.
// This replaces the old PackStatusResult and is used by status, deploy, and install commands.
type DisplayResult struct {
	Command   string        `json:"command"` // "status", "link", "provision"
	Packs     []DisplayPack `json:"packs"`
	DryRun    bool          `json:"dryRun"` // For deploy/install commands
	Timestamp time.Time     `json:"timestamp"`
}

// CommandResult wraps a DisplayResult with an optional message for unified output.
// This is used by all commands that alter pack state (link, unlink, provision, on, off, etc.)
// to provide a consistent output format:
//
//	<Optional Message>
//	<pack status representation>
type CommandResult struct {
	Message string         `json:"message,omitempty"` // Optional message like "The packs vim and git have been linked."
	Result  *DisplayResult `json:"result"`            // The pack status display
}

// FormatCommandMessage generates a standard message for command results.
// It handles pluralization and pack name listing appropriately.
// Returns empty string if there are no packs (message will be omitted).
//
// Examples:
//   - FormatCommandMessage("linked", []string{"vim", "git"}) -> "The packs vim and git have been linked."
//   - FormatCommandMessage("linked", []string{"vim"}) -> "The pack vim has been linked."
//   - FormatCommandMessage("linked", []string{}) -> ""
func FormatCommandMessage(verb string, packNames []string) string {
	if len(packNames) == 0 {
		return "" // No message for empty pack list
	}

	if len(packNames) == 1 {
		return "The pack " + packNames[0] + " has been " + verb + "."
	}

	// Multiple packs
	if len(packNames) == 2 {
		return "The packs " + packNames[0] + " and " + packNames[1] + " have been " + verb + "."
	}

	// More than 2 packs
	lastPack := packNames[len(packNames)-1]
	otherPacks := strings.Join(packNames[:len(packNames)-1], ", ")
	return "The packs " + otherPacks + ", and " + lastPack + " have been " + verb + "."
}

// DisplayPack represents a single pack for display.
type DisplayPack struct {
	Name      string        `json:"name"`
	Status    string        `json:"status"` // Aggregated: "alert", "success", "queue"
	Files     []DisplayFile `json:"files"`
	HasConfig bool          `json:"hasConfig"` // Pack has .dodot.toml
	IsIgnored bool          `json:"isIgnored"` // Pack has .dodotignore
}

// DisplayFile represents a single file within a pack for display.
type DisplayFile struct {
	Handler        string     `json:"handler"`
	Path           string     `json:"path"`
	Status         string     `json:"status"` // File-level: "success", "error", "queue", "config", "ignored"
	Message        string     `json:"message"`
	IsOverride     bool       `json:"isOverride"`     // File handler was overridden in .dodot.toml
	LastExecuted   *time.Time `json:"lastExecuted"`   // When operation was last executed
	HandlerSymbol  string     `json:"handlerSymbol"`  // Unicode symbol for the handler
	AdditionalInfo string     `json:"additionalInfo"` // Additional context (e.g., symlink target, shell type)
}

// GetPackStatus determines the pack-level status based on its files.
// Following the aggregation rules from the design:
// - If ANY file has ERROR status → Pack status is "alert"
// - If ANY file has WARNING status (but no errors) → Pack status is "partial"
// - If ALL files have SUCCESS status → Pack status is "success"
// - Empty pack or mixed states → Pack status is "queue"
func (dp *DisplayPack) GetPackStatus() string {
	if len(dp.Files) == 0 {
		return "queue"
	}

	hasError := false
	hasWarning := false
	allSuccess := true

	for _, file := range dp.Files {
		// Skip config files in status calculation
		if file.Status == "config" {
			continue
		}

		if file.Status == "error" {
			hasError = true
		}
		if file.Status == "warning" {
			hasWarning = true
		}
		if file.Status != "success" {
			allSuccess = false
		}
	}

	if hasError {
		return "alert" // Will be displayed with ALERT styling
	}
	if hasWarning {
		return "partial" // Has warnings but no errors
	}
	if allSuccess {
		return "success"
	}
	return "queue"
}

// FillResult holds the result of the 'fill' command.
type FillResult struct {
	PackName     string   `json:"packName"`
	FilesCreated []string `json:"filesCreated"`
	// Operations field removed - part of Operation layer elimination
}

// InitResult holds the result of the 'init' command.
type InitResult struct {
	PackName     string   `json:"packName"`
	Path         string   `json:"path"`
	FilesCreated []string `json:"filesCreated"`
	// Operations field removed - part of Operation layer elimination
}

// AddIgnoreResult holds the result of the 'add-ignore' command.
type AddIgnoreResult struct {
	PackName       string `json:"packName"`
	IgnoreFilePath string `json:"ignoreFilePath"`
	Created        bool   `json:"created"`
	AlreadyExisted bool   `json:"alreadyExisted"`
}

// GenConfigResult holds the result of the 'gen-config' command.
type GenConfigResult struct {
	ConfigContent string   `json:"configContent"`
	FilesWritten  []string `json:"filesWritten"`
}

// AdoptResult holds the result of the 'adopt' command.
type AdoptResult struct {
	PackName     string        `json:"packName"`
	AdoptedFiles []AdoptedFile `json:"adoptedFiles"`
}

// AdoptedFile represents a single file that was adopted.
type AdoptedFile struct {
	OriginalPath string `json:"originalPath"` // Original file path (e.g., ~/.gitconfig)
	NewPath      string `json:"newPath"`      // New path in pack (e.g., /path/to/dotfiles/git/gitconfig)
	SymlinkPath  string `json:"symlinkPath"`  // Symlink path (usually same as OriginalPath)
}

// GetHandlerSymbol returns the Unicode symbol for a given Handler
func GetHandlerSymbol(handlerName string) string {
	switch handlerName {
	case "symlink":
		return "➞"
	case "shell", "shell_add_path":
		return "⚙"
	case "path":
		return "+"
	case "homebrew":
		return "⚙"
	case "provision":
		return "×"
	default:
		return ""
	}
}

// GetHandlerAdditionalInfo returns a short description for the Handler type
func GetHandlerAdditionalInfo(handlerName string) string {
	switch handlerName {
	case "symlink":
		return "" // Will be filled with actual target
	case "shell", "shell_add_path":
		return "shell source"
	case "path":
		return "add to $PATH"
	case "homebrew":
		return "brew install"
	case "provision":
		return "run script"
	default:
		return ""
	}
}

// TruncateLeft truncates a string from the left side to fit within maxLen characters
// If the string is longer than maxLen, it returns "..." + the last (maxLen-3) characters
func TruncateLeft(s string, maxLen int) string {
	if len(s) <= maxLen {
		return s
	}
	if maxLen <= 3 {
		return "..."
	}
	return "..." + s[len(s)-(maxLen-3):]
}

// FormatSymlinkForDisplay formats a symlink path for display
// It replaces $HOME with ~ and truncates from the left if needed
func FormatSymlinkForDisplay(path string, homeDir string, maxLen int) string {
	// Replace home directory with ~ if path starts with it
	if homeDir != "" && strings.HasPrefix(path, homeDir) {
		path = "~" + path[len(homeDir):]
	}

	// Truncate from left if needed
	return TruncateLeft(path, maxLen)
}
