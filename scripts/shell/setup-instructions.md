# dodot Shell Integration Setup

To enable dodot shell integration, add the following line to your shell configuration file:

## For Bash users
Add to `~/.bashrc`:
```bash
[ -f "$HOME/.local/share/dodot/shell/dodot-init.sh" ] && source "$HOME/.local/share/dodot/shell/dodot-init.sh"
```

## For Zsh users
Add to `~/.zshrc`:
```bash
[ -f "$HOME/.local/share/dodot/shell/dodot-init.sh" ] && source "$HOME/.local/share/dodot/shell/dodot-init.sh"
```

## For Fish users
Add to `~/.config/fish/config.fish`:
```fish
if test -f "$HOME/.local/share/dodot/shell/dodot-init.fish"
    source "$HOME/.local/share/dodot/shell/dodot-init.fish"
end
```

## Custom data directory
If you're using a custom DODOT_DATA_DIR, adjust the path accordingly:
```bash
[ -f "$DODOT_DATA_DIR/shell/dodot-init.sh" ] && source "$DODOT_DATA_DIR/shell/dodot-init.sh"
```

## Verify installation
After adding the line and restarting your shell, run:
```bash
dodot_status
```

This should show your dodot deployment status.