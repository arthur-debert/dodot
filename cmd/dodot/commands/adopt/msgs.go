package adopt

import (
	_ "embed"
	"strings"
)

// Message constants
const (
	MsgShort         = "Adopt existing files into a pack"
	MsgErrAdoptFiles = "failed to adopt files: %w"
)

// Embedded message files
var (
	//go:embed adopt-long.txt
	msgLongRaw string
	MsgLong    = strings.TrimSpace(msgLongRaw)

	//go:embed adopt-example.txt
	msgExampleRaw string
	MsgExample    = strings.TrimSpace(msgExampleRaw)
)
