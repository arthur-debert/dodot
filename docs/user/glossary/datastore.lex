:: verified ::
Datastore:
    The directory dodot uses to track what it has done — `~/.local/share/dodot/` by default (`$XDG_DATA_HOME/dodot/`). Holds the symlinks and sentinels that make a deployment "live," and is the only place dodot writes outside your dotfiles root.

    It's deliberately legible: a regular directory you can `ls`, `tree`, or inspect with `dodot probe show-data-dir`. The state isn't a record of what dodot did — it IS the input the shell init reads on every login. The datastore is at the same time the log of what's being done and the done thing itself, so you can't drift from it.
