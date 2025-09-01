package genconfig

// Message constants
const (
	MsgShort   = "Generate default configuration file for a pack or root"
	MsgLong    = "Output the default configuration to stdout or write it to specified packs.\n\nWith no arguments and -w flag, writes to current directory.\nWith pack names and -w flag, writes to each pack directory."
	MsgExample = `  dodot gen-config                    # Output to stdout
  dodot gen-config -w                  # Write to ./.dodot.toml
  dodot gen-config vim git -w          # Write to vim/.dodot.toml and git/.dodot.toml`
)
