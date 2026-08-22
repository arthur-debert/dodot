#!/usr/bin/env bats
# E2E tests for `dodot adopt`.

setup() {
    load helpers/setup
    sandbox_setup
}

teardown() {
    sandbox_teardown
}

@test "adopt moves file into pack and creates symlink" {
    create_pack "vim"
    create_home_file ".vimrc" "set nocompatible"

    # HOME-direct dotfile requires --into (pack name has no path-derivable
    # source under the new inference rules).
    run dodot adopt --into vim "$HOME/.vimrc"
    [ "$status" -eq 0 ]
    # Adopt output is now the destination pack's status (matches `dodot status vim`).
    assert_output_contains "vim"
    assert_output_contains "vimrc"
    assert_output_contains "pending"

    # File should be in the pack (dot prefix stripped, home. prefix added).
    assert_exists "$DOTFILES_ROOT/vim/home.vimrc"
    assert_file_contents "$DOTFILES_ROOT/vim/home.vimrc" "set nocompatible"

    # Original location should be a symlink
    [ -L "$HOME/.vimrc" ]
}

@test "adopt with --force overwrites existing pack file" {
    create_pack_file "vim" "home.vimrc" "old content"
    create_home_file ".vimrc" "new content"

    run dodot adopt --into vim --force "$HOME/.vimrc"
    [ "$status" -eq 0 ]

    assert_file_contents "$DOTFILES_ROOT/vim/home.vimrc" "new content"
}

@test "adopt reports error without --force when file exists in pack" {
    create_pack_file "vim" "home.vimrc" "old content"
    create_home_file ".vimrc" "new content"

    run dodot adopt --into vim "$HOME/.vimrc"
    assert_output_contains "already exists"

    # Original pack file should be unchanged
    assert_file_contents "$DOTFILES_ROOT/vim/home.vimrc" "old content"
}

@test "adopt multiple files" {
    create_pack "shell"
    create_home_file ".bashrc" "# bashrc"
    create_home_file ".zshrc" "# zshrc"

    run dodot adopt --into shell "$HOME/.bashrc" "$HOME/.zshrc"
    [ "$status" -eq 0 ]
    # Status output lists both adopted files under the destination pack.
    assert_output_contains "shell"
    assert_output_contains "bashrc"
    assert_output_contains "zshrc"

    # `bashrc` and `zshrc` are in the default force_home list, so they
    # adopt with the bare in-pack name (Priority 3 routes them back to
    # ~/.X without the `home.` prefix).
    assert_exists "$DOTFILES_ROOT/shell/bashrc"
    assert_exists "$DOTFILES_ROOT/shell/zshrc"
}

@test "adopt reports error when target file does not exist" {
    create_pack "vim"

    run dodot adopt --into vim "$HOME/.nonexistent"
    assert_output_contains "source does not exist"
}

@test "adopt infers pack from XDG path and auto-creates" {
    # No pre-existing pack, no --into — pack name comes from the path
    # (`~/.config/<X>/...` → pack `<X>`), and the pack is auto-created.
    create_home_file ".config/ghostty/config" "theme = dark"

    run dodot adopt "$HOME/.config/ghostty/config"
    [ "$status" -eq 0 ]
    assert_output_contains "ghostty"

    # Pack auto-created at <dotfiles>/ghostty/, file at pack root (the
    # default rule routes pack `ghostty`/`config` back to ~/.config/ghostty/config).
    assert_exists "$DOTFILES_ROOT/ghostty/config"
    assert_file_contents "$DOTFILES_ROOT/ghostty/config" "theme = dark"
    [ -L "$HOME/.config/ghostty/config" ]
}

@test "adopt of XDG pack-root directory expands to children" {
    # Adopting the directory itself enumerates its children and adopts
    # each as its own top-level pack entry (rather than symlinking the
    # whole directory).
    create_home_file ".config/helix/config.toml" "theme = \"onedark\""
    create_home_file ".config/helix/themes/extra.toml" "fg = \"white\""

    run dodot adopt "$HOME/.config/helix"
    [ "$status" -eq 0 ]

    assert_exists "$DOTFILES_ROOT/helix/config.toml"
    assert_exists "$DOTFILES_ROOT/helix/themes/extra.toml"
    [ -L "$HOME/.config/helix/config.toml" ]
    [ -L "$HOME/.config/helix/themes" ]
    # The pack-root directory itself stays a real directory.
    [ ! -L "$HOME/.config/helix" ]
}

@test "adopt without --into for HOME source errors with hint" {
    create_home_file ".vimrc" "set nocompatible"

    run dodot adopt "$HOME/.vimrc"
    # Post-standout-7.6.2: handler errors exit non-zero. Pin both the
    # status and the message so a regression on either side is caught.
    [ "$status" -ne 0 ]
    assert_output_contains "--into"
    assert_output_contains "could not infer"
}

# ── Safe new-pack adoption path ──────────────────────────────────
#
# docs/proposals/adopt-safety.lex §2.3: a refused adopt leaves the
# dotfiles repo as it found it, an inferred pack included.

@test "adopt --dry-run of a new inferred pack reports the plan and creates no pack" {
    create_home_file ".config/ghostty/config" "theme = dark"

    run dodot adopt --dry-run "$HOME/.config/ghostty/config"
    [ "$status" -eq 0 ]
    assert_output_contains "ghostty"
    assert_output_contains "config"

    # Nothing at a final path: no pack, no preparation directory left
    # behind, and the source is still the user's own file.
    assert_not_exists "$DOTFILES_ROOT/ghostty"
    [ -z "$(find "$DOTFILES_ROOT" -maxdepth 1 -name '.dodot-adopt-*' -print -quit)" ]
    [ ! -L "$HOME/.config/ghostty/config" ]
    assert_file_contents "$HOME/.config/ghostty/config" "theme = dark"
}

@test "adopt refused by a cross-pack conflict creates no inferred pack" {
    # `other` already deploys to ~/.config/ghostty/config via the _xdg/
    # routing prefix, so publishing a `ghostty` pack would collide.
    create_pack_file "other" "_xdg/ghostty/config" "from the other pack"
    create_home_file ".config/ghostty/config" "theme = dark"

    run dodot adopt "$HOME/.config/ghostty/config"
    [ "$status" -ne 0 ]

    assert_not_exists "$DOTFILES_ROOT/ghostty"
    [ -z "$(find "$DOTFILES_ROOT" -maxdepth 1 -name '.dodot-adopt-*' -print -quit)" ]
    [ ! -L "$HOME/.config/ghostty/config" ]
    assert_file_contents "$HOME/.config/ghostty/config" "theme = dark"
    assert_file_contents "$DOTFILES_ROOT/other/_xdg/ghostty/config" "from the other pack"
}

@test "adopt refuses a source that contains another source" {
    create_home_file ".config/nvim/lua/init.lua" "-- init"

    run dodot adopt "$HOME/.config/nvim/lua" "$HOME/.config/nvim/lua/init.lua"
    [ "$status" -ne 0 ]
    assert_output_contains "contains"

    assert_not_exists "$DOTFILES_ROOT/nvim"
    [ ! -L "$HOME/.config/nvim/lua" ]
    assert_file_contents "$HOME/.config/nvim/lua/init.lua" "-- init"
}

@test "adopt publishes a new inferred pack whole and leaves no marker in it" {
    create_home_file ".config/helix/config.toml" 'theme = "onedark"'
    create_home_file ".config/helix/themes/extra.toml" 'fg = "white"'

    run dodot adopt "$HOME/.config/helix"
    [ "$status" -eq 0 ]

    assert_file_contents "$DOTFILES_ROOT/helix/config.toml" 'theme = "onedark"'
    assert_file_contents "$DOTFILES_ROOT/helix/themes/extra.toml" 'fg = "white"'
    assert_not_exists "$DOTFILES_ROOT/helix/.dodotignore"
    [ -z "$(find "$DOTFILES_ROOT" -maxdepth 1 -name '.dodot-adopt-*' -print -quit)" ]

    # Immediately discoverable, with the sources replaced by links.
    run dodot list
    assert_output_contains "helix"
    [ -L "$HOME/.config/helix/config.toml" ]
    [ -L "$HOME/.config/helix/themes" ]
}

@test "adopt refuses while another pack's externals manifest is an unrendered template" {
    # `shared` declares its fetch targets inside externals.toml, and that
    # file only exists as a template dodot has never rendered — so adopt
    # cannot know what `shared` claims, and refuses rather than publish
    # into a collision it could not see.
    create_pack_file "shared" "externals.toml.tmpl" \
        '[bashrc]\ntype = "file"\nurl = "https://example.com/bashrc"\ntarget = "~/{{ name }}"\nsha256 = "abc"\n'
    create_home_file ".config/ghostty/config" "theme = dark"

    run dodot adopt "$HOME/.config/ghostty/config"
    [ "$status" -ne 0 ]
    assert_output_contains "externals.toml.tmpl"
    assert_output_contains "dodot up"

    assert_not_exists "$DOTFILES_ROOT/ghostty"
    [ -z "$(find "$DOTFILES_ROOT" -maxdepth 1 -name '.dodot-adopt-*' -print -quit)" ]
    [ ! -L "$HOME/.config/ghostty/config" ]
    assert_file_contents "$HOME/.config/ghostty/config" "theme = dark"
}

# ── Classification and the one-run report ────────────────────────
#
# docs/proposals/adopt-safety.lex §3–§4: adopt classifies where the
# pack scan reads a name, refuses a typed source no scan would read,
# and leaves a discovered one where it is with a single report line.

@test "adopt leaves an ignored child in place, reports it once, and succeeds" {
    create_home_file ".config/zed/settings.json" "{}"
    create_home_file ".config/zed/keymap.json" "[]"
    create_home_file ".config/zed/.DS_Store" "finder noise"

    run dodot adopt "$HOME/.config/zed"
    [ "$status" -eq 0 ]
    assert_output_contains "left in place"
    assert_output_contains ".DS_Store"
    assert_output_contains "[pack] ignore"

    # The adoptable siblings completed.
    assert_file_contents "$DOTFILES_ROOT/zed/settings.json" "{}"
    [ -L "$HOME/.config/zed/settings.json" ]

    # The ignored child is untouched and never entered the pack.
    assert_not_exists "$DOTFILES_ROOT/zed/.DS_Store"
    [ ! -L "$HOME/.config/zed/.DS_Store" ]
    assert_file_contents "$HOME/.config/zed/.DS_Store" "finder noise"

    # And no later command mentions it: [pack] ignore is silent by design.
    run dodot status zed
    [ "$status" -eq 0 ]
    assert_output_not_contains ".DS_Store"
}

@test "adopt refuses an explicitly named ignored source, naming pattern and layer" {
    create_home_file ".config/zed/scratch.tmp" "junk"
    create_root_config '[pack]\nignore = ["*.tmp"]\n'

    run dodot adopt "$HOME/.config/zed/scratch.tmp"
    [ "$status" -ne 0 ]
    assert_output_contains "*.tmp"
    assert_output_contains "the root .dodot.toml"

    assert_not_exists "$DOTFILES_ROOT/zed"
    assert_file_contents "$HOME/.config/zed/scratch.tmp" "junk"
}

@test "adopt refuses an explicitly named hidden source without offering a setting" {
    create_home_file ".config/nvim/.luarc.json" "{}"

    run dodot adopt "$HOME/.config/nvim/.luarc.json"
    [ "$status" -ne 0 ]
    assert_output_contains "No config setting changes that"

    assert_not_exists "$DOTFILES_ROOT/nvim"
    [ ! -L "$HOME/.config/nvim/.luarc.json" ]
}

@test "adopt refuses a directory with no adoptable children and writes nothing" {
    create_home_file ".config/cache-only/.DS_Store" "noise"
    create_home_file ".config/cache-only/index.swp" "swap"

    run dodot adopt "$HOME/.config/cache-only"
    [ "$status" -ne 0 ]
    assert_output_contains "no adoptable entries"
    assert_output_contains ".DS_Store"
    assert_output_contains "index.swp"

    assert_not_exists "$DOTFILES_ROOT/cache-only"
    [ -z "$(find "$DOTFILES_ROOT" -maxdepth 1 -name '.dodot-adopt-*' -print -quit)" ]
    assert_file_contents "$HOME/.config/cache-only/.DS_Store" "noise"
}

@test "adopt refuses a reserved filename whether named or discovered" {
    create_home_file ".config/zed/.dodot.toml" "[pack]\n"

    run dodot adopt "$HOME/.config/zed/.dodot.toml"
    [ "$status" -ne 0 ]
    assert_output_contains "dodot's own pack configuration file"
    assert_not_exists "$DOTFILES_ROOT/zed"

    create_home_file ".config/zed/settings.json" "{}"
    run dodot adopt "$HOME/.config/zed"
    [ "$status" -ne 0 ]
    assert_output_contains ".dodot.toml"
    assert_not_exists "$DOTFILES_ROOT/zed"
}

@test "adopt --force changes no classification outcome" {
    create_home_file ".config/zed/settings.json" "{}"
    create_home_file ".config/zed/.DS_Store" "finder noise"

    run dodot adopt --force "$HOME/.config/zed"
    [ "$status" -eq 0 ]
    assert_output_contains "left in place"
    assert_not_exists "$DOTFILES_ROOT/zed/.DS_Store"
    assert_file_contents "$HOME/.config/zed/.DS_Store" "finder noise"

    create_home_file ".config/nvim/.luarc.json" "{}"
    run dodot adopt --force "$HOME/.config/nvim/.luarc.json"
    [ "$status" -ne 0 ]
    assert_output_contains "No config setting changes that"
}

@test "adopt classifies below the first component the way a pack scan does" {
    # `plugins` matches a pattern but sits below the position the
    # top-level walk reads, so the adoption proceeds — the scan reads
    # `lua` and hands the whole directory to the symlink handler.
    create_home_file ".config/nvim/lua/plugins/init.lua" "-- plugins"
    create_root_config '[pack]\nignore = ["plugins"]\n'

    run dodot adopt "$HOME/.config/nvim/lua/plugins/init.lua"
    [ "$status" -eq 0 ]
    assert_file_contents "$DOTFILES_ROOT/nvim/lua/plugins/init.lua" "-- plugins"
}

# ── The pack directory the scan reads ────────────────────────────
#
# §3 applies at every position a later scan reads, and the pack
# directory is one of them. Adopt picks that name by inference, so it
# is the position adopt can get wrong on the user's behalf.

@test "adopt refuses an inferred pack name the dotfiles-root scan ignores" {
    create_home_file ".config/node_modules/settings.json" "{}"

    run dodot adopt "$HOME/.config/node_modules/settings.json"
    [ "$status" -ne 0 ]
    assert_output_contains "node_modules"
    assert_output_contains "[pack] ignore"

    assert_not_exists "$DOTFILES_ROOT/node_modules"
    [ ! -L "$HOME/.config/node_modules/settings.json" ]
    assert_file_contents "$HOME/.config/node_modules/settings.json" "{}"
}

@test "adopt refuses a hidden inferred pack name" {
    create_home_file ".config/.foo/settings" "x"

    run dodot adopt "$HOME/.config/.foo/settings"
    [ "$status" -ne 0 ]
    assert_output_contains "No config setting changes that"

    assert_not_exists "$DOTFILES_ROOT/.foo"
    assert_file_contents "$HOME/.config/.foo/settings" "x"
}

@test "an explicit --into pack takes a source whose inferred name is ignored" {
    create_pack "editor"
    create_home_file ".config/node_modules/settings.json" "{}"

    run dodot adopt "$HOME/.config/node_modules/settings.json" --into editor
    [ "$status" -eq 0 ]

    assert_file_contents "$DOTFILES_ROOT/editor/_xdg/node_modules/settings.json" "{}"
    [ -L "$HOME/.config/node_modules/settings.json" ]
}

@test "adopt refuses a source behind an undefined gate directory" {
    create_home_file ".config/nvim/_bogus/init.lua" "-- config"

    run dodot adopt "$HOME/.config/nvim/_bogus/init.lua"
    [ "$status" -ne 0 ]
    assert_output_contains "gate label"

    assert_not_exists "$DOTFILES_ROOT/nvim"
    assert_file_contents "$HOME/.config/nvim/_bogus/init.lua" "-- config"
}

@test "an undefined gate directory found by expansion is left in place and the pack still scans" {
    create_home_file ".config/nvim/init.lua" "-- config"
    create_home_file ".config/nvim/_bogus/extra.lua" "-- extra"

    run dodot adopt "$HOME/.config/nvim"
    [ "$status" -eq 0 ]
    assert_output_contains "left in place"
    assert_output_contains "_bogus"

    assert_file_contents "$DOTFILES_ROOT/nvim/init.lua" "-- config"
    assert_not_exists "$DOTFILES_ROOT/nvim/_bogus"
    assert_file_contents "$HOME/.config/nvim/_bogus/extra.lua" "-- extra"

    # The published pack is one a scan reads end to end.
    run dodot status nvim
    [ "$status" -eq 0 ]
}
