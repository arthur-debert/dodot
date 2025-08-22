# Dry Run Mode

The `--dry-run` flag allows you to preview what dodot will do without actually
making any changes to your system.

## Usage

Add `--dry-run` to any command to see what would happen:

```bash
$ dodot link --dry-run
$ dodot provision vim --dry-run
$ dodot link --all --dry-run
```

## What It Shows

In dry-run mode, dodot will:

- List all symlinks that would be created
- Show which files would be backed up
- Display shell profile modifications
- Report install scripts that would run
- Show Brewfiles that would be processed

## What It Doesn't Do

In dry-run mode, dodot will **NOT**:

- Create any symlinks
- Modify any files
- Run any install scripts
- Execute brew commands
- Make backups
- Update shell profiles

## Example Output

```bash
$ dodot link vim --dry-run

DRY RUN MODE - No changes will be made

Would process pack: vim
  ✓ Would create symlink: ~/.vimrc -> ~/dotfiles/vim/vimrc
  ✓ Would create symlink: ~/.vim/colors -> ~/dotfiles/vim/colors
  ✓ Would add to shell profile: source ~/dotfiles/vim/aliases.sh
  ✓ Would run install script: vim/install.sh
```

## Use Cases

- Preview changes before deployment
- Verify pack configuration
- Debug trigger patterns
- Understand what a pack will do
- Safe exploration of dodot commands