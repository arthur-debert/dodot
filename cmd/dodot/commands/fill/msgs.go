package fill

import (
	_ "embed"
	"strings"
)

// Message constants
const (
	MsgShort       = "Add placeholder files to an existing pack"
	MsgErrFillPack = "failed to fill pack: %w"
)

// Embedded message files
var (
	//go:embed fill-long.txt
	msgLongRaw string
	MsgLong    = strings.TrimSpace(msgLongRaw)

	//go:embed fill-example.txt
	msgExampleRaw string
	MsgExample    = strings.TrimSpace(msgExampleRaw)
)
