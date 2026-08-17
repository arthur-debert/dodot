:: verified ::
Dotfiles root:
    The top-level directory holding your packs, kept under git. dodot picks it in this order:
    - `$DOTFILES_ROOT` if set; an empty value, or a path that doesn't exist, isn't a directory, or can't be read, is an error, never a reason to choose another root;
    - The git top-level of your current directory (so `cd ~/dotfiles/nvim && dodot up` finds the repo root);
    - Current directory itself.

    The root IS the source of truth for pack content. Commands such as `init`, `adopt`, and root-scoped `config set` explicitly write user-owned source or configuration here; Safety Lock protects those writes when the root was discovered implicitly. dodot's internal bookkeeping lives in its data directory instead, so it never drops state files alongside your configs.
