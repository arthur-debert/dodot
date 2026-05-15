:: verified ::
The nix handler

Runs `nix profile install` against your source `packages.nix` once per content-hash, tracked by a sentinel. The Linux counterpart to the homebrew handler — and a fine choice on macOS too if you prefer declarative Nix to imperative Homebrew. Mechanically a specialization of the install handler with a more ergonomic default for the common case: "install these packages on every machine I use, declared in one file."

1. Default claim

    A source file named `packages.nix` at the pack root. Single-string match — the nix handler claims one manifest per pack.

    Cross-platform: `nix` runs on Linux and macOS, so the handler has no OS gate. On a host without `nix` on PATH the install simply fails; use a `[pack] os` predicate or directory-gate (`_linux/`, `_darwin/`) if you need the pack itself to no-op on a host without Nix.

    Coexists cleanly with the homebrew handler — a single pack can ship both a `Brewfile` and a `packages.nix` and the two run independently against their own package managers.

2. Manifest shape

    `packages.nix` must evaluate (after applying any default-arg function wrapper) to one of:

    - **List of derivations** — the canonical form:

          { pkgs ? import <nixpkgs> {} }:
          with pkgs; [ ripgrep fd bat ]

      :: nix ::

    - **Bare derivation** — the common case for a one-tool pack:

          { pkgs ? import <nixpkgs> {} }:
          pkgs.zoxide

      :: nix ::

    The `{ pkgs ? import <nixpkgs> {} }:` function wrapper with a default argument is what makes `nix profile install --file <path>` work without dodot injecting anything: Nix auto-applies functions with defaulted arguments at evaluation time, resolving `pkgs` from the user's `NIX_PATH`. A bare list literal with no function wrapper has no `pkgs` in scope and fails to evaluate.

    The handler delegates shape detection to Nix itself via `nix eval --apply` so dodot owns no `packages.nix` parser.

    **Attribute-set manifests** (`{ ripgrep = pkgs.ripgrep; fd = pkgs.fd; }`) are recognized but **not yet supported in v1**. `nix profile install --file <path>` against an attribute set requires the `'.*'` selector argument, and per-shape install dispatch is deferred to a follow-up. The validator surfaces a clear error pointing at the list-form workaround when it sees an attribute set:

        packages.nix evaluates to an attribute set, which is not yet
        supported in v1. Please use the list form:
        `{ pkgs ? import <nixpkgs> {} }: with pkgs; [ <packages> ]`

    :: text ::

3. Pre-flight shape validation

    Before invoking `nix profile install`, the handler runs:

        nix eval --file <path> --json --apply '<probe>'

    :: text ::

    where `<probe>` is a small Nix expression that classifies the manifest into `list` / `drv` / `set` / `unsupported`. The probe is function-aware (`if builtins.isFunction f then f {} else f`) so the canonical wrapper resolves to the inner shape rather than reporting back as a bare lambda.

    If the manifest doesn't match an accepted shape, the validator fails the planning phase with a manifest-shape error. Note: this currently fires even when the file was previously installed and the user has just edited it into a broken shape; the install proceeds only after the user fixes the manifest. (Closing that gap so the notify-don't-rerun policy applies uniformly to nix is tracked as follow-up work — see the source comment in `crates/dodot-lib/src/handlers/nix.rs`.)

4. Sentinels

    On success, dodot writes a sentinel file `<filename>-<checksum>` into the datastore — for example `packages.nix-a1b2c3d4e5f6a7b8`. The checksum is the first 8 bytes (16 hex chars) of a SHA-256 of the source manifest's bytes. Alongside it dodot also writes a sibling file `<filename>-<checksum>.snapshot` containing the manifest bytes as they were at the time of that run, so a future `dodot status` can show what changed.

    Same flag set as install / homebrew:

    - `--no-provision` — skip install / homebrew / nix handlers entirely on this run.
    - `--provision-rerun` — the canonical "apply pending content edits" escape hatch for run-once handlers. Re-executes nix even when a sentinel exists. Use it after editing `packages.nix` to opt back into running the new content.
    - `--force` — overwrite pre-existing files at symlink target paths. Distinct from `--provision-rerun`; does **not** trigger run-once re-execution.

5. Editing `packages.nix` after it ran (the three states)

    When you edit `packages.nix` after a successful install, dodot does **not** re-run `nix profile install` automatically. The conservative posture is to *notify* and let you decide.

    `dodot up` and `dodot status` report one of three states:

    - **`nix packages not installed`** — no sentinel exists. `dodot up` will run `nix profile install` on the next invocation.
    - **`nix packages installed`** — a sentinel exists for the *current* content hash. The install has run, and the source hasn't changed since. `dodot up` is a no-op.
    - **`nix packages older version (N lines added, M removed)`** — a sentinel exists, but for a *different* content hash. The install ran successfully against an earlier version of `packages.nix`, and you've edited it since. `dodot up` does not auto-rerun. To apply the edits, run `dodot up --provision-rerun`.

    For sentinels written before the snapshot convention was introduced, the third state shows `nix packages older version (no diff data)` — the run state is still tracked, but dodot has no record of the prior content to summarize what changed. Manual `nix profile remove` of a package the manifest still lists likewise stays sticky: the sentinel records "we ran with this content," and dodot considers the work done until the file changes or `--provision-rerun` is passed.

    To inspect the actual diff before deciding to re-run:

        dodot status --diff           # all packs
        dodot status tools --diff     # one pack

    For each `older version` entry, `--diff` prints a unified diff between the snapshot (the bytes that were last successfully installed) and the current manifest.

    Snapshots live alongside sentinels in the handler data dir: `<datastore>/packs/<pack>/nix/<filename>-<hash>.snapshot`. If you want to manage state directly, removing the sentinel + snapshot pair flips the file back to `nix packages not installed`.

6. "Ensure installed", not "owned by pack"

    The semantics of `dodot up` for this handler is *ensure these packages are installed in the user's Nix profile*. Not *these packages are here because of this pack*. Concretely:

    - dodot does not maintain a per-pack ownership manifest. It does not record "pack X installed package Y" for the purpose of later removal.
    - If a pack is removed, dodot does *nothing* to its packages. They stay installed.
    - If a pack's `packages.nix` shrinks from `[ ripgrep fd ]` to `[ ripgrep ]`, dodot also does *nothing* to `fd`. It stays installed.
    - `dodot down --uninstall` does not exist for this handler.

    This is why packages install into the user's default profile rather than a dodot-owned side profile. A side profile would tacitly re-introduce ownership ("packages dodot put here") and break the property that packages persist past dodot's involvement. Installing into `~/.nix-profile` keeps dodot a *trigger* for installation, not an *owner* of the result. If the user uninstalls dodot, the packages stay.

    What dodot tracks is the sentinel, not the Nix profile. dodot does not call `nix profile list`, does not diff installed packages against the manifest, and does not skip its run because a package is already present. The sentinel records "we ran `nix profile install --file <path>` against this content hash" — that is the entire state dodot tracks. Implications worth knowing:

    - **Manual `nix profile install` of the same package before dodot's first run doesn't suppress dodot's run.** With no sentinel on disk, the next `dodot up` will still invoke `nix profile install --file <path>`. `nix profile install` is not idempotent at the profile level — if the same package is already in the profile, Nix surfaces an error and the pack reports the failure. Reconcile by hand: either `nix profile remove` the manual entry before running dodot, or skip dodot's first invocation for that pack and let `dodot up --provision-rerun` apply once you've decided.
    - **Manual `nix profile remove` of a package the manifest still lists doesn't trigger a reinstall by dodot.** The sentinel says "we already ran with this content"; dodot considers the work done until the manifest changes or `--provision-rerun` is passed.

7. Configuration

    Under `[mappings]`:

        [mappings]
        nix = "deps.nix"

    :: toml ::

    Single string only — the nix handler claims one filename per pack. There's no dedicated `[nix]` section. The default mapping is `nix = "packages.nix"`.

8. Live edits

    Edits to `packages.nix` — adding or removing a package, switching a derivation — change its content hash. dodot detects the change but **does not re-run `nix profile install` automatically** — instead `dodot status` reports `nix packages older version` and `dodot up` skips it with the same notice. Apply the edits explicitly with `dodot up --provision-rerun`. See section 5 for the full three-state model and `--diff` workflow.

    Channel updates that bump a package version (e.g. `ripgrep` 13 → 14) do **not** change the manifest's content hash, so dodot does not trigger a reinstall on its own. Run `nix profile upgrade '.*'` (or just the packages you care about) when you want newer versions; that's outside dodot's job.

    Removing the source `packages.nix` from the pack stops dodot from invoking `nix profile install`, but does **not** uninstall the packages it installed earlier — `nix profile remove` (against your own selectors) is the Nix-side mechanism for that, run by hand.

9. What this handler does not do

    Out of scope for v1 — flagged here so the surface stays predictable:

    - **`flake.nix` / `home.nix`.** Flakes are ambiguous (dev-shell, package, NixOS module, anything) and require an attribute selector the handler can't reliably infer. `home.nix` is not pack-composable: home-manager's single-user manifest can't compose across packs without dodot understanding home-manager's module system.
    - **Dotfile management, services, or shell init via Nix.** That's home-manager's territory, and dodot already handles dotfiles via the symlink handler. Users who want home-manager run it themselves.
    - **NixOS `configuration.nix` / nix-darwin `darwin-configuration.nix`.** System-level configuration requires root and has a blast radius incompatible with `dodot up`'s "edit and re-run cheaply" model.
    - **Auto-installing Nix.** Same posture as homebrew with `brew`. If `nix` isn't on PATH, the handler fails loudly. Bootstrapping a package manager isn't a dotfile manager's job.
    - **Removing packages.** See §6. dodot says "ensure installed", and that is the whole commitment.
    - **Upgrading packages on channel drift.** See §8.
    - **Pinning or injecting `nixpkgs`.** The manifest's `{ pkgs ? import <nixpkgs> {} }` relies on the user's `NIX_PATH`. A future mode may let packs declare a pinned source.
    - **Mirroring / wrapping `apt` / `dnf` / `pacman` / `snap` / `flatpak`.** Users who need distro-package provisioning have `install.sh`.
