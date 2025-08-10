// Package template provides the TemplatePowerUp implementation for dodot.
//
// # Overview
//
// The TemplatePowerUp processes template files with variable substitution to generate
// personalized configuration files. This is ideal for configs that need machine-specific
// values like usernames, hostnames, email addresses, or environment-specific settings
// while keeping a single template in version control.
//
// # When It Runs
//
// - **Deploy Mode**: YES - Runs during `dodot deploy` (RunModeMany)
// - **Install Mode**: NO - Does not run during `dodot install`
// - **Idempotent**: YES - Regenerates files on each run (overwrites)
//
// # Standard Configuration
//
// In your global dodot configuration (~/.config/dodot/config.toml):
//
//	[matchers.templates]
//	trigger = "extension"
//	extension = ".tmpl"
//	powerup = "template"
//	priority = 70
//
// Or in a pack-specific .dodot.toml:
//
//	[[matchers]]
//	trigger = "filename"
//	patterns = ["*.tmpl", "*.template"]
//	powerup = "template"
//	options = { target = "$HOME/.config", company = "AcmeCorp" }
//
// # File Selection Process
//
// 1. **Pack Discovery**: dodot finds all subdirectories in $DOTFILES_ROOT
// 2. **File Walking**: Recursively walks each pack directory
// 3. **Trigger Matching**: ExtensionTrigger matches files ending in .tmpl
// 4. **PowerUp Invocation**: Matched files are passed to TemplatePowerUp.Process()
//
// Example file structure:
//
//	~/dotfiles/
//	├── git/
//	│   └── gitconfig.tmpl  # Generates ~/.gitconfig
//	├── ssh/
//	│   └── config.tmpl     # Generates ~/.ssh/config
//	└── work/
//	    └── npmrc.tmpl      # Generates ~/.npmrc with work registry
//
// # Execution Strategy
//
// dodot processes templates by performing variable substitution:
//
// 1. **Read Template**: Loads the .tmpl file content
// 2. **Gather Variables**: Collects system, environment, and custom variables
// 3. **Substitute**: Replaces all variable placeholders with actual values
// 4. **Write Output**: Creates the processed file at target location
// 5. **Strip Extension**: Removes .tmpl from the output filename
//
// Variable substitution supports multiple syntaxes:
//
//	{{.USER}}        # Go template style
//	{{.Env.HOME}}    # Environment variable style
//	${HOSTNAME}      # Shell variable style
//	{{.Custom}}      # Custom variables from options
//
// # Storage Locations
//
// - **Templates**: Remain in pack directories (not copied)
// - **Generated files**: Written to target directory (default: ~)
// - **No intermediate files**: Processing happens in memory
// - **No sentinels**: Files are regenerated on each deploy
//
// # Environment Variable Tracking
//
// The TemplatePowerUp does not currently track generated files in environment
// variables. Unlike other powerups, template-generated files are not tracked
// because they are processed files rather than symlinks or references.
//
// To see which templates are available, you can check your dotfiles for .tmpl files:
// `find $DOTFILES_ROOT -name "*.tmpl"`
//
// # Available Variables
//
// Default variables available in all templates:
//
// - **HOME**: User's home directory
// - **USER**: Current username
// - **SHELL**: User's default shell
// - **HOSTNAME**: Machine hostname
// - **All environment variables**: Via {{.Env.VARNAME}}
//
// Custom variables can be added via powerup options.
//
// # Effects on User Environment
//
// - **Creates files**: Generates new files from templates
// - **Overwrites existing**: Replaces files on each run
// - **No backups**: Previous versions are lost (unless in git)
// - **Permissions**: Files created with default umask
// - **Not reversible**: No built-in way to remove generated files
//
// # Options
//
// The TemplatePowerUp accepts these options:
//
// - **target** (string): Directory where files are created
//   - Default: User's home directory (~)
//   - Supports environment variables: $HOME/.config
//
// - **Any string key**: Becomes a custom variable in templates
//   - Example: `email = "user@example.com"` → {{.email}}
//
// # Template Examples
//
// Git configuration (gitconfig.tmpl):
//
//	[user]
//	name = {{.USER}}
//	email = {{.email}}
//
//	[core]
//	editor = {{.SHELL}}
//
//	[includeIf "gitdir:~/work/"]
//	path = ~/.gitconfig-work
//
// SSH configuration (ssh/config.tmpl):
//
//	Host *.{{.company}}.com
//	User {{.USER}}
//	IdentityFile ~/.ssh/{{.company}}_rsa
//
//	Host personal
//	HostName {{.personal_server}}
//	User {{.personal_user}}
//
// # Example End-to-End Flow
//
// User runs: `dodot deploy`
//
// 1. dodot finds ~/dotfiles/git/gitconfig.tmpl
// 2. ExtensionTrigger matches ".tmpl" extension
// 3. TemplatePowerUp gathers variables:
//   - System: USER=john, HOME=/home/john, HOSTNAME=laptop
//   - Options: email=john@example.com
//
// 4. Creates ActionTypeTemplate with variables map
// 5. DirectExecutor reads gitconfig.tmpl content
// 6. Substitutes: {{.USER}} → john, {{.email}} → john@example.com
// 7. Writes processed content to ~/.gitconfig
// 8. User has personalized git configuration
//
// # Error Handling
//
// Common errors and their codes:
//
// - **TMPL001**: Template file not found
// - **TMPL002**: Invalid template syntax
// - **TMPL003**: Undefined variable referenced
// - **TMPL004**: Can't write to target directory
// - **TMPL005**: Permission denied creating file
//
// # Best Practices
//
// 1. **Use meaningful variable names**: {{.work_email}} better than {{.em}}
// 2. **Document variables**: Add comments listing required variables
// 3. **Provide defaults**: Use Go template conditionals for optional vars
// 4. **Keep templates readable**: Avoid complex logic in templates
// 5. **Test locally**: Verify substitution before deploying
// 6. **Version control**: Templates are code - commit them
// 7. **Separate concerns**: One template per config file
//
// # Advanced Template Features
//
// Go template conditionals:
//
//	{{if .work_email}}
//	email = {{.work_email}}
//	{{else}}
//	email = {{.USER}}@localhost
//	{{end}}
//
// Default values:
//
//	editor = {{.EDITOR | default "vim"}}
//
// Multiple variable sources:
//
//	# From environment
//	path = {{.Env.CUSTOM_PATH}}
//
//	# From options
//	token = {{.github_token}}
//
//	# From system
//	user = {{.USER}}
//
// # Security Considerations
//
// - **No secret storage**: Don't put passwords in templates
// - **Public templates**: Assume templates are visible in repos
// - **Use references**: Reference secret files, don't embed values
// - **Environment isolation**: Use different templates for work/personal
//
// # Comparison with Other PowerUps
//
// - **SymlinkPowerUp**: Links existing files as-is
// - **TemplatePowerUp**: Generates new files from templates
// - **ShellProfilePowerUp**: For shell environment setup
// - **InstallScriptPowerUp**: For one-time setup tasks
//
// Use TemplatePowerUp when you need configuration files that vary by machine,
// user, or environment. It's perfect for personalizing configs while keeping
// a single source of truth in your dotfiles repository.
package template
