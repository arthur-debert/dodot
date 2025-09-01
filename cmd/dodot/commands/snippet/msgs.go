package snippet

import (
	_ "embed"
	"strings"
)

// Message constants
const (
	MsgShort = "Output shell integration snippet for inclusion in profile"
)

// Embedded message files
var (
	//go:embed snippet-long.txt
	msgLongRaw string
	MsgLong    = strings.TrimSpace(msgLongRaw)

	//go:embed snippet-example.txt
	msgExampleRaw string
	MsgExample    = strings.TrimSpace(msgExampleRaw)
)
