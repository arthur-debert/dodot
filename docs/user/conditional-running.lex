Conditional Running

    Some dotfiles only make sense on certain machines: a `Brewfile` is
    macOS-specific; an `apt-get`-flavoured `install.sh` is Linux-specific;
    a Hammerspoon config is dead weight on a Linux laptop. Rather than
    turning every such file into a Jinja template and rendering empty
    output on the wrong host, dodot lets you *gate* files, directories,
    and whole packs against host facts (OS, arch, hostname) so they only
    deploy where they apply.

    Templates render content that varies between hosts; gates decide
    whether a file *exists* on a host. The two compose: a darwin-only
    template is `aliases._darwin.sh.tmpl` (gate first, template second).

    :: note :: For terminology see [./../reference/terms-and-concepts.lex]. For the design rationale see the conditional-running proposal in [./../proposals/].

1. The Five Surfaces, At A Glance

    Each surface picks a different granularity. Pick the one that
    matches what you want to gate.

    Five surfaces:
        | Surface              | Granularity     | Example                                        |
        | filename suffix      | one file        | `install._darwin.sh`                           |
        | directory segment    | a subtree       | `_darwin/_home/.bashrc`                        |
        | `[pack] os`          | whole pack      | `[pack] os = ["darwin"]` in pack `.dodot.toml` |
        | `[mappings.gates]`   | glob (legacy)   | `"install-mac.sh" = "darwin"`                  |
        | `dodot adopt --only-os` | adopting in   | `dodot adopt ~/.bashrc --only-os darwin`       |
    :: table align=lll ::

    The first three are the everyday tools. `[mappings.gates]` is an
    escape hatch for repos that can't rename files. `--only-os` is a
    convenience flag on `dodot adopt`.

2. Built-In Labels

    A *label* is a token that names a host predicate. dodot ships these
    labels compiled in — they work zero-config:

    Built-in labels:
        | Label     | Means                                |
        | `darwin`  | OS is darwin (macOS)                 |
        | `linux`   | OS is linux                          |
        | `macos`   | alias for `darwin`                   |
        | `arm64`   | arch is aarch64                      |
        | `aarch64` | same as `arm64`                      |
        | `x86_64`  | arch is x86_64                       |
    :: table align=ll ::

    Labels are case-sensitive: `_Darwin` is unknown and produces a hard
    error at scan time (typo guard). Stick to the lowercase forms.

3. Filename Suffix: One File, One Predicate

    The simplest surface. A `._<label>` token sits before the file's
    extension (or as a trailing segment for extensionless files); on a
    matching host the suffix is stripped and the file deploys under the
    cleaned name.

    Per-file gates:

        ~/dotfiles/
            mac-tools/
                install._darwin.sh        # only runs on macOS
                install._linux.sh         # only runs on Linux
                Brewfile._darwin          # brew bundle only on macOS
                aliases._darwin.sh        # sourced into shell only on macOS
                home.bashrc._darwin       # → ~/.bashrc on macOS only

    :: text ::

    On a darwin host, `install._darwin.sh` is matched by the `install`
    handler under its stripped name `install.sh` and deployed
    accordingly. On a linux host the same file is gated out and
    surfaces in `dodot status` as `gated out (darwin)`.

    The suffix composes with file-routing prefixes (the `home.X`,
    `app.X`, `xdg.X`, `lib.X` family from
    [./../reference/symlink-paths.lex]) and with template extensions:

    Composition:

        home.bashrc._darwin               → ~/.bashrc on darwin only
        gitconfig.tmpl._darwin            → renders gitconfig only on darwin
        aliases._darwin.sh.tmpl           → both: darwin-only AND a template

    :: text ::

4. Directory Segment: A Whole Subtree

    A `_<label>/` directory at the pack root gates everything beneath
    it. On a matching host the directory expands transparently — its
    contents surface at the pack root level, with the gate segment
    stripped from their relative paths. On a mismatch the whole
    directory is gated out.

    Subtree gates:

        ~/dotfiles/
            cross-pack/
                _darwin/
                    Brewfile              # only on darwin
                    install.sh            # only on darwin
                _linux/
                    install.sh            # only on linux
                shared.txt                # always

    :: text ::

    On macOS, `cross-pack/_darwin/Brewfile` is treated as if the user
    had written `cross-pack/Brewfile` directly — the `_darwin/` segment
    vanishes from the deploy view. On Linux, the entire `_darwin/`
    subtree is gated out.

    Gate dirs nest naturally with routing-prefix subtrees and with
    each other (AND semantics). Gate dirs always sit *outside* routing
    prefixes:

    Composing with routing prefixes:

        ~/dotfiles/
            mac-tools/
                _darwin/
                    _home/
                        .hammerspoon/init.lua    # → ~/.hammerspoon/init.lua on darwin
                _arm-mac/
                    setup.sh                    # only on darwin AND aarch64

    :: text ::

    Routing-prefix names (`home`, `xdg`, `app`, `lib`) cannot be used
    as gate labels — `_home/` is always a routing prefix, never a
    gate. Other names are looked up in the gate table; unknown labels
    are a hard error at scan time.

    :: note :: The reverse nesting (`_home/_darwin/...` — routing prefix outside, gate inside) is *not* supported. The symlink resolver owns recursion inside routing-prefix subtrees and is gate-unaware. Put the gate at the outer level: `_darwin/_home/...`.

5. Pack-Level: `[pack] os`

    For packs that should only exist on certain operating systems, set
    `[pack] os` in the pack's `.dodot.toml`:

    Pack-level OS gating:

        # mac-tools/.dodot.toml
        [pack]
        os = ["darwin"]

    :: toml ::

    `os` is a list of OS identifiers. The canonical value for macOS is
    `"darwin"`; `"macos"` is accepted as an alias. `"linux"` is the
    canonical value on Linux. Set to `["darwin", "linux"]` for "either."
    Empty or absent means "all OSes" (today's behaviour).

    Note: template variables expose `dodot.os` as `"macos"` on macOS,
    but gate OS matching uses `"darwin"` as the canonical form and
    accepts `"macos"` as an alias — the two surfaces are not identical.

    On a non-matching host, the entire pack is short-circuited at scan
    time — no preprocessing fires, no handlers run, no symlinks land.
    `dodot status` surfaces the pack under an "Inactive on this OS"
    section so you know it's there but skipped:

    Status output (running on linux):

        $ dodot status
          shared-tools/
            vimrc                  ➞ ~/.config/vim/vimrc       deployed
            …
          Inactive on this OS
            mac-tools (os=darwin, current=linux)

    :: text ::

    Root-level `[pack] os` is rejected (gating every pack from the
    root would silently neutralise the dotfiles repo for hosts not in
    the list — almost always a misconfiguration). Set it per-pack.

6. User-Defined Labels: `[gates]`

    Need a label not in the built-in seed? Define one in `.dodot.toml`:

    User-defined labels:

        [gates]
        laptop  = { hostname = "mbp-arthur" }
        work    = { hostname = "work-laptop" }
        arm-mac = { os = "darwin", arch = "aarch64" }
        intel-mac = { os = "darwin", arch = "x86_64" }

    :: toml ::

    Each entry maps a label name to a table of `(dimension, value)`
    equality checks AND-ed together. Recognised dimensions: `os`,
    `arch`, `hostname`, `username` — same set templates expose under
    `dodot.*`.

    Once defined, labels work in every gate surface:

    Using a custom label:

        ~/dotfiles/
            shell/
                aliases._laptop.sh        # only on the laptop
            apple-silicon/
                _arm-mac/
                    install.sh            # only on darwin + aarch64

    :: text ::

    Constraints on label names: they must match `[A-Za-z0-9_-]+` (so
    they can be parsed from filenames and directories) and must not
    collide with routing-prefix tokens (`home`/`xdg`/`app`/`lib`).
    Both are hard errors at config load.

    Labels deep-merge across the standard config layers (compiled
    defaults < root `.dodot.toml` < pack `.dodot.toml`), so a pack can
    add labels without restating root labels. Shadowing a built-in is
    allowed but unusual.

7. Glob Escape Hatch: `[mappings.gates]`

    For repos where renaming files isn't an option (or where the gate
    is a property of an external project's filename), use
    `[mappings.gates]`:

    Glob → label:

        [mappings.gates]
        "install-mac.sh" = "darwin"
        "Brewfile"       = "darwin"

    :: toml ::

    Patterns match against the same top-level pack entries the scanner
    surfaces. Globs containing path separators (`"setup/*.sh"`) only
    match if a top-level entry has that shape — the symlink handler's
    nested per-file recursion is intentionally gate-unaware (same
    posture as gates inside routing-prefix subtrees).

    A file carrying both a filename gate (`._<label>`) and a matching
    `[mappings.gates]` entry is a hard error: pick one source of
    truth. Invalid glob patterns are also a hard error at scan time
    (no silent typos).

8. Adopting With A Gate: `dodot adopt --only-os`

    `dodot adopt` defaults to "no gate" — adopting `~/.bashrc` from a
    darwin host produces `home.bashrc` (no suffix), so re-deploying on
    any host puts the file back at `~/.bashrc`. Pass `--only-os
    <label>` to wrap the adopted entry in a gate dir so re-deploy
    only fires on matching hosts:

    Gated adopt:

        $ dodot adopt ~/.bashrc --only-os darwin --into shell

    :: shell ::

    The pack tree ends up as:

    Result:

        ~/dotfiles/
            shell/
                _darwin/
                    home.bashrc          # re-deploys to ~/.bashrc on darwin only

    :: text ::

    The label is validated against the *root* `.dodot.toml` gate table
    (built-ins + user-defined `[gates]` at the root level) at the start
    of the command, so an unknown label fails before any filesystem work
    happens. Labels defined only in a pack-level `.dodot.toml` are not
    visible here.

9. When To Use Gates Versus Templates

    Both gates and templates handle "this varies between hosts," but
    they answer different questions:

    Gates vs templates:
        | Question                                    | Tool      |
        | Should this file exist on this host?        | gate      |
        | Should this directory deploy on this host?  | gate      |
        | Should this whole pack run on this host?    | gate      |
        | What does this line look like on this host? | template  |
        | What value goes in this `{{ }}` slot?       | template  |
    :: table align=ll ::

    Reach for a template when the file always exists but its *content*
    varies between hosts. Reach for a gate when the question is binary
    — deploy or don't.

    The two compose: `aliases._darwin.sh.tmpl` is darwin-only AND a
    template. The gate runs first (drops on Linux, no template render
    fires for the gated-out file). The template runs second (renders
    `aliases.sh` on darwin, which the shell handler picks up).

10. Reading Gate Status Output

    `dodot status` surfaces every gate decision so you don't have to
    guess what the host did:

    Status output (running on darwin):

        $ dodot status
          mac-tools/
            install.sh           ×  run script     never run
            install._linux.sh    ·  not deployed   gated out (linux) [1]
            Brewfile             ⚙  brew install   not installed
          Inactive on this OS
            linux-tools (os=linux, current=darwin)

        Errors:
          [1] expected os=linux; got os=darwin

    :: shell ::

    The footnote shows the predicate the gate expected and what the
    host actually has. For *passing*-gate files the row is rendered
    under the *stripped* name (`install.sh` not `install._darwin.sh`)
    because that's what dodot will deploy. For *failing* gates the
    original on-disk name is shown so you can find the file.

11. Limitations

    Things gates intentionally do not do:

    - **No predicate language**. Labels are equality-only AND
      conjunctions. For OR, define multiple labels and use multiple
      filenames.
    - **No filename stacking**. `install._darwin._arm64.sh` is *not*
      parsed as "darwin AND arm64." Use a compound user-defined label
      (`arm-mac = { os = "darwin", arch = "aarch64" }`) and write
      `install._arm-mac.sh`.
    - **No negation**. There's no `_!darwin` syntax. Write the
      positive form for the OSes you do want.
    - **No nested gates inside routing-prefix subtrees**.
      `_home/_darwin/...` is *not* recognised — the symlink handler
      owns recursion inside routing prefixes. Put the gate at the
      outer level: `_darwin/_home/...`.
    - **No profile selection**. dodot is single-config-per-machine;
      hostname-based gates are the closest analog. See
      [./../reference/philosophy.lex] §7.

12. Diagnostic Tips

    - `dodot status` is the source of truth — it shows every gated
      file under its actual disposition.
    - `dodot config` prints the fully resolved config including the
      `[gates]` table after merging. Useful when a label appears not
      to fire.
    - Unknown gate labels are a *scan-time error*, not a silent skip.
      A typo in `_darwn.sh` or `[mappings.gates] = { foo = "darwn" }`
      stops `dodot up` with a clear "unknown gate label" diagnostic.
