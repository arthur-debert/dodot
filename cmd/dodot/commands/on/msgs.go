package on

// Message constants
const (
	MsgShort = "Install and deploy pack(s)"
	MsgLong  = `The 'on' command is dodot's primary deployment command. It handles all aspects of pack deployment:
  - Creates symlinks for configuration files
  - Sets up shell integrations and PATH entries
  - Runs installation scripts and package managers (unless --no-provision is used)

By default, provisioning handlers only run once per pack. Use options to control this behavior.

Provisioning Options:
  --no-provision: Skip provisioning handlers (only link files)
  --provision-rerun: Force re-run provisioning even if already done`

	MsgExample = `  # Deploy all packs
  dodot on
  
  # Deploy specific packs
  dodot on vim zsh
  
  # Preview what will be deployed
  dodot on --dry-run vim
  
  # Only create symlinks, skip installations
  dodot on --no-provision vim
  
  # Force re-run provisioning handlers
  dodot on --provision-rerun vim`
)
