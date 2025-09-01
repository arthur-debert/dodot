package status

import (
	_ "embed"
	"strings"
)

// Message constants
const (
	MsgShort          = "Show pack status(es)"
	MsgErrStatusPacks = "failed to get pack status: %w"
)

// Embedded message files
var (
	//go:embed status-long.txt
	msgLongRaw string
	MsgLong    = strings.TrimSpace(msgLongRaw)

	//go:embed status-example.txt
	msgExampleRaw string
	MsgExample    = strings.TrimSpace(msgExampleRaw)
)
