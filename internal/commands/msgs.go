package commands

import (
	_ "embed"
	"strings"
)

// Short messages (one-liners)
const (
	// Command descriptions
	MsgRootShort    = "A stateless dotfiles manager"
	MsgVersionShort = "Print version information"
	MsgVersionLong  = "Print detailed version information including commit hash and build date"
	MsgDeployShort  = "Deploy dotfiles to the system"
	MsgInstallShort = "Install and deploy dotfiles to the system"
	MsgListShort    = "List all available packs"
	MsgListLong     = "List displays all packs found in your DOTFILES_ROOT directory."
	MsgStatusShort  = "Show deployment status of packs"
	MsgInitShort    = "Create a new pack with template files"
	MsgFillShort    = "Add placeholder files to an existing pack"

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
	MsgPackStatusFormat  = "\n%s:\n"
	MsgPowerUpStatus     = "  %s: %s"
	MsgPowerUpDesc       = " - %s"

	// Version output
	MsgVersionFormat = "dodot version %s\n"
	MsgCommitFormat  = "Commit: %s\n"
	MsgBuiltFormat   = "Built:  %s\n"

	// Error messages
	MsgErrInitPaths     = "failed to initialize paths: %w"
	MsgErrDeployPacks   = "failed to deploy packs: %w"
	MsgErrInstallPacks  = "failed to install packs: %w"
	MsgErrListPacks     = "failed to list packs: %w"
	MsgErrStatusPacks   = "failed to get pack status: %w"
	MsgErrInitPack      = "failed to initialize pack: %w"
	MsgErrFillPack      = "failed to fill pack: %w"

	// Flag descriptions
	MsgFlagVerbose = "Increase verbosity (-v INFO, -vv DEBUG, -vvv TRACE)"
	MsgFlagDryRun  = "Preview changes without executing them"
	MsgFlagForce   = "Force execution of run-once power-ups even if already executed"
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

	//go:embed msgs/deploy-long.txt
	msgDeployLongRaw string
	MsgDeployLong    = strings.TrimSpace(msgDeployLongRaw)

	//go:embed msgs/deploy-example.txt
	msgDeployExampleRaw string
	MsgDeployExample    = strings.TrimSpace(msgDeployExampleRaw)

	//go:embed msgs/install-long.txt
	msgInstallLongRaw string
	MsgInstallLong    = strings.TrimSpace(msgInstallLongRaw)

	//go:embed msgs/install-example.txt
	msgInstallExampleRaw string
	MsgInstallExample    = strings.TrimSpace(msgInstallExampleRaw)

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

	//go:embed msgs/fallback-warning.txt
	msgFallbackWarningRaw string
	MsgFallbackWarning    = strings.TrimSpace(msgFallbackWarningRaw)
)