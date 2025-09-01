package off

// Message constants
const (
	MsgShort = "Remove and uninstall pack(s)"
	MsgLong  = `The 'off' command is dodot's primary removal command. It completely removes pack deployments:
  - Removes all symlinks
  - Clears shell integrations and PATH entries
  - Removes all handler state from the data directory

Note: This is a complete removal - no state is saved for restoration. Files in your dotfiles repository are never touched.`
	MsgExample = `  # Remove all pack deployments
  dodot off
  
  # Remove specific packs
  dodot off vim zsh
  
  # Preview what will be removed
  dodot off --dry-run vim`
)
