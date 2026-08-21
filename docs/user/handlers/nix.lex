:: verified ::
The nix handler

Runs `nix profile install` against your source `packages.nix` once, tracked by a sentinel keyed on the file's content. Editing the manifest does not re-run it on its own: dodot reports `nix packages older version` and holds until you pass `dodot up --provision-rerun` (section 10). The Linux counterpart to the homebrew handler — and a fine choice on macOS too if you prefer declarative Nix to imperative Homebrew. Mechanically a specialization of the install handler with a more ergonomic default for the common case: "install these packages on every machine I use, declared in one file."

1. Default claim

    A source file named `packages.nix` at the pack root. Single-string match — the nix handler claims one manifest per pack.

    Cross-platform: `nix` runs on Linux and macOS, so the handler has no OS gate. On a host that does not have nix, dodot skips the file and says where it looked rather than running an install that cannot work — section 2. Use a `[pack] os` predicate, a filename gate (`packages._linux.nix`), or a `_linux/` directory gate if you want the pack itself to no-op on a host without Nix; [./../conditional-running.lex] has the full surface.

    Coexists cleanly with the homebrew handler — a single pack can ship both a `Brewfile` and a `packages.nix` and the two run independently against their own package managers. [./homebrew.lex] §1 lays out when to reach for which: brew where its prerequisites (a system compiler, build tools, and on older distributions brew's own gcc and glibc) are acceptable, nix where they are not, and `install.sh` where neither fits.

2. When nix is not installed

    Before it plans anything for a `packages.nix`, dodot looks for `nix` itself, at a fixed list of paths, in order:

        ~/.nix-profile/bin/nix                       # per-user profile; what a login shell resolves
        /nix/var/nix/profiles/default/bin/nix        # multi-user (daemon) install
        ~/.local/state/nix/profiles/profile/bin/nix  # profile location for Nix >= 2.14
        /run/current-system/sw/bin/nix               # NixOS, where nix comes with the system closure

    :: text ::

    The first of those that is a regular file carrying an execute bit wins — every one of them is a symlink into `/nix/store`, and dodot follows it — and *that exact path* is what the run spawns, not whatever `nix` your `PATH` happens to resolve later. dodot never runs nix to find out whether nix is there: the check is a `stat` and a look at the mode bits, which is what lets `dodot status` ask the same question without spawning anything.

    When none of those paths holds an executable, dodot plans nothing for the file. No command runs, no sentinel is written, and it is not an error: `dodot up` exits 0, and the rest of the pack deploys exactly as it would otherwise — symlinks, `$PATH` entries, and shell init are untouched. The manifest still gets its own row in `dodot up` and `dodot status`, styled `skipped` and labelled:

        nix not installed

    :: console ::

    with a warning footnote naming every location that was probed, in probe order, and what to do:

        nix is not installed — probed /Users/you/.nix-profile/bin/nix,
        /nix/var/nix/profiles/default/bin/nix,
        /Users/you/.local/state/nix/profiles/profile/bin/nix, /run/current-system/sw/bin/nix.
        Nothing was recorded, so installing nix (https://nixos.org/download) and re-running
        `dodot up` runs this file.

    :: console ::

    "Nothing was recorded" is the sentence that matters. Absence is a fact about this machine right now, never a receipt: because dodot wrote no sentinel, installing Nix and re-running `dodot up` runs the file — no flag, no stale state to clear first. The project page is the only install instruction dodot gives; it does not run a third-party installer and does not offer to.

    2.1. When a receipt already exists

        Absence does not rewrite history. If `packages.nix` was installed on this machine and nix was removed afterwards, `dodot status` keeps the row's normal `nix packages installed` verdict and adds a warning footnote instead:

            nix is not installed (probed /Users/you/.nix-profile/bin/nix, …) — the receipt
            stands: packages.nix ran when it was

        :: console ::

        And if the receipt is for older content — the `older version` state of section 6 — the row stays stale and the remedy says why the usual fix will not fire:

            ran an older version of packages.nix — run `dodot up --provision-rerun` to apply the
            current one — nix is not installed (probed /Users/you/.nix-profile/bin/nix, …), so
            that re-run skips until it is back

        :: console ::

        That is the honest version: `--provision-rerun` against an absent nix skips the file exactly as a plain `up` would. Install Nix first.

    2.2. When dodot cannot look

        A probe that fails for a reason other than "not there" — a permission error on a parent directory, an I/O failure — is a different event, and dodot reports it as one. The row is styled `broken`, labelled `cannot probe nix`, with an error footnote:

            could not check whether nix is installed: /nix/var/nix/profiles/default/bin/nix
            (permission denied). Nothing was run for this file.

        :: console ::

        Unlike absence, this fails the run: `dodot up` reports "Packs deployed with errors." and exits 1, because the question dodot was asked went unanswered and its report about that file is not to be trusted. It also outranks any receipt — a `packages.nix` with a current sentinel still renders this row, rather than claiming a state dodot could not verify. The rest of the pack deploys either way.

3. Manifest shape

    `packages.nix` evaluates to one of:

    - **List of derivations** — the canonical form:

          { pkgs ? import <nixpkgs> {} }:
          with pkgs; [ ripgrep fd bat ]

      :: nix ::

    - **Bare derivation** — the common case for a one-tool pack:

          { pkgs ? import <nixpkgs> {} }:
          pkgs.zoxide

      :: nix ::

    - **Attribute set of derivations** — useful when a pack wants named attrs for tooling outside dodot:

          { pkgs ? import <nixpkgs> {} }:
          { ripgrep = pkgs.ripgrep; fd = pkgs.fd; }

      :: nix ::

    The `{ pkgs ? import <nixpkgs> {} }:` function wrapper with a default argument lets the manifest resolve `pkgs` from the user's `NIX_PATH`. A bare list literal with no function wrapper has no `pkgs` in scope and fails to evaluate.

4. How dodot invokes nix

    Rather than dispatch on manifest shape, dodot wraps the manifest in a shape-normalizing Nix expression before installing. The install call is the same for every accepted shape:

        nix profile install --impure \
          --extra-experimental-features 'nix-command flakes' \
          --argstr manifest <abs-path-to-packages.nix> \
          --expr '{ manifest }:
          let raw = import manifest;
              m = if builtins.isFunction raw then raw {} else raw;
          in
            if builtins.isList m then m
            else if builtins.isAttrs m && (m.type or null) == "derivation" then [ m ]
            else if builtins.isAttrs m then builtins.attrValues m
            else throw "unsupported shape"'

    :: text ::

    The wrapper imports the manifest, applies the outer function with `{}` when present (resolving the `pkgs` default), collapses list / derivation / attrset to a single list, and `nix profile install` installs that list directly — no selector needed for any shape.

    Two details of that command line are worth knowing, because neither is incidental:

    `--argstr manifest <path>`:
        Your manifest's path travels as an argument, and the wrapper reads it as an ordinary Nix function parameter. The expression above is therefore byte-for-byte identical for every pack on every machine. It is also what lets dodot name the file it ran — in the progress header, and in the `.snapshot` it writes beside the sentinel for `dodot status --diff`.

    `--impure`:
        `nix profile install --expr` evaluates in Nix's *pure* mode, which refuses to read an absolute path outside the store — and that covers both your `packages.nix` and the `<nixpkgs>` the recommended `{ pkgs ? import <nixpkgs> {} }:` wrapper resolves from your `NIX_PATH`. Without the flag, every manifest shape documented above fails to evaluate. It is the same impurity a `nix-shell` or a `nix-env -f` invocation already has: what your `NIX_PATH` points at determines what you get. dodot passes `--impure` on every invocation and has no mode that does not; §8 is the whole story on what that means for pinning.

    There is no planning-time content validation: a syntax error or an unsupported shape inside `packages.nix` surfaces at apply time as a `nix` error, the same way a broken `Brewfile` surfaces a `brew bundle` error and a broken `install.sh` surfaces a `bash` error. dodot stays out of the business of writing its own Nix linter.

    dodot declares no minimum `nix` version. The `nix profile install` surface it uses is a years-old part of the Nix CLI, and dodot has no Nix installation to verify a floor against — an unverified floor would refuse hosts on guesswork. (Homebrew does declare one, for a specific reason: [./homebrew.lex] §6.)

5. Sentinels

    On success, dodot writes a sentinel file `<filename>-<checksum>` into the datastore — for example `packages.nix-a1b2c3d4e5f6a7b8`. The checksum is the first 8 bytes (16 hex chars) of a SHA-256 of the source manifest's bytes. Alongside it dodot also writes a sibling file `<filename>-<checksum>.snapshot` containing the manifest bytes as they were at the time of that run, so a future `dodot status` can show what changed.

    Same flag set as install / homebrew:

    - `--no-provision` — skip every code-execution handler entirely on this run: install, homebrew, nix, and external. The skipped files still get a row, labelled `skipped (--no-provision)`.
    - `--provision-rerun` — the canonical "apply pending content edits" escape hatch for the run-once handlers: install, homebrew, and nix. Re-executes them even when a sentinel exists. Use it after editing `packages.nix` to opt back into running the new content.
    - `--force` — overwrite pre-existing files at symlink target paths. Distinct from `--provision-rerun`; does **not** trigger run-once re-execution.

6. Editing `packages.nix` after it ran (the three states)

    When you edit `packages.nix` after a successful install, dodot does **not** re-run `nix profile install` automatically. The conservative posture is to *notify* and let you decide.

    `dodot up` and `dodot status` report one of three states:

    - **`nix packages not installed`** — no sentinel exists. `dodot up` will run `nix profile install` on the next invocation.
    - **`nix packages installed`** — a sentinel exists for the *current* content hash. The install has run, and the source hasn't changed since. `dodot up` is a no-op.
    - **`nix packages older version (N lines added, M removed)`** — a sentinel exists, but for a *different* content hash. The install ran successfully against an earlier version of `packages.nix`, and you've edited it since. `dodot up` does not auto-rerun. To apply the edits, run `dodot up --provision-rerun`.

    Section 2 covers what each of these three looks like when nix itself is missing.

    For sentinels written before the snapshot convention was introduced, the third state shows `nix packages older version (no diff data)` — the run state is still tracked, but dodot has no record of the prior content to summarize what changed. Manual `nix profile remove` of a package the manifest still lists likewise stays sticky: the sentinel records "we ran with this content," and dodot considers the work done until the file changes or `--provision-rerun` is passed.

    To inspect the actual diff before deciding to re-run:

        dodot status --diff           # all packs
        dodot status tools --diff     # one pack

    For each `older version` entry, `--diff` prints a unified diff between the snapshot (the bytes that were last successfully installed) and the current manifest.

    Snapshots live alongside sentinels in the handler data dir: `<datastore>/packs/<pack>/nix/<filename>-<hash>.snapshot`. If you want to manage state directly, removing the sentinel + snapshot pair flips the file back to `nix packages not installed`.

7. "Ensure installed", not "owned by pack"

    The semantics of `dodot up` for this handler is *ensure these packages are installed in the user's Nix profile*. Not *these packages are here because of this pack*. Concretely:

    - dodot does not maintain a per-pack ownership manifest. It does not record "pack X installed package Y" for the purpose of later removal.
    - If a pack is removed, dodot does *nothing* to its packages. They stay installed.
    - If a pack's `packages.nix` shrinks from `[ ripgrep fd ]` to `[ ripgrep ]`, dodot also does *nothing* to `fd`. It stays installed.
    - `dodot down --uninstall` does not exist for this handler.

    This is not a soft default or a conservative first cut — it is the whole shape of the handler. The only command dodot can build for `packages.nix` is `nix profile install`; **`nix profile remove` appears nowhere in dodot's source**. There is no code path, no flag, and no failure mode that uninstalls a Nix package. Everything below follows from that one fact:

    - *Removing a line from `packages.nix` uninstalls nothing, ever.* The edit changes the content hash, so the next `up` reports `nix packages older version` and holds; `dodot up --provision-rerun` then installs the *remaining* list, which adds nothing and removes nothing.
    - *Deleting `packages.nix` outright removes nothing either.* `dodot up` wipes and re-applies the configuration handlers' state on every run, so a deleted source file stops being deployed — but provisioning state is deliberately left out of that reconcile, and provisioning side-effects were never dodot's to reverse.
    - *`dodot down` deletes the receipt, not the packages.* Teardown removes dodot's datastore state, so the file reads `nix packages not installed` afterwards and a later `dodot up` runs `nix profile install` again. Your profile is untouched by the `down` itself.

    Uninstalling is `nix profile remove`, run by hand against your own selectors, exactly as it would be if dodot had never been involved.

    This is also why packages install into the user's default profile rather than a dodot-owned side profile. A side profile would tacitly re-introduce ownership ("packages dodot put here") and break the property that packages persist past dodot's involvement. Installing into `~/.nix-profile` keeps dodot a *trigger* for installation, not an *owner* of the result. If you uninstall dodot, the packages stay.

    What dodot tracks is the sentinel, not the Nix profile. dodot does not call `nix profile list`, does not diff installed packages against the manifest, and does not skip its run because a package is already present. The sentinel records "we ran `nix profile install` against this content hash" — that is the entire state dodot tracks. Implications worth knowing:

    - **Manual `nix profile install` of the same package before dodot's first run doesn't suppress dodot's run.** With no sentinel on disk, the next `dodot up` will still invoke `nix profile install` against the manifest. `nix profile install` is not idempotent at the profile level — if the same package is already in the profile, Nix surfaces an error and the pack reports the failure. Reconcile by hand: either `nix profile remove` the manual entry before running dodot, or skip dodot's first invocation for that pack and let `dodot up --provision-rerun` apply once you've decided.
    - **Manual `nix profile remove` of a package the manifest still lists doesn't trigger a reinstall by dodot.** The sentinel says "we already ran with this content"; dodot considers the work done until the manifest changes or `--provision-rerun` is passed. Same shape as a manual `brew uninstall` against a `Brewfile`.

8. Pinning: what Nix can do that Homebrew cannot, and what dodot does not do

    The real difference between this handler and the homebrew one is not reliability, or determinism, or "Nix is reproducible." It is *pinning*, and it is a property of the manifests rather than of dodot:

    - A Nix expression *can* pin exactly what it installs — a fixed nixpkgs revision, a flake input with a locked hash. Two machines evaluating that expression get the same derivations. But you have to write that pinning into `packages.nix` yourself; nothing about the file being named `packages.nix` supplies it. The `{ pkgs ? import <nixpkgs> {} }:` form in section 3 is the *unpinned* form — it takes whatever your `NIX_PATH` points at on this host.
    - A `Brewfile` cannot do that portably. `brew "ripgrep"` installs whatever the tap holds today, and a run in six months installs something else. That is Homebrew's model, not a defect dodot can paper over.

    dodot itself does not yet have a pinned mode. It runs `nix profile install --impure` against whatever your expression evaluates to on this host, on every invocation, with no flag to do otherwise. So: writing a pinned manifest gets you a pinned install, and writing an unpinned one gets you whatever the host resolves — dodot passes it through either way and promises nothing about the outcome being the same on two machines. Section 11 records the pinned mode as out of scope for v1.

9. Configuration

    Under `[mappings]`:

        [mappings]
        nix = "deps.nix"

    :: toml ::

    Single string only — the nix handler claims one filename per pack. There's no dedicated `[nix]` section. The default mapping is `nix = "packages.nix"`.

10. Live edits

    Edits to `packages.nix` — adding or removing a package, switching a derivation — change its content hash. dodot detects the change but **does not re-run `nix profile install` automatically** — instead `dodot status` reports `nix packages older version` and `dodot up` skips it with the same notice. Apply the edits explicitly with `dodot up --provision-rerun`. See section 6 for the full three-state model and `--diff` workflow.

    Removing a package from the manifest is a special case worth stating twice: the re-run installs the shorter list and uninstalls nothing. Nothing dodot does will ever take a package back out of your profile — section 7.

    Channel updates that bump a package version (e.g. `ripgrep` 13 → 14) do **not** change the manifest's content hash, so dodot does not trigger a reinstall on its own. Run `nix profile upgrade '.*'` (or just the packages you care about) when you want newer versions; that's outside dodot's job.

    Removing the source `packages.nix` from the pack stops dodot from invoking `nix profile install`, but does **not** uninstall the packages it installed earlier — `nix profile remove` (against your own selectors) is the Nix-side mechanism for that, run by hand.

    On a machine without nix, an edit changes what the row says but not what happens: nothing runs either way. If the manifest had been installed here before, the row goes `nix packages older version` and the footnote adds that the re-run waits until nix is back; if it never ran here, the row stays `nix not installed` and your edit takes effect the first time you run `dodot up` on a machine that has Nix. Section 2 has both.

11. What this handler does not do

    Out of scope for v1 — flagged here so the surface stays predictable:

    - **`flake.nix` / `home.nix`.** Flakes are ambiguous (dev-shell, package, NixOS module, anything) and require an attribute selector the handler can't reliably infer. `home.nix` is not pack-composable: home-manager's single-user manifest can't compose across packs without dodot understanding home-manager's module system.
    - **Dotfile management, services, or shell init via Nix.** That's home-manager's territory, and dodot already handles dotfiles via the symlink handler. Users who want home-manager run it themselves.
    - **NixOS `configuration.nix` / nix-darwin `darwin-configuration.nix`.** System-level configuration requires root and has a blast radius incompatible with `dodot up`'s "edit and re-run cheaply" model.
    - **Auto-installing Nix.** Same posture as homebrew with `brew`. A host without nix gets a skipped row naming every path dodot probed and the project page (§2), and nothing else. Bootstrapping a package manager isn't a dotfile manager's job.
    - **Removing packages.** See §7. dodot says "ensure installed", and that is the whole commitment.
    - **Upgrading packages on channel drift.** See §10.
    - **Pinning or injecting `nixpkgs`.** See §8. The manifest's `{ pkgs ? import <nixpkgs> {} }` relies on the user's `NIX_PATH`, and dodot's invocation is unconditionally `--impure`. A future mode may let packs declare a pinned source.
    - **Mirroring / wrapping `apt` / `dnf` / `pacman` / `snap` / `flatpak`.** Users who need distro-package provisioning have `install.sh` — see [./install.lex].
