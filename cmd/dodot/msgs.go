package dodot

import (
	_ "embed"
	"strings"
)

// Short messages (one-liners)
const (
	// Command descriptions
	MsgRootShort       = "A stateless dotfiles manager"
	MsgLinkShort       = "Link dotfiles to the system"
	MsgProvisionShort  = "Provision and link dotfiles to the system"
	MsgListShort       = "List all available packs"
	MsgListLong        = "List displays all packs found in your DOTFILES_ROOT directory."
	MsgStatusShort     = "Show deployment status of packs"
	MsgUnlinkShort     = "Unlink specified packs"
	MsgInitShort       = "Create a new pack with template files"
	MsgFillShort       = "Add placeholder files to an existing pack"
	MsgAddIgnoreShort  = "Create a .dodotignore file to ignore a pack"
	MsgAdoptShort      = "Adopt existing files into a pack"
	MsgTopicsShort     = "Display available documentation topics"
	MsgTopicsLong      = "Display a list of all available help topics that provide additional documentation beyond command help."
	MsgCompletionShort = "Generate shell completion script"
	MsgSnippetShort    = "Output shell integration snippet"

	// Status messages
	MsgDryRunNotice      = "\nDRY RUN MODE - No changes were made"
	MsgNoOperations      = "No operations needed."
	MsgOperationsFormat  = "\nPerformed %d operations:\n"
	MsgOperationItem     = "  ✓ %s\n"
	MsgNoPacksFound      = "No packs found."
	MsgAvailablePacks    = "Available packs:"
	MsgPackItem          = "  %s\n"
	MsgPackCreatedFormat = "Created pack '%s' with the following files:\n"
	MsgPackFilledFormat  = "Added the following files to pack '%s':\n"
	MsgPackHasAllFiles   = "Pack '%s' already has all standard files.\n"
	MsgIgnoreFileCreated = "Created .dodotignore file in pack '%s'\n"
	MsgIgnoreFileExists  = "Pack '%s' already has a .dodotignore file\n"
	MsgFileAdopted       = "✔ Moving '%s' to '%s'\n"
	MsgSymlinkCreated    = "✔ Creating symlink: '%s' -> '%s'\n"
	MsgAdoptSuccess      = "✨ Success! %d file(s) are now managed by dodot in the '%s' pack.\n"
	MsgNoFilesAdopted    = "No files were adopted.\n"
	MsgPackStatusFormat  = "\n%s:\n"
	MsgHandlerStatus     = "  %s: %s"
	MsgHandlerDesc       = " - %s"

	// Error messages
	MsgErrInitPaths      = "failed to initialize paths: %w"
	MsgErrLinkPacks      = "failed to link packs: %w"
	MsgErrProvisionPacks = "failed to provision packs: %w"
	MsgErrListPacks      = "failed to list packs: %w"
	MsgErrStatusPacks    = "failed to get pack status: %w"
	MsgErrInitPack       = "failed to initialize pack: %w"
	MsgErrFillPack       = "failed to fill pack: %w"
	MsgErrAddIgnore      = "failed to add ignore file: %w"
	MsgErrAdoptFiles     = "failed to adopt files: %w"
	MsgErrUnlinkPacks    = "failed to unlink packs: %w"

	// Flag descriptions
	MsgFlagVerbose = "Increase verbosity (-v INFO, -vv DEBUG, -vvv TRACE)"
	MsgFlagDryRun  = "Preview changes without executing them"
	MsgFlagForce   = "Force execution of run-once handlers even if already executed"
	MsgFlagType    = "Type of pack to create (basic, shell, vim, etc.)"

	// Debug messages
	MsgDebugDotfilesRoot = "Debug: Using dotfiles root: %s (fallback=%v)\n"
	MsgDebugUsingCwd     = "Using current directory: %s\n"
)

// Long messages from embedded files
var (
	//go:embed msgs/root-long.txt
	msgRootLongRaw string
	MsgRootLong    = strings.TrimSpace(msgRootLongRaw)

	//go:embed msgs/link-long.txt
	msgLinkLongRaw string
	MsgLinkLong    = strings.TrimSpace(msgLinkLongRaw)

	//go:embed msgs/link-example.txt
	msgLinkExampleRaw string
	MsgLinkExample    = strings.TrimSpace(msgLinkExampleRaw)

	//go:embed msgs/provision-long.txt
	msgProvisionLongRaw string
	MsgProvisionLong    = strings.TrimSpace(msgProvisionLongRaw)

	//go:embed msgs/provision-example.txt
	msgProvisionExampleRaw string
	MsgProvisionExample    = strings.TrimSpace(msgProvisionExampleRaw)

	//go:embed msgs/list-example.txt
	msgListExampleRaw string
	MsgListExample    = strings.TrimSpace(msgListExampleRaw)

	//go:embed msgs/status-long.txt
	msgStatusLongRaw string
	MsgStatusLong    = strings.TrimSpace(msgStatusLongRaw)

	//go:embed msgs/status-example.txt
	msgStatusExampleRaw string
	MsgStatusExample    = strings.TrimSpace(msgStatusExampleRaw)

	//go:embed msgs/init-long.txt
	msgInitLongRaw string
	MsgInitLong    = strings.TrimSpace(msgInitLongRaw)

	//go:embed msgs/init-example.txt
	msgInitExampleRaw string
	MsgInitExample    = strings.TrimSpace(msgInitExampleRaw)

	//go:embed msgs/fill-long.txt
	msgFillLongRaw string
	MsgFillLong    = strings.TrimSpace(msgFillLongRaw)

	//go:embed msgs/fill-example.txt
	msgFillExampleRaw string
	MsgFillExample    = strings.TrimSpace(msgFillExampleRaw)

	//go:embed msgs/addignore-long.txt
	msgAddIgnoreLongRaw string
	MsgAddIgnoreLong    = strings.TrimSpace(msgAddIgnoreLongRaw)

	//go:embed msgs/addignore-example.txt
	msgAddIgnoreExampleRaw string
	MsgAddIgnoreExample    = strings.TrimSpace(msgAddIgnoreExampleRaw)

	//go:embed msgs/adopt-long.txt
	msgAdoptLongRaw string
	MsgAdoptLong    = strings.TrimSpace(msgAdoptLongRaw)

	//go:embed msgs/adopt-example.txt
	msgAdoptExampleRaw string
	MsgAdoptExample    = strings.TrimSpace(msgAdoptExampleRaw)

	//go:embed msgs/unlink-long.txt
	msgUnlinkLongRaw string
	MsgUnlinkLong    = strings.TrimSpace(msgUnlinkLongRaw)

	//go:embed msgs/unlink-example.txt
	msgUnlinkExampleRaw string
	MsgUnlinkExample    = strings.TrimSpace(msgUnlinkExampleRaw)

	//go:embed msgs/fallback-warning.txt
	msgFallbackWarningRaw string
	MsgFallbackWarning    = strings.TrimSpace(msgFallbackWarningRaw)

	//go:embed msgs/usage-template.txt
	msgUsageTemplateRaw string
	MsgUsageTemplate    = strings.TrimSpace(msgUsageTemplateRaw)

	//go:embed msgs/completion-long.txt
	msgCompletionLongRaw string
	MsgCompletionLong    = strings.TrimSpace(msgCompletionLongRaw)

	//go:embed msgs/snippet-long.txt
	msgSnippetLongRaw string
	MsgSnippetLong    = strings.TrimSpace(msgSnippetLongRaw)

	//go:embed msgs/snippet-example.txt
	msgSnippetExampleRaw string
	MsgSnippetExample    = strings.TrimSpace(msgSnippetExampleRaw)
)
