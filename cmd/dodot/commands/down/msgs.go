package down

// Message constants
const (
	MsgShort = "Remove and uninstall pack(s)"
	MsgLong  = `The 'down' command is dodot's primary removal command. It completely removes pack deployments:
  - Removes all symlinks
  - Clears shell integrations and PATH entries
  - Removes all handler state from the data directory

Note: This is a complete removal - no state is saved for restoration. Files in your dotfiles repository are never touched.`
	MsgExample = `  # Remove all pack deployments
  dodot down
  
  # Remove specific packs
  dodot down vim zsh
  
  # Preview what will be removed
  dodot down --dry-run vim`
)
