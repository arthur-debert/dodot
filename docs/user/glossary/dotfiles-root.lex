:: verified :: 
Dotfiles root:
    The top-level directory holding your packs, kept under git. dodot picks it in this order: 
    - `$DOTFILES_ROOT` if set and the path exists
    - The git top-level of your current directory (so `cd ~/dotfiles/nvim && dodot up` finds the repo root); 
    - Current directory itself. 

    Everything dodot _reads_ as input lives here; nothing dodot _writes_ lives here. 
    The root IS the source of truth — dodot never drops state files alongside your configs, so `git status` always shows your changes, never dodot's bookkeeping.
