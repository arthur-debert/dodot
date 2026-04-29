Design Specification: MacOs Paths and the `_app/` Convention

    This document specifies how dodot routes pack files on MacOs, where a third filesystem coordinate — `~/Library/Application Support/<App>/` — sits alongside the two roots the resolver already understands (`$HOME` and `$XDG_CONFIG_HOME`). It extends the symlink-deployment ladder defined in [./../reference/symlink-paths.lex] with two new directory prefixes (`_app/` and `_lib/`), a curated `force_app` list, and a pack-level `app_aliases` map. It also describes an advisory layer — homebrew-cask and MacOs-native introspection — that powers `dodot adopt` suggestions and `dodot up` warnings without ever overriding the resolver's deterministic priority order.

    The spec is split into two parts. The first half (sessions 1–7) is the deterministic core: where files end up, why, and how to express user intent. The second half (session 8) is the advisory "automagical" layer that sits on top, isolated so it does not perturb the resolver and can evolve independently.

1. The Problem

    This proposal addresses a MacOs-specific gap: on Linux, GUI and CLI applications share the same XDG-compliant text-file hierarchy and the resolver's existing rules already cover them, whereas MacOs splits GUI app data into `~/Library/Application Support/` with app-specific folder names that rarely match the natural pack name.

    1.1. Two Roots Are Not Enough

        dodot's resolver recognizes two filesystem roots: `$HOME` (legacy unix canons — `.bashrc`, `.ssh/`, `.gnupg/`) and `$XDG_CONFIG_HOME` (everything else, namespaced under the pack name by default). The four escape hatches in `symlink-paths.lex` (`home.X`, `_home/`, `_xdg/`, `[symlink.targets]`) all express user intent within those two coordinates.

        MacOs GUI applications read configuration from a third coordinate: `~/Library/Application Support/<App>/`. VS Code reads `~/Library/Application Support/Code/User/settings.json` on MacOs but `~/.config/Code/User/settings.json` on Linux. Same logical file, different root, *and the folder name (`Code`) often differs from the natural pack name (`vscode`)*.

        Today the only way to land a file under `~/Library/Application Support/` in dodot is `[symlink.targets]` with an absolute path. That hardcodes the platform into the pack and forfeits the pack-relative composability the `_xdg/` and `_home/` prefixes were introduced to provide.

    1.2. `XDG_CONFIG_HOME` Cannot Be the Switch

        A tempting solution would be to redirect `$XDG_CONFIG_HOME` itself to `~/Library/Application Support/` on MacOs — exactly what the Rust `directories` crate does. dodot rejects this approach. Modern CLI tools on MacOs (nvim, helix, ghostty, kitty, lazygit, starship, gh, …) use `~/.config/` natively and would break under such a flip. MacOs users routinely have both kinds of tools coexisting on the same machine. The choice between roots cannot be a per-machine env-var; it must be a per-target decision encoded in the pack.

    1.3. Cross-Platform Packs

        A `vscode` pack containing `User/settings.json` should deploy to:

            - Linux:  `~/.config/Code/User/settings.json`
            - MacOs:  `~/Library/Application Support/Code/User/settings.json`

        Both the root *and* the app-folder name diverge from a naive `vscode/User/settings.json` reading. dodot needs a way for the pack to express "this is GUI-app config under the name X", once, with the OS choosing the root and an alias mechanism resolving the pack-name → app-folder mismatch.

    1.4. Goals

        - Express "this is GUI-app config" once per subtree, in a way that does the right thing on Linux *and* MacOs, without `if os == "darwin"` branching inside packs.
        - Slot cleanly into the existing §1–§5 priority ladder as new rungs, not as parallel machinery.
        - Keep the resolver deterministic over textual prefixes; OS-awareness lives in one place (`Pather`).
        - Make `dodot adopt ~/Library/Application\ Support/Code/User/settings.json` Just Work — produce a pack-relative path that round-trips on both platforms.
        - Avoid mackup-style maintenance debt. Ship a small curated list of well-known apps with a hard cap; do not maintain a 700-entry database.

2. Three Roots, Not Two

    2.1. The `Pather` Surface

        A new accessor on the `Pather` trait exposes the application-support root:

        Pather addition:

            fn app_support_dir(&self) -> &Path;

        :: rust ::

        Resolution in `XdgPatherBuilder::build()`:

            - MacOs:           `$HOME/Library/Application Support`
            - Linux and other: `$XDG_CONFIG_HOME` (so `_app/` falls through cleanly to the existing root)

        Overridable via `XdgPatherBuilder::app_support_dir(...)` for tests and for users who deliberately want MacOs to route through `~/.config/` like Linux. The OS check lives exclusively in this builder; the resolver stays platform-agnostic and operates only on textual prefixes.

    2.2. Three Coordinates Going Forward

        After this proposal lands, every symlink target resolves into exactly one of three roots:

        Roots:
            | Symbol             | MacOs                                 | Linux / other         |
            | `$HOME`            | `/Users/<user>`                       | `/home/<user>`        |
            | `$XDG_CONFIG_HOME` | `~/.config` (unless env-set)          | `~/.config` (unless env-set) |
            | `app_support_dir`  | `~/Library/Application Support`       | `$XDG_CONFIG_HOME`    |
        :: table align=lll ::

        On Linux the second and third coordinates collapse to one filesystem location. On MacOs they diverge. The resolver always knows which of the three a target belongs to; the OS only enters the picture once, when `Pather` materialises `app_support_dir`.

3. The `_app/` Directory Prefix

    3.1. Behavior

        `_app/` is the per-subtree opt-in for "this is GUI-application config". Like `_xdg/` and `_home/`, it skips pack namespacing entirely: every file under `_app/` deploys raw, with the pack name playing no role.

        Mapping:

            <pack>/_app/<name>/<rest>  →  <app_support_dir>/<name>/<rest>

        Concretely, a portable `vscode` pack:

        Pack layout:

            vscode/
                _app/
                    Code/
                        User/
                            settings.json
                            keybindings.json

        :: text ::

        Deploys to:

            - Linux:  `~/.config/Code/User/settings.json`
            - MacOs:  `~/Library/Application Support/Code/User/settings.json`

        The pack literally states "this is GUI-app config under name `Code`". dodot picks the root per platform.

    3.2. Position in the Priority Ladder

        `_app/` is an explicit per-subtree user opt-in, structurally identical to `_xdg/` and `_home/`. It joins them at Priority 2 of the resolver (see §5). It outranks `force_home` and the default rule, but is outranked by `home.X` (per-file opt-in, more specific) and `[symlink.targets]` (explicit absolute override).

    3.3. The `app_aliases` Map

        Cross-platform packs frequently want the *natural* pack name (`vscode`) without writing `_app/Code/` for every entry. The `[symlink.app_aliases]` table lets a user declare a pack-level rewrite:

        Pack-level alias:

            [symlink.app_aliases]
            vscode = "Code"
            warp   = "dev.warp.Warp-Stable"

        :: toml ::

        When a pack name appears as a key in `app_aliases`, the *default rule* for that pack is rerouted: instead of `$XDG_CONFIG_HOME/<pack>/<rel_path>`, the deploy path becomes `<app_support_dir>/<value>/<rel_path>`. The pack `vscode` with `User/settings.json` then deploys to `~/Library/Application Support/Code/User/settings.json` on MacOs, `~/.config/Code/User/settings.json` on Linux — without any `_app/` prefix in the pack tree.

        Aliases compose with the rest of the ladder:

            - A `home.X` file inside an aliased pack still routes to `$HOME/.X` (priority 1 outranks the aliased default).
            - A `_xdg/` subtree inside an aliased pack still deploys raw under `$XDG_CONFIG_HOME` (explicit user intent wins over the alias).
            - A `[symlink.targets]` entry still wins absolutely.

        Aliases are scoped to the alias map's defining `.dodot.toml`. A pack-level `.dodot.toml` setting `[symlink.app_aliases]` overrides the root `.dodot.toml`. A pack-level alias is the recommended location, since the alias is logically a property of the pack.

    3.4. The Curated `force_app` List

        Analogous to `force_home`, `force_app` is a curated list of top-level pack-entry names that get auto-routed to the app-support root *without* requiring a `_app/` prefix or an alias entry. It exists to make the high-impact common case (well-known editors, terminals, productivity apps) magical.

        Schema:

            [symlink]
            force_app = [
                "Code",
                "Cursor",
                "Sublime Text",
                "Rectangle Pro",
                # … capped at ~100 entries; see §3.4.1
            ]

        :: toml ::

        Matching follows `force_home` semantics: the *first segment* of the pack-relative path is compared against entries (case-sensitive — Library folder names are case-sensitive).

        3.4.1. The Hundred-Entry Cap

            The default `force_app` list is capped at 100 entries, enforced as a CI invariant. Adding entry 101 is a forcing function to drop the weakest-justified existing entry. This keeps the list a curated convenience, not an open-ended database. The cap is not configurable; users who want more redirects use `_app/` or `app_aliases`.

            Inclusion criterion (documented in the source file, one-line per entry):

                - The app must be widely deployed in the developer-tools community.
                - Its Application Support folder name must be stable across versions.
                - The folder name must not collide with any plausible CLI tool's `~/.config/` directory.

            Entries that fail the third criterion (e.g. an app whose folder name happens to be `vim`) are excluded by construction.

        3.4.2. Initial Seed

            The initial list is empty. Entries are added based on real user demand once the mechanism ships. This avoids day-one wrong defaults and lets the cap-100 discipline establish itself organically.

4. The `_lib/` Directory Prefix

    4.1. Behavior

        `_lib/` is the MacOs-only counterpart to `_app/`. Where `_app/` cross-routes between two platforms, `_lib/` declares a hard MacOs-only target — appropriate for apps with no Linux equivalent (`Rectangle Pro`, `Karabiner-Elements`, MacOs-native plist consumers).

        Mapping (MacOs):

            <pack>/_lib/<rest>  →  $HOME/Library/<rest>

        Note that `_lib/` maps to `~/Library/`, *not* to `~/Library/Application Support/`. This gives access to other Library subtrees (e.g. `LaunchAgents/`, `Fonts/`, `Services/`) without further prefix proliferation. The user writes the full subpath:

        Examples:

            <pack>/_lib/Application Support/Rectangle Pro/RectanglePro.json
                →  ~/Library/Application Support/Rectangle Pro/RectanglePro.json

            <pack>/_lib/LaunchAgents/com.example.foo.plist
                →  ~/Library/LaunchAgents/com.example.foo.plist

        :: text ::

    4.2. Linux Behavior

        On non-MacOs platforms, `_lib/` emits no symlink intent and produces a soft warning:

        Warning text:

            warning: pack `<pack>` contains `_lib/<rest>` — MacOs-only path,
                     skipping on this platform

        :: text ::

        The pack is otherwise unaffected; other entries deploy normally. This matches the friction level of a missing source file: visible, recoverable, non-fatal.

5. Path Resolution: Full Specification

    5.1. The Updated Priority Ladder

        The resolver in `crates/dodot-lib/src/handlers/symlink.rs` evaluates the following rules in order. The first matching rule wins. This is a strict superset of the existing ladder; sessions 0, 1, 3, 4 are unchanged from `symlink-paths.lex`.

        Priorities (highest first):

            0. Custom target from `[symlink.targets]`
            1. `home.X` prefix (top-level files only) → `$HOME/.X`
            2. Directory prefixes (per-subtree, skip pack namespace):
                a. `_home/<rest>` → `$HOME/.<rest>`
                b. `_xdg/<rest>`  → `$XDG_CONFIG_HOME/<rest>`
                c. `_app/<rest>`  → `<app_support_dir>/<rest>`
                d. `_lib/<rest>`  → `$HOME/Library/<rest>` (MacOs only; warn elsewhere)
            3. `force_home` list → `$HOME/.<first-segment>/<rest>`
            4. `force_app` list → `<app_support_dir>/<first-segment>/<rest>`
            5. `app_aliases[pack]` → `<app_support_dir>/<alias>/<rel_path>`
            6. Default → `$XDG_CONFIG_HOME/<pack-display-name>/<rel_path>`

        Tie-breaking notes:

            - Within Priority 2, only one prefix can match a given path (they are mutually exclusive by syntax).
            - `force_home` and `force_app` are mutually exclusive; entries appearing in both lists are a configuration error and produce a startup warning.
            - `app_aliases` only applies when no higher-priority rule matched — it modifies *the default*, not user-explicit prefixes.

    5.2. Resolver Pseudocode

        Pseudocode:

            fn resolve_target(pack, rel_path, config, paths) -> PathBuf {
                let pack = display_name_for(pack);  // strip ordering prefix
                let home = paths.home_dir();
                let xdg  = paths.xdg_config_home();
                let app  = paths.app_support_dir();

                // 0. Custom target override
                if let Some(t) = config.targets.get(rel_path) {
                    return if t.is_absolute() { t } else { xdg.join(t) };
                }

                // 1. home.X (top-level files only)
                if let Some(rest) = strip_home_prefix(rel_path) {
                    return home.join(rest);
                }

                // 2. Directory-prefix escape hatches
                if let Some(r) = rel_path.strip_prefix("_home/")  { return home.join(dot_first(r)); }
                if let Some(r) = rel_path.strip_prefix("_xdg/")   { return xdg.join(r); }
                if let Some(r) = rel_path.strip_prefix("_app/")   { return app.join(r); }
                if let Some(r) = rel_path.strip_prefix("_lib/")   {
                    return if cfg!(target_os = "MacOs") { home.join("Library").join(r) }
                           else { warn_and_skip() };
                }

                // 3. force_home
                if is_force_home(rel_path, &config.force_home) {
                    return home.join(dot_first(rel_path));
                }

                // 4. force_app
                if is_force_app(rel_path, &config.force_app) {
                    return app.join(rel_path);
                }

                // 5. app_aliases (pack-level rewrite of the default)
                if let Some(alias) = config.app_aliases.get(pack) {
                    return app.join(alias).join(rel_path);
                }

                // 6. Default
                xdg.join(pack).join(rel_path)
            }

        :: rust ::

    5.3. Worked Examples

        All examples assume `XDG_CONFIG_HOME=~/.config`, MacOs unless noted, and `[symlink.app_aliases] vscode = "Code"`.

        Examples:
            | Pack/path                            | Resolution                                                          |
            | `vscode/User/settings.json`          | `~/Library/Application Support/Code/User/settings.json` (rule 5)    |
            | `vscode/User/settings.json` (Linux)  | `~/.config/Code/User/settings.json` (rule 5)                        |
            | `vscode/_xdg/Code/User/foo`          | `~/.config/Code/User/foo` (rule 2b — explicit overrides alias)      |
            | `mac-apps/_app/Cursor/User/keys`     | `~/Library/Application Support/Cursor/User/keys` (rule 2c)          |
            | `mac-apps/_lib/LaunchAgents/x.plist` | `~/Library/LaunchAgents/x.plist` (rule 2d, MacOs)                   |
            | `mac-apps/_lib/...` (Linux)          | (skipped, warning emitted)                                          |
            | `nvim/init.lua`                      | `~/.config/nvim/init.lua` (rule 6)                                  |
            | `shell/home.bashrc`                  | `~/.bashrc` (rule 1)                                                |
            | `net/ssh/config`                     | `~/.ssh/config` (rule 3, force_home)                                |
            | `apps/Code/User/foo` + force_app     | `~/Library/Application Support/Code/User/foo` (rule 4)              |
        :: table align=ll ::

6. Configuration Surface

    6.1. Schema Additions

        Added `[symlink]` keys:

            [symlink]
            # When true, `_app/` and `app_aliases` route through
            # ~/Library/Application Support on MacOs. Default true on
            # darwin, false elsewhere. Set false on MacOs to opt into
            # Linux-style ~/.config behavior for everything.
            app_uses_library = true

            # Curated list. Top-level pack entries whose first segment
            # matches deploy to <app_support_dir>/<name>/. Capped at 100.
            force_app = []

        :: toml ::

        Added `[symlink.app_aliases]` table:

            [symlink.app_aliases]
            vscode = "Code"
            warp   = "dev.warp.Warp-Stable"

        :: toml ::

    6.2. Inheritance

        Follows the existing 3-layer hierarchy: compiled defaults < root `.dodot.toml` < pack `.dodot.toml`. `app_aliases` is most idiomatically declared in pack-level config since it is logically a property of the pack.

    6.3. Defaults by Platform

        | Setting             | MacOs default | Linux default                       |
        | `app_uses_library`  | `true`        | `false` (and ignored)               |
        | `force_app`         | `[]`          | `[]`                                |
        | `app_aliases`       | `{}`          | `{}` (pack-level overrides apply equally) |
        :: table align=lll ::

        On Linux with `app_uses_library = false`, `_app/` and `app_aliases` continue to route, but `app_support_dir` resolves to `$XDG_CONFIG_HOME` so behavior is indistinguishable from `_xdg/`. The mechanism is OS-agnostic; only the destination differs.

7. Adopt: Source Roots

    The pre-inference adopt CLI took the form `dodot adopt <pack> <files...>` and refused any source whose parent was not `$HOME` (the "flat-at-top-level" rule). The inference framework — landed for HOME and XDG roots, with the AppSupport root reserved here for Phases M1–M4 — generalises that into a per-source path-inference pass that picks the pack name, the in-pack path, and the round-trip prefix from the source's deployed location.

    7.1. CLI Shape

        New shape:

            dodot adopt <path>...                # pack name inferred per source
            dodot adopt <path>... --into <pack>  # all sources land in <pack>

        :: text ::

        `--into` is the override switch: it forces a single destination pack regardless of what inference would have picked. When the source's natural pack differs from the override, the in-pack path switches to the explicit-prefix encoding (`_xdg/<X>/<rest>`, `_app/<X>/<rest>`) so round-trip via Priority 2 still lands the deployed file at the original location. See §7.4.

    7.2. Recognized Source Roots

        Recognized adopt source roots, with the in-pack path each yields when the pack name is the *naturally inferred* one (i.e. `--into` agrees with inference, or `--into` is omitted). The override-aware path is in §7.4.

        Recognized adopt source roots:
            | Source path under                          | Inferred pack | In-pack path (natural) |
            | `$HOME/.<X>` (file)                        | (require `--into`) | `home.<X>`        |
            | `$HOME/.<X>/...` (dir)                     | (require `--into`) | `_home/<X>/...`   |
            | `$HOME/<X>` (force_home match)             | (require `--into`) | `<X>` (bare)      |
            | `$XDG_CONFIG_HOME/<X>/<rest>`              | `<X>`              | `<rest>`          |
            | `$XDG_CONFIG_HOME/<X>/` (the dir itself)   | `<X>`              | (expand children) |
            | `~/Library/Application Support/<X>/<rest>` | `<X>`              | `_app/<X>/<rest>` |
            | `~/Library/Application Support/<X>/`       | `<X>`              | (expand children) |
        :: table align=lll ::

        Two design notes are load-bearing:

            - **XDG sources use *bare* in-pack paths, not `_xdg/<X>/<rest>`.** The pack-name segment is *the* pack name; the resolver's default rule (Priority 6, `$XDG/<pack>/<rel>`) round-trips correctly without any prefix. This is a corrected reading versus an earlier draft of this section. The `_xdg/` prefix is the *override* encoding (§7.4), not the natural one.
            - **AppSupport in-pack paths *do* use the `_app/<X>/` prefix even at natural pack name.** That's because the resolver's default rule routes pack `<X>` to `$XDG/<X>/`, *not* `<app_support_dir>/<X>/`. Without the `_app/` prefix the round-trip would land the file in `~/.config/<X>/` on MacOs (wrong root), so the prefix is mandatory. Alternative: declare `[symlink.app_aliases] <X> = "<X>"` in the pack and use bare paths; adopt may emit this aliasing form when the capitalization heuristic (§8.1) says the folder is a GUI app and the user opts in. By default adopt produces `_app/<X>/<rest>` with no config writes.

    7.3. Inference Decline and `--into`

        `$HOME/.X` sources (and force_home matches) carry no pack structure inference can mine: a file like `~/.bashrc` could plausibly belong in a `shell`, `bash`, or `dotfiles` pack. Inference declines and surfaces an error pointing at `--into`.

        Multi-source invocations are required to agree on a single pack: if `~/.config/nvim/init.lua` and `~/.config/helix/config.toml` are passed together without `--into`, inference produces conflicting pack names (`nvim` and `helix`) and adopt refuses, naming both candidates so the user can split or specify `--into`.

        Mixing HOME-decline and XDG-inferred sources in a single invocation is allowed: they all land in the (single) inferred pack, with the HOME sources using their pack-name-independent `home.X` / `_home/X/` prefixes.

    7.4. Pack-Override Encoding

        When `--into <Y>` is supplied and `<Y>` ≠ the source's natural pack name, the in-pack path switches to the explicit-prefix encoding so the deployed path stays the same:

        Override-aware in-pack paths:
            | Source root                                 | In-pack path (override)       |
            | `$HOME/...`                                 | unchanged from §7.2 (already pack-name independent) |
            | `$XDG_CONFIG_HOME/<X>/<rest>`               | `_xdg/<X>/<rest>`             |
            | `~/Library/Application Support/<X>/<rest>`  | `_app/<X>/<rest>` (same as natural) |
        :: table align=ll ::

        Concrete: `dodot adopt ~/.config/lazygit/config.yml --into toolbox` lands the file at `toolbox/_xdg/lazygit/config.yml`. The `_xdg/` Priority-2 prefix bypasses pack-namespacing, so re-deploying still lands the symlink at `~/.config/lazygit/config.yml` even though the pack is `toolbox`.

    7.5. Pack-Root Directory Expansion

        When the source IS the pack-root directory under XDG or AppSupport (`~/.config/nvim/`, `~/Library/Application Support/Code/`), adopt enumerates the directory's children and creates one plan per top-level entry, instead of making the directory itself one big symlink-to-pack-root. Each child becomes a top-level pack member, so `dodot up` deploys per-entry like any other pack.

        Concrete: `dodot adopt ~/.config/helix/` produces, per child of `~/.config/helix/`:

            - `~/.config/helix/config.toml` → `helix/config.toml`
            - `~/.config/helix/themes/` → `helix/themes/`

        After adopt, each child of `~/.config/helix/` is its own symlink; `~/.config/helix/` itself stays a real directory.

        Expansion does not apply to `$HOME/.X/` directory sources — those keep the existing `_home/<X>/` whole-subtree adoption (§7.2 row 2). The reason is asymmetric: `~/.weechat/` *is* the file the user wants symlinked back, whereas `~/.config/helix/` is a container *whose contents* are what the user wants symlinked.

    7.6. Auto-Creating Packs

        When all sources point at a single inferred pack name and that pack does not exist on disk, adopt creates it (an empty directory; no `.dodot.toml` written). When `--into <pack>` is supplied and `<pack>` does not exist, adopt refuses — the explicit name is a typo guard the user opted into. The user can run `dodot init <pack>` first to bootstrap an explicit pack.

    7.7. AppSupport Source Implementation Status

        The `~/Library/Application Support/<X>/<rest>` row of §7.2 is *specified* but not yet *plumbed*: the inference framework reserves a `SourceRoot::AppSupport` variant, but the matcher does not consult it because `Pather` does not yet expose `app_support_dir()` (Phase M1). When M1 lands, two changes complete the AppSupport adopt path:

            - Add a third arm to the root-matching ladder in `commands::adopt::infer::infer_target` checking `app_support_dir`, between the XDG and HOME arms (longest-prefix order: AppSupport > XDG > HOME — though on Linux both AppSupport and XDG resolve to the same path and the AppSupport arm is unreachable in practice).
            - In the natural in-pack path, prepend `_app/<X>/` so Priority 2c routes back to `<app_support_dir>/<X>/<rest>`. The override-aware path is already `_app/<X>/<rest>` (§7.4), so override behavior is symmetric.

        Adopt's directory-expansion path (`SourceRoot::AppSupport`) is already wired through `expand_child_in_pack`: when a pack-root directory under AppSupport expands, each child gets `_app/<X>/<child>` as its in-pack path. This is the "pack `vscode` with `app_aliases.vscode = "Code"`" alternative's *non-aliased* implementation, kept simple by default and refinable via the capitalization heuristic (§8.1) once advisory probing lands.

    7.8. Sandboxed Apps

        MacOs sandboxed apps (App Store apps and many first-party apps) write to `~/Library/Containers/<bundle-id>/Data/Library/Application Support/<X>/` rather than the canonical location. These containers are not intended for external editing and their contents are often partially managed by the system.

        `dodot adopt` refuses sources under `~/Library/Containers/` with a clear message. The refusal is platform-agnostic: even on Linux a path matching that shape is rejected, so the same refusal text is the right thing to print on every OS (the path simply doesn't exist on Linux in practice).

        Refusal text:

            error: <path>
              this is a sandboxed app's container; its config is not
              intended to be edited externally. dodot does not support
              adopting from ~/Library/Containers/.

        :: text ::

        Users who genuinely need to manage container files can copy them into a pack manually and use `[symlink.targets]` with an absolute path.

8. Automagical Path Divination

    The deterministic resolver in §5 is intentionally narrow: it operates on textual prefixes and configured lists, nothing else. But MacOs exposes rich metadata about installed applications, and Homebrew — present on essentially every MacOs developer machine — maintains a community-curated database of where every cask-installed app stores its data. Used judiciously, these signals turn `dodot adopt` into a pleasant suggestion engine and `dodot up` into a verification layer that catches typos before they ship.

    The cardinal rule for everything in this session: *probes are advisory, never authoritative*. The resolver in §5 never consults them. If a probe returns wrong information, the user sees a wrong suggestion or a wrong warning, not a wrong deployment.

    8.1. The Capitalization Heuristic

        MacOs GUI app folders under `~/Library/Application Support/` follow a strong naming pattern that distinguishes them from Linux CLI-tool config folders under `~/.config/`. A folder name is a *probable* MacOs GUI app target if:

            - It contains at least one uppercase letter (`Code`, `Cursor`, `IntelliJ IDEA`, `Sublime Text`), or
            - It contains a space (`Application Support`, `Sublime Text 3`, `Smart Code ltd`), or
            - It matches a reverse-DNS pattern of two or more dotted lowercase segments (`com.apple.dt.Xcode`, `org.videolan.vlc`, `dev.warp.Warp-Stable`).

        A folder name is a *probable* CLI-tool target if it is all lowercase, contains no spaces, and does not match reverse-DNS. The CLI-tool population (`nvim`, `helix`, `ghostty`, `kitty`, `lazygit`, …) is uniformly lowercase-hyphenated; the population of capitalized `~/.config/Foo` directories for CLI tools is empty in practice (FS case-insensitivity gotchas keep package authors away from it).

        Used inside `dodot adopt`, this heuristic powers the default suggestion when a source path could be ambiguous. It is *not* used in the resolver, where determinism over textual prefixes is non-negotiable.

    8.2. Homebrew Cask Probing

        Homebrew cask packages declare a `zap` stanza enumerating every filesystem location an app touches: Application Support directories, Caches, Preferences PLists, LaunchAgents, et cetera. This is structurally the same data mackup curated by hand for years — except homebrew-cask is community-maintained at scale and self-updates as apps change.

        Available via subprocess:

            $ brew list --cask --versions      # installed casks (offline, fast)
            $ brew info --json=v2 --cask <token>   # full metadata incl. zap

        The JSON output's `artifacts[].zap` field gives, for any installed cask:

            - app-folder-name candidates: leaf names of `~/Library/Application Support/<X>` zap entries
            - bundle name: `artifacts[].app[0]` (e.g. `"Visual Studio Code.app"`)
            - associated Preferences plists, Caches, LaunchAgents

        dodot uses this data in two places:

            - At `adopt` time, to enrich the success message ("homebrew cask `visual-studio-code` confirms this is VS Code's app-support directory") and to surface sibling adoption candidates ("homebrew also reports `~/Library/Preferences/com.microsoft.VSCode.plist` for this cask — adopt that too?").
            - At `up`/`status` time, to soften "missing target directory" warnings into actionable hints ("looks like `<cask-token>` isn't installed yet — `_app/Code/...` will deploy but the app isn't here to read it").

        8.2.1. Why Not Auto-Derive `force_app`

            A reasonable-sounding alternative is to populate `force_app` automatically from homebrew-cask zap data. dodot deliberately does not do this. Auto-derivation reintroduces mackup's maintenance debt by proxy: a homebrew-cask change to a zap stanza would silently change a dodot deployment. The curated 100-entry list (§3.4) is owned by dodot. Brew probes inform user-facing suggestions, never resolver state.

    8.3. MacOs Native APIs

        Several MacOs-native sources of metadata complement homebrew probing:

            - `mdls` — Spotlight metadata for a single bundle. `mdls -name kMDItemCFBundleIdentifier "/Applications/<X>.app"` returns the bundle ID (e.g. `com.microsoft.VSCode`). Instantaneous.
            - `mdfind` — Spotlight query. `mdfind "kMDItemKind == 'Application' && kMDItemDisplayName == 'Cursor'"` resolves a display name to a bundle path. Useful when only a friendly name is known.
            - `defaults domains` — lists every preference domain registered in `~/Library/Preferences/`. The canonical API for plist-style preferences. Out of scope for `_app/` (preferences are a different file class) but available if `_lib/Preferences/` ever needs validation.
            - `lsregister -dump` — the full LaunchServices database. Comprehensive but heavy. Not used by dodot baseline.
            - `NSFileManager.URLsForDirectory(.applicationSupportDirectory, ...)` — the Cocoa API. Would require linking AppKit; not worth the dependency burden when env-var resolution is sufficient and trivially mockable.

        Of these, `mdls` is the most useful: cheap, scriptable, and resolves the "is this directory actually an installed app?" question authoritatively when given the .app bundle path.

    8.4. The `dodot probe app` Subcommand

        A new probe subcommand surfaces all available metadata for a pack, treating MacOs introspection as an explicit diagnostic step rather than a silent background process:

            $ dodot probe app <pack>

        Reports, for each `_app/<X>` entry and each `app_aliases[pack]` mapping in the pack:

            - whether `<app_support_dir>/<X>/` exists on the local filesystem
            - matching homebrew cask (token, install state)
            - matching `/Applications/<X>.app` bundle and its bundle ID (via `mdfind`/`mdls`)
            - suggested `app_aliases` entries when the pack-display-name diverges from the resolved app-folder name

        Sample output:

            pack: vscode
              alias: vscode → Code (~/Library/Application Support/Code/) [exists]
              cask:  visual-studio-code [installed]
              app:   /Applications/Visual Studio Code.app
              bundle: com.microsoft.VSCode
              suggested adoptions:
                ~/Library/Preferences/com.microsoft.VSCode.plist (from cask zap)

        :: text ::

        This is a debugging and discovery surface, not part of any deployment hot path. It is run on demand.

    8.5. What dodot Does Not Do

        Out of scope, deliberately:

            - Auto-curate `force_app` from homebrew-cask zap data (§8.2.1).
            - Run a long-running daemon that watches LaunchServices for app-install events.
            - Use `defaults write/read` to manage plist contents (that is a separate handler — see [./plists.lex]).
            - Mirror mackup's bundled app database. The combination of `_app/`, `app_aliases`, `force_app` (curated, capped), and homebrew probing covers the same ground without the curation cost.

9. Documentation Updates

    `docs/reference/symlink-paths.lex` gains a new §6 (renumbering existing §6/§7) documenting `_app/` and `_lib/`, and §3 (`force_home`) gains a sibling `force_app` subsection. The §1 introduction adds the third-coordinate table from §2.2 of this proposal.

    `docs/reference/terms-and-concepts.lex` adds entries for *app-support root*, *app alias*, and *force_app*.

    A new tutorial section (`dodot tutorial`) walks through adopting `~/Library/Application Support/Code/User/settings.json` end-to-end on MacOs, demonstrating both the `_app/` prefix and the `app_aliases` mechanism.

10. Implementation Phases

    This proposal is structurally additive: every change either introduces a new resolver rule, a new config key, or a new advisory probe. No existing behaviour changes. Phases are independently shippable.

    Phase M1: `Pather` and `_app/`
        - Add `app_support_dir()` to `Pather` and `XdgPatherBuilder`.
        - Add the `_app/` directory prefix to the resolver as Priority 2c.
        - Add `app_uses_library` config key with platform-aware defaults.
        - Update `symlink-paths.lex` with the new prefix.
        - Tests: prefix resolution on both platforms; builder override.

    Phase M2: `force_app` and `app_aliases`
        - Add `force_app` config key and resolver Priority 4.
        - Add `[symlink.app_aliases]` table and resolver Priority 5.
        - Add CI invariant capping `force_app` defaults at 100 entries.
        - Tests: alias-vs-prefix precedence; alias scoping; force_app first-segment matching.

    Phase M3: `_lib/`
        - Add the `_lib/` directory prefix as Priority 2d (MacOs) / warn-and-skip (other).
        - Tests: MacOs resolution; Linux warning emission.

    Phase M4: Adopt extensions
        - Extend `derive_pack_filename` to accept the source root.
        - Recognize `~/Library/Application Support/<X>/` as a valid adopt source.
        - Recognize `$XDG_CONFIG_HOME/<X>/` as a valid adopt source (deriving `_xdg/`).
        - Refuse `~/Library/Containers/` sources.
        - Tests: round-trip adopt → up → identical deployed path; container refusal.

    Phase M5: Capitalization heuristic in adopt
        - Use the heuristic to drive `adopt`'s default suggestion when the user's input is ambiguous.
        - Keep the resolver heuristic-free.

    Phase M6: Brew + MacOs probes
        - Implement `brew info --json=v2 --cask` lookup with on-disk caching.
        - Wire enrichment into `adopt` success messages and sibling-adoption suggestions.
        - Wire missing-target verification into `up`/`status`.
        - Add the `dodot probe app` subcommand.
        - Tests: mock subprocess output; cache invalidation.

    Phases M1–M4 are the deterministic core and ship together as a single user-facing feature. Phase M5 is small ergonomics. Phase M6 is the advisory layer and is independently shippable later.

11. Open Questions and Future Work

    11.1. `_app/` Naming

        Alternatives considered: `_gui/`, `_apps/`, `_appcfg/`. `_app/` is the shortest and matches the term "app config" most readers reach for first. Locked in unless review surfaces a strong objection.

    11.2. `app_uses_library = false` on MacOs

        Allowed and supported. A user who is "`~/.config`-everywhere even on MacOs" can flip this in their root `.dodot.toml`, and `_app/` plus `app_aliases` will route through `~/.config/` instead. `_lib/` is unaffected (it explicitly targets `~/Library/`, not `app_support_dir`).

    11.3. Plist Coverage

        `_lib/Preferences/<bundle-id>.plist` is syntactically valid under this proposal, but plist files are usually binary and not directly user-editable. The plist preprocessor [./plists.lex] is the right path for plist content management. `_lib/Preferences/` covers the routing; the preprocessor covers the format. The two compose without further work.

    11.4. Windows

        Out of scope for this proposal. Windows analogues (`%APPDATA%`, `%LOCALAPPDATA%`) would slot into the same architecture by extending `Pather` with additional accessors and adding a `_appdata/` prefix. Deferred until there is a real Windows user.

    11.5. Adopt-Time Plist Conversion

        When a user adopts `~/Library/Preferences/com.foo.plist` (a binary plist), should adopt automatically invoke the plist preprocessor's reverse path to produce the XML form? The `plists.lex` proposal answers yes. This proposal defers to that one and stays orthogonal.
