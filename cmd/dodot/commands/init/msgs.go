package init

import (
	_ "embed"
	"strings"
)

// Message constants
const (
	MsgShort       = "Create a new pack with template files"
	MsgFlagType    = "Type of pack to create (basic, shell, vim, etc.)"
	MsgErrInitPack = "failed to initialize pack: %w"
)

// Embedded message files
var (
	//go:embed init-long.txt
	msgLongRaw string
	MsgLong    = strings.TrimSpace(msgLongRaw)

	//go:embed init-example.txt
	msgExampleRaw string
	MsgExample    = strings.TrimSpace(msgExampleRaw)
)
