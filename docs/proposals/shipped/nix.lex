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

        The handler invokes a single install command for every accepted manifest shape:

            nix profile install --expr <wrapper-expr> \
                --extra-experimental-features 'nix-command flakes'

        :: text ::

        `<wrapper-expr>` is a shape-normalizing Nix expression the handler builds at command-construction time. It imports the user's `packages.nix`, applies the outer function with `{}` when one is present (which resolves the `{ pkgs ? import <nixpkgs> {} }:` default), and collapses the resulting value to a list of derivations:

            let
              raw = import "<path>";
              m   = if builtins.isFunction raw then raw {} else raw;
            in
              if builtins.isList m then m
              else if builtins.isAttrs m && (m.type or null) == "derivation" then [ m ]
              else if builtins.isAttrs m then builtins.attrValues m
              else throw "packages.nix at <path> evaluates to an unsupported shape ..."

        :: nix ::

        With the manifest collapsed to a list, `nix profile install --expr` installs it directly. The install command is identical for list, bare-derivation, and attribute-set manifests; the handler does not classify the manifest at planning time and does not run a separate `nix eval --apply` shape probe.

        Malformed content (syntax errors, missing `pkgs`, unsupported shapes) surfaces as a `nix` subprocess error at apply time — the same way a broken `Brewfile` surfaces a `brew bundle` error and a broken `install.sh` surfaces a `bash` error. This _no planning-time content validation_ posture is the lifecycle invariant shared with the other run-once handlers (see §5.5); it is what keeps the handler from accumulating per-package-manager parsing logic it could never keep aligned with the upstream tool.

        The handler does not check that `nix` is installed before invoking — same posture as Brewfile with `brew`. If the binary is missing, the executor surfaces the error and `dodot up` reports it.

        The `--extra-experimental-features 'nix-command flakes'` flag is passed defensively on every invocation. It is a no-op when the features are already enabled in the user's `nix.conf`; it guards against fresh Nix installs that have not yet opted into the new CLI.

        _Design history._ An earlier version of this spec (v5) called for a planning-time `nix eval --file --json --apply '<shape-classifier>'` followed by shape-dispatched `nix profile install --file <path>` (with `'.*'` for attribute sets). The pivot to the wrapper expression dropped the eval probe entirely. Reasons: (1) one install command, no per-shape branch, removes a class of accidentally-divergent handling; (2) shape classification at plan time would be the only run-once handler doing planning-time content validation, breaking the lifecycle invariant called out in §5.5; (3) attribute-set manifests now install with no special selector, instead of being rejected as v1 originally intended. The change shipped in PR 3 of #161.

    5.4. Phase and Category

        The handler runs in the *Provision* phase and the *CodeExecution* category, the same bucket as Brewfile and `install.sh`.

    5.5. Run-Once Lifecycle

        The handler uses the same hash + sentinel + snapshot machinery as `install` and `homebrew`, via the shared `RunOnceHandler` introduced in #169. On each `dodot up`:

            1. The handler hashes `packages.nix` (Blake3, rendered bytes preferred; on-disk bytes as fallback).
            2. `DataStore::did_run` classifies the file as `NeverRan`, `RanCurrent`, or `RanDifferent` against any previously recorded sentinel for this (pack, "nix", "packages.nix") triple.
            3. `NeverRan` → run the install (sentinel + `<sentinel>.snapshot` written on success). `RanCurrent` → skip silently. `RanDifferent` → skip with notice; `dodot status` reports "nix packages older version (N lines added, M removed)".
            4. `dodot up --provision-rerun` bypasses both skip cases and re-runs against the current content. `--no-provision` skips the handler entirely, as for other Provision-phase handlers.

        Sentinel content and naming match the format used by `install` and `homebrew`. The snapshot sibling enables `dodot status --diff <pack>` to show the unified diff between the recorded content and the current `packages.nix`.

        _Why a sentinel, not `nix profile list --json` diffing?_

        An earlier version of this spec (v5 §5.5) argued that Nix could sidestep sentinels by diffing the desired package set against `nix profile list --json` on every `up`. The argument: `install.sh` and `Brewfile` need sentinels because their "is this done?" check is either impossible (arbitrary script) or slow (`brew bundle check` triggers `brew update`), but Nix has a sub-second local manifest read. That argument did not survive contact with the run-once handlers' actual job.

        The thing being run is arbitrary user-supplied content. Even for Brew and Nix — bounded compared to `install.sh` — the _state outside the file_ dwarfs the state inside it:

            - Packages may already be installed before `dodot up` ever runs. The pack lists them; the profile has them; nothing happened.
            - The same `pname` can resolve to multiple store paths (different versions, channel drift, manual `nix profile install` of a different `nixpkgs` ref).
            - System packages, environment-managed binaries, and prior `nix profile install nixpkgs#<name>` invocations can shadow what a pack expects.
            - User configuration (`nix.conf`, `NIX_PATH`, environment overrides) alters what "installed" even means.

        Getting a normalized "what would this manifest do, and is it already done?" answer requires reimplementing the package manager's resolution logic. dodot does not. There is no truthful diff dodot can compute without owning Nix-internal logic — and owning Nix-internal logic is exactly what the proposal's "delegate to Nix" principle (§4) rejects.

        What dodot can answer truthfully is a much narrower question: _did we, dodot, run this exact file successfully?_ That is the sentinel. The same question applies uniformly to `install.sh`, `Brewfile`, and `packages.nix`: a successful run is recorded with the content hash; a subsequent run with the same hash is a skip; a subsequent run with a different hash is reported as "older version" and held until the user explicitly opts in via `--provision-rerun`. The shape of the answer is identical across the three handlers, because the underlying epistemic problem — _we cannot inspect the world richly enough to second-guess what we did_ — is identical.

        Consequences worth being honest about:

            - _Pre-installed packages are not detected._ A pack listing `pkgs.ripgrep` will invoke `nix profile install` on first run even if `ripgrep` is already on the user's profile. `nix profile install` is generally tolerant of this (it adds a profile element pointing at the same or a different store path; the user can `nix profile remove` extras). Same posture as Brewfile invoking `brew bundle` against already-installed packages.
            - _Manual `nix profile remove` is sticky._ Once dodot has recorded a successful run for a hash, removing one of the installed packages by hand does not cause dodot to reinstall on the next `up`. The sentinel records "we ran with this content"; dodot considers the work done until the file content changes or the user runs `dodot up --provision-rerun`. Mirrors Brewfile: a manual `brew uninstall` of a package the Brewfile still lists also stays sticky.
            - _Channel drift does not trigger reinstall._ A `nixpkgs` channel update that bumps `ripgrep` from 13.0 to 14.0 does not change the `packages.nix` content hash, so dodot takes no action. Users wanting the newer version run `nix profile upgrade ripgrep` themselves. dodot is not an opportunistic upgrader, by design.

        _Partial failure._ If multiple packs each carry a `packages.nix` and one pack's install fails, the failing pack does not block the others — pack errors are surfaced as per-pack failures rather than aborting `dodot up` for the whole tree. Successfully-run packs have their sentinels written; the failing pack will be re-attempted on the next `up`. The Nix profile is purely additive; partial application is a benign state and there is no rollback (Nix profile generations exist for users who want one; dodot does not orchestrate them).

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

        - *Upgrade packages on channel drift.* See §5.5. The sentinel records "we ran with this content"; channel drift does not change file content, so dodot takes no action. Users wanting newer versions run `nix profile upgrade` themselves.

        - *Pin or inject `nixpkgs`.* The manifest's `{ pkgs ? import <nixpkgs> {} }` relies on the user's `NIX_PATH`. A v2 mode (§9.2) may inject pinned sources.

        - *Mirror or wrap apt, dnf, pacman, snap, flatpak.* They don't pass criterion 2 (§1.2). Users who need distro-package provisioning have `install.sh`.


8. Open Decisions

    8.1. Minimum Nix Version

        The handler depends on `nix profile install --expr` and `--extra-experimental-features 'nix-command flakes'`. Pinning the floor at *Nix 2.18*: by that version the new CLI is widely available, `experimental-features = nix-command flakes` (or just `nix-command`) is commonly enabled by default in popular installers (Determinate Systems installer, recent upstream defaults), and `--expr` is stable. 2.18 is the safer floor for current-distribution alignment.

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

    The pack manifest is the source of truth at the moment the handler first records a successful run; after that, the sentinel pins the state (§5.5). A manual `nix profile remove` of a package the pack still lists is _not_ undone on the next `dodot up` — the sentinel records "we ran with this content" and dodot considers the work done until the file content changes or the user runs `dodot up --provision-rerun`. This matches Brewfile semantics. Users coming from imperative `nix profile install` may not have internalized it, so user-facing docs should call it out explicitly.

    Honest about what this does not cost:

        - No change to Brewfile behavior. The two handlers are independent.
        - No platform gating. Linux and macOS users get the same handler.
        - No system-level surface. Nothing requires sudo or modifies `/etc`.
        - No automatic bootstrap. If `nix` is not installed, `dodot up` reports it like any other missing binary.
        - No interference with a user's existing home-manager setup. The handler does not touch home-manager.

    The shape we are aiming for: the same "edit a file, run dodot up, machine is closer to ready" experience that Brewfile gives macOS users, with the same scope, the same safety posture, and a tighter "ensure installed" semantic that survives manual user activity on the same profile.
