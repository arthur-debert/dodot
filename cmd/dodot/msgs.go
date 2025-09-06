package dodot

import (
	_ "embed"
	"strings"

	// Import messages from command packages
	"github.com/arthur-debert/dodot/cmd/dodot/commands/addignore"
	"github.com/arthur-debert/dodot/cmd/dodot/commands/adopt"
	"github.com/arthur-debert/dodot/cmd/dodot/commands/down"
	"github.com/arthur-debert/dodot/cmd/dodot/commands/fill"
	"github.com/arthur-debert/dodot/cmd/dodot/commands/genconfig"
	initcmd "github.com/arthur-debert/dodot/cmd/dodot/commands/init"
	"github.com/arthur-debert/dodot/cmd/dodot/commands/snippet"
	"github.com/arthur-debert/dodot/cmd/dodot/commands/status"
	"github.com/arthur-debert/dodot/cmd/dodot/commands/topics"
	"github.com/arthur-debert/dodot/cmd/dodot/commands/up"
)

// Re-export command messages for backward compatibility
var (
	// Status command
	MsgStatusShort    = status.MsgShort
	MsgErrStatusPacks = status.MsgErrStatusPacks

	// Init command
	MsgInitShort   = initcmd.MsgShort
	MsgErrInitPack = initcmd.MsgErrInitPack

	// Fill command
	MsgFillShort   = fill.MsgShort
	MsgErrFillPack = fill.MsgErrFillPack

	// Adopt command
	MsgAdoptShort    = adopt.MsgShort
	MsgErrAdoptFiles = adopt.MsgErrAdoptFiles

	// Add-ignore command
	MsgAddIgnoreShort = addignore.MsgShort
	MsgErrAddIgnore   = addignore.MsgErrAddIgnore

	// Topics command
	MsgTopicsShort = topics.MsgShort
	MsgTopicsLong  = topics.MsgLong

	// Snippet command
	MsgSnippetShort = snippet.MsgShort

	// Gen-config command
	MsgGenConfigShort = genconfig.MsgShort

	// Down command (not currently used, but imported to avoid "imported and not used" error)
	_ = down.MsgShort

	// Up command (not currently used, but imported to avoid "imported and not used" error)
	_ = up.MsgShort
)

// General messages (not command-specific)
const (
	// Root command description
	MsgRootShort = "A stateless dotfiles manager"

	// Error messages
	MsgErrInitPaths = "failed to initialize paths: %w"

	// Flag descriptions
	MsgFlagVerbose = "Increase verbosity (-v INFO, -vv DEBUG, -vvv TRACE)"
	MsgFlagDryRun  = "Preview changes without executing them"
	MsgFlagForce   = "Force execution of run-once handlers even if already executed"

	// Debug messages
	MsgDebugDotfilesRoot = "Debug: Using dotfiles root: %s (fallback=%v)\n"
)

// Embedded message files for general messages
var (
	//go:embed msgs/root-long.txt
	msgRootLongRaw string
	MsgRootLong    = strings.TrimSpace(msgRootLongRaw)

	//go:embed msgs/fallback-warning.txt
	msgFallbackWarningRaw string
	MsgFallbackWarning    = strings.TrimSpace(msgFallbackWarningRaw)

	//go:embed msgs/usage-template.txt
	msgUsageTemplateRaw string
	MsgUsageTemplate    = strings.TrimSpace(msgUsageTemplateRaw)
)
