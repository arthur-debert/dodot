Proposal: Nix Handler for Declarative Per-Pack Provisioning

    This document specifies a Nix handler for dodot — a portable counterpart to the existing Brewfile handler, activated by a `packages.nix` file at pack root and applied via `nix profile install`. It is the Linux story for "edit a file, run dodot up, machine is closer to ready" that the Brewfile handler tells on macOS today, and it works equally well on macOS for users who prefer Nix to Homebrew.

    The handler is scoped narrowly on purpose. It installs packages into the user's default Nix profile and stops there: no home-manager, no system rebuild, no flake auto-detection, no apt/dnf/pacman fallback. The narrowness is what lets the handler stay pack-composable and lets `dodot up` retain its "ensure installed" semantics without owning the world.


1. Motivation

    1.1. The Linux Provisioning Gap

        dodot's Brewfile handler turns `dodot up` into a one-stop shop for macOS users: dotfiles, shell config, *and* the packages those dotfiles assume installed, all in one declarative invocation. Configuration files are about programs; having dodot apply the program-set alongside its config is what makes the experience feel coherent.

        Linux users have no equivalent default handler today. The closest fallback is `install.sh` — a bespoke shell script the pack author writes — which ends up calling `apt`, `dnf`, `pacman`, or `snap` with hand-rolled idempotency and a hand-rolled answer for every distro. Distro package managers have many warts, and a thin shim over them becomes an eternal whack-a-mole.

    1.2. The Criteria

        Three criteria define what could actually close this gap:

            1. *Significant audience.* Nothing on Linux is as universal as Homebrew on macOS, but a choice that over-indexes on the dotfile-using population is good enough.

            2. *Formal manifest mechanism.* The tool dodot wraps must have a first-class "apply this manifest" verb. A shell script invoking the package manager is not it.

            3. *Fast "is it installed?" check.* Without one, dodot has to fall back to hash-and-sentinel skipping, which is a workaround for slow tools, not a feature. With one, `dodot up` can ask the package manager what's installed and just install the delta.

        Nix is the only candidate that passes all three. `apt`, `dnf`, `pacman`, `snap`, `flatpak` all fail criterion 2 — none ship a curated user-facing manifest format, only system-wide state dumps or list-input flags. Homebrew-on-Linux is a freebie for users already on Linuxbrew (the existing Brewfile handler runs unchanged), but the audience is too narrow to be the default. `asdf` and `mise` solve a different problem (pinning dev-tool versions); plausible future sibling handlers, not Brewfile parallels.


2. Why Nix Passes

    Cross-platform: a Nix handler works on macOS too, and a macOS user who prefers declarative Nix to imperative Homebrew can use both handlers in the same pack tree.

    On formal manifest: `packages.nix` is a Nix expression evaluating to a set of packages. `nix profile install --file <path>` is its formal apply verb. dodot doesn't invent a manifest format; Nix's own evaluation is the format.

    On the install check: `nix profile list --json` is a local read of the profile manifest (well under a second in typical cases). dodot can compute the desired set, compare it to what's installed, and only invoke `nix profile install` for what's missing. No sentinel, no hash-and-skip dance. The check *is* the truth.


3. Pack Composability

    This is the architectural reason the handler is shaped as it is, and the reason an earlier draft of this proposal — which centered on `home.nix` and `home-manager switch` — was wrong.

    dodot's organizing unit is the pack. Each pack is independently up-able and owns its own slice of the user's environment. The composability invariant is: *one pack's contribution must not clobber another pack's contribution, and nothing dodot does to install one pack should interfere with the user's nix activity outside dodot.*

    `home.nix` violates this. It is a single user-wide manifest. Two packs each shipping a `home.nix` cannot both `home-manager switch` — each invocation overwrites the entire user environment, blasting away everything the other pack and the user themselves declared. The only way to make home-manager pack-composable would be for dodot to merge per-pack home-manager modules into a synthetic top-level config, which requires dodot to understand home-manager's module system — a category of complexity dodot has no business taking on.

    Per-pack `packages.nix` composes cleanly. Each pack lists the packages it wants. dodot collects the union across packs and installs it. Nothing one pack does affects another pack's install. Nothing dodot does touches a home-manager config the user maintains independently — they live on different rails.

    The cost of this scope choice: the Nix handler installs packages but does not manage dotfiles, services, or shell init via Nix. dodot already handles dotfiles via the symlink handler; the package-install side is the gap this handler closes. Users who want declarative dotfile-management-via-Nix can run home-manager themselves, outside dodot.


4. "Ensure Installed", Not "Owned by Pack"

    The semantics of `dodot up` for this handler is: *ensure these packages are installed in the user's Nix profile*. Not: *these packages are here because of this pack*. Not: *if this pack goes away, these packages should be uninstalled*.

    Concretely:

        - dodot does not maintain an ownership manifest. It does not record "pack X installed package Y" for the purpose of later removal.
        - On `dodot up`, dodot computes the union of all packs' `packages.nix` outputs, asks Nix what's already in the profile, and installs the missing packages.
        - If a pack is removed, dodot does *nothing* to its packages. They stay installed.
        - If a pack's `packages.nix` shrinks from `[ ripgrep fd ]` to `[ ripgrep ]`, dodot also does *nothing* to `fd`. It stays installed. The pack stopped declaring it, but dodot still has no basis to assert no one needs it — another pack may, the user may, the user may have grown to depend on it after the pack first installed it.
        - `dodot down --uninstall` does not exist in this proposal and is left as a separate (harder) design problem for whenever it comes up.

    This framing is what makes installing into the user's default profile — rather than a dodot-owned side profile — the right call. A side profile would tacitly re-introduce ownership ("packages dodot put here") and break the property that packages persist past dodot's involvement. Installing into `~/.nix-profile` keeps dodot a *trigger* for installation, not an *owner* of the result. If the user later `nix profile install`s the same package by hand, no conflict. If the user uninstalls dodot, packages stay. If a manual install reaches the package first, dodot's next `up` sees it already present and does nothing.


5. The Handler

    5.1. Trigger

        A file named `packages.nix` at pack root.

    5.2. Manifest Shape

        `packages.nix` evaluates to one of three Nix forms:

            - *List of derivations* (canonical, documented form):

                  { pkgs ? import <nixpkgs> {} }:
                  with pkgs; [ ripgrep fd bat ]

              :: nix ::

            - *Bare derivation* (single-package convenience — the common case for a one-tool pack like `bat` or `zoxide`):

                  { pkgs ? import <nixpkgs> {} }:
                  pkgs.zoxide

              :: nix ::

            - *Attribute set of derivations* (useful if a pack wants named attrs for tooling outside dodot):

                  { pkgs ? import <nixpkgs> {} }:
                  { ripgrep = pkgs.ripgrep; fd = pkgs.fd; }

              :: nix ::

        All three shapes require the `{ pkgs ? import <nixpkgs> {} }:` function wrapper with the default argument. The default is what makes `nix profile install --file` work without dodot injecting anything: Nix auto-applies functions with default arguments at evaluation time, resolving `pkgs` from the user's `NIX_PATH`. A `packages.nix` written as a bare list literal (no function wrapper) has no `pkgs` in scope and fails to evaluate.

        Documentation leads with the list form. If a pack author hands in something other than the three shapes above, the handler rejects it with a manifest-shape error (see §5.3). dodot does not "fix" malformed manifests.

    5.3. Apply Command

        The handler first determines the manifest shape with a cheap evaluation:

            nix eval --file <path> --json --apply 'x:
              if builtins.isList x then "list"
              else if builtins.isAttrs x && (x.type or "") == "derivation" then "drv"
              else if builtins.isAttrs x then "set"
              else "unsupported"'

        :: text ::

        For each shape, the install invocation is:

            - list → `nix profile install --file <path>`
            - drv  → `nix profile install --file <path>`
            - set  → `nix profile install --file <path> '.*'`

        Anything reported as `unsupported` (string, number, function that doesn't apply with defaults, etc.) is rejected with a manifest-shape error before any install is attempted. The `--file` form mirrors the Brewfile handler's `--file Brewfile`: explicit path, no implicit working-directory lookup, no environment-variable indirection.

        The handler does not check that `nix` is installed before invoking — same posture as Brewfile with `brew`. If the binary is missing, the executor surfaces the error and `dodot up` reports it.

    5.4. Phase and Category

        The handler runs in the *Provision* phase and the *CodeExecution* category, the same bucket as Brewfile and `install.sh`.

    5.5. Idempotency Without a Sentinel

        Sentinels exist as workarounds for slow or impossible "is this done?" checks:

            - `install.sh` is arbitrary user code; there is no general way to ask "is this script done", so the handler hashes the script and stamps a sentinel.
            - `Brewfile`'s `brew bundle check` typically triggers `brew update` (network, often 20s–2m). At 30 packs, full-pack checking is 7–42 minutes. A hash-based skip is the only viable workaround.

        Nix has neither problem. `nix profile list --json` reads a local manifest in well under a second. On each `dodot up`, the handler:

            1. Evaluates each pack's `packages.nix` and extracts the desired set of package names (`pname` of each derivation).
            2. Reads the current profile via `nix profile list --json` and extracts installed package names.
            3. For each pack, if any of its desired packages are not in the installed set, invokes the install command for that pack (shape-dispatched per §5.3). Packs whose desired set is fully present are skipped.

        No sentinel file is recorded. No hash is computed. The state of the profile is the truth, and dodot just keeps it in sync.

        *Diff key: `pname`.* The diff matches by package name (`pname`), not store path and not name-with-version. A pack listing `pkgs.ripgrep` is satisfied by any installed `ripgrep`, regardless of version or how it got there. This means:

            - A user's prior `nix profile install nixpkgs#ripgrep` satisfies a pack that lists `pkgs.ripgrep`. Correct under "ensure installed": the package is there, whoever put it there.
            - A `nixpkgs` channel update that bumps `ripgrep` from 13.0 to 14.0 does *not* trigger a reinstall. The existing entry's `pname` still matches; the diff is empty. `dodot up` is not an opportunistic upgrader. Users who want newer versions run `nix profile upgrade ripgrep` (or `nix profile upgrade '.*'`) themselves.

        Matching by store path is rejected because every channel update would make every package look uninstalled, conflating "drift" with "missing." Matching by full name-with-version has the same problem in milder form. `pname` is the right key for "ensure installed."

        *Partial failure.* Each pack's install invocation is independent. If pack A succeeds and pack B's install fails, dodot reports the failure for pack B, leaves pack A's installs in place (the user profile is purely additive, partial application is a benign state), and exits non-zero. The next `dodot up` re-diffs all packs and re-attempts the failed one. There is no rollback — Nix profile generations exist for users who want one, dodot does not orchestrate them.

        *`--force` for this handler.* `dodot up --force` is dodot's general "re-run regardless of skip state" flag. For this handler it has no useful effect: the diff is already the source of truth, so an empty diff means there is genuinely nothing to install. `--force` does not upgrade packages and does not reset the profile. Users wanting an upgrade should run `nix profile upgrade`; users wanting a reset should manage the profile directly via `nix profile remove` or by rolling generations. `--no-provision` skips the handler entirely, as it does for other Provision-phase handlers.

    5.6. Cross-Platform Coexistence

        Nix runs on Linux and macOS. The handler has no platform gate. A macOS user can have both a `Brewfile` and a `packages.nix` in different packs — or even in the same pack — and both handlers fire independently. They do not conflict: Brewfile manages a Homebrew-installed set, `packages.nix` manages a Nix-installed set, the two package managers do not overlap on disk.

        This coexistence is a feature, not an accident. A user migrating from Homebrew to Nix can do so pack-by-pack. A user who prefers Nix for cross-machine work and Homebrew for macOS-GUI casks can use each where it fits.


6. Configuration Surface

    The default mapping is added to the `[mappings]` section, alongside `homebrew`:

        [mappings]
        homebrew = "Brewfile"
        nix      = "packages.nix"

    :: toml ::

    A user who prefers a different filename overrides the mapping in their root or pack `.dodot.toml`. The handler name `nix` is short and reserves room to grow modes (flake-aware, pinned-nixpkgs) under one handler key later without renaming.


7. What dodot Does Not Do

    Out of scope for v1, deliberately:

        - *Match `flake.nix` or `home.nix`.* Flakes are ambiguous (dev shell, package, NixOS module, anything) and require an attribute selector the handler cannot reliably infer. `home.nix` is not pack-composable (§3).

        - *Manage dotfiles, services, or shell init via Nix.* That is home-manager's territory, and dodot already handles dotfiles via the symlink handler. Users who want home-manager run it themselves, on their own schedule, outside dodot.

        - *Touch NixOS `configuration.nix` or nix-darwin `darwin-configuration.nix`.* System-level configuration requires root and has a blast radius incompatible with `dodot up`'s "edit and re-run cheaply" model. The privilege boundary alone disqualifies it.

        - *Auto-install Nix.* If `nix` is not on PATH, the handler fails loudly. Same posture as Brewfile with `brew`. Bootstrapping a package manager is not a dotfile manager's job.

        - *Remove packages.* See §4. dodot says "ensure installed", and that is the whole commitment.

        - *Upgrade packages on channel drift.* See §5.5. `pname` matching means an installed `ripgrep` satisfies a pack listing `pkgs.ripgrep` regardless of version. Upgrades are the user's job.

        - *Pin or inject `nixpkgs`.* The manifest's `{ pkgs ? import <nixpkgs> {} }` relies on the user's `NIX_PATH`. A v2 mode (§9.2) may inject pinned sources.

        - *Mirror or wrap apt, dnf, pacman, snap, flatpak.* They don't pass criterion 2 (§1.2). Users who need distro-package provisioning have `install.sh`.


8. Open Decisions

    8.1. Minimum Nix Version

        The handler depends on `nix profile list --json`, `nix profile install --file`, and `nix eval --file --apply --json`. Pinning the floor at **Nix 2.18**: by that version the new CLI is widely available with `experimental-features = nix-command flakes` (or just `nix-command`) commonly enabled by default in popular installers, including the Determinate Systems installer and recent upstream defaults. `nix profile list --json` stabilized earlier (2.13-ish) but 2.18 is the safer floor for current-distribution alignment.

        On startup, the handler probes `nix --version`; if the parsed version is below 2.18, it errors with a clear message naming the required version and pointing at the installer docs. Earlier versions may work but are not tested.


9. Future Work

    9.1. Flake-Aware Mode

        Once `packages.nix` usage settles, a v2 could detect a sibling `flake.nix` and switch to `nix profile install <flake-ref>#<attr>`. The flake reference and attribute selector are the missing pieces — likely solved by a `[handlers.nix] flake_attr = "..."` config key. Not v1.

    9.2. Pinned `nixpkgs` Injection

        v1 relies on the user's `NIX_PATH` for `<nixpkgs>`, which means two users on the same pack can get different package versions depending on their channel. A v2 mode could let a pack declare a pinned `nixpkgs` source (flake input or `niv`-style JSON) and dodot would invoke evaluation against the pinned source via `--arg`. The shape interacts with §9.1 — settle both together.

    9.3. asdf / mise Sibling Handler

        Orthogonal. `.tool-versions` (asdf) and `mise.toml` (mise) pin dev-tool versions; not Brewfile parallels. A separate handler would slot into the same Provision-phase shape and invoke `asdf install` or `mise install`. Mentioned here so future-us does not accidentally absorb it into the Nix handler.


10. What This Costs the User

    Honest about the price tag:

        - One config-schema field added (`[mappings] nix = "packages.nix"`).
        - One handler-name reservation (`nix`).
        - For users already running Nix: zero new effort. `dodot up` starts driving per-pack package installation.
        - For users new to Nix: a real ramp. Nix has a learning curve, and the manifest-as-source-of-truth model is unfamiliar coming from imperative `nix profile install`.
        - No reproducibility guarantee across machines on different `NIX_PATH` channels. The same `packages.nix` may resolve to different package versions for different users (same as Brewfile, which doesn't pin versions either). Nix's reputation is built on reproducibility and users may assume they're getting it; user docs should be explicit that v1 does not. §9.2 outlines the pinning mode that would close this.

    The pack manifest is the source of truth. A manual `nix profile remove` of a package a pack still lists will be undone on the next `dodot up`. This matches Brewfile semantics; users coming from imperative `nix profile install` may not have internalized it, so user-facing docs should call it out explicitly.

    Honest about what this does not cost:

        - No change to Brewfile behavior. The two handlers are independent.
        - No platform gating. Linux and macOS users get the same handler.
        - No system-level surface. Nothing requires sudo or modifies `/etc`.
        - No automatic bootstrap. If `nix` is not installed, `dodot up` reports it like any other missing binary.
        - No interference with a user's existing home-manager setup. The handler does not touch home-manager.

    The shape we are aiming for: the same "edit a file, run dodot up, machine is closer to ready" experience that Brewfile gives macOS users, with the same scope, the same safety posture, and a tighter "ensure installed" semantic that survives manual user activity on the same profile.
