package addignore

import (
	_ "embed"
	"strings"
)

// Message constants
const (
	MsgShort        = "Create a .dodotignore file to ignore a pack"
	MsgErrAddIgnore = "failed to add ignore file: %w"
)

// Embedded message files
var (
	//go:embed addignore-long.txt
	msgLongRaw string
	MsgLong    = strings.TrimSpace(msgLongRaw)

	//go:embed addignore-example.txt
	msgExampleRaw string
	MsgExample    = strings.TrimSpace(msgExampleRaw)
)
