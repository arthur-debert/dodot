Configuration System

    This document covers the internals of configuration loading and resolution. For the user-facing TOML schema, see [./../user/configuration.lex].

    :: note :: See [./../reference/terms-and-concepts.lex] for terminology used throughout.

1. Components

    Configuration lives in `dodot_lib::config`. Three pieces:

    - `DodotConfig`: the struct every config key lives on, with compiled-in defaults declared via `#[derive(confique::Config)]` and `#[config(default = ...)]`.
    - `ConfigManager`: wraps a clapfig `Resolver`; handles discovery and per-pack merging.
    - `.dodot.toml` files: on-disk overrides, at the dotfiles root and optionally in each pack.

    clapfig and confique do the heavy lifting. clapfig provides the layered Resolver; confique provides derive-based default declaration and TOML deserialization.

2. The Three Layers

    Every config key is resolved through three layers, with later layers overriding earlier ones:

    - _Compiled defaults_ — the `#[config(default = ...)]` attributes on `DodotConfig` fields. Always present.
    - _Root config_ — `$DOTFILES_ROOT/.dodot.toml`. Optional.
    - _Pack config_ — `$DOTFILES_ROOT/<pack>/.dodot.toml`. Optional.

    Pack configuration is only applied when resolving for a specific pack. The root-level resolution (what `dodot config` shows when called outside a pack context) is defaults + root.

3. Merge Rules

    clapfig's Resolver merges TOML values according to type:

    - _Scalars_ (string, int, bool): override. The later value replaces the earlier one.
    - _Arrays_: override. No concatenation, no deduplication — the later array replaces the earlier one wholesale. (This is by design: an override should be predictable without knowing the upstream.)
    - _Maps / tables_: deep merge, recursively. Nested keys combine across layers; any scalars or arrays within are handled per the rules above.

    This means you can add a single variable to `[preprocessor.template.vars]` in a pack without having to restate every root-level variable, because the tables deep-merge. But if you want to change `[symlink] force_home` for one pack, you have to restate the full list — arrays override.

4. Discovery

    `ConfigManager::new(dotfiles_root)` constructs a Resolver configured to:

    - Search for `.dodot.toml` files via ancestor walk, stopping at the dotfiles root.
    - Stop walking at the `.git` directory boundary — this prevents a stray `.dodot.toml` in a parent directory above the dotfiles repo from leaking into resolution.
    - Preserve discovery order so merges apply in the right sequence.

    Two primary APIs:

    - `config_manager.root_config()` — returns the merged config for the root (defaults + root `.dodot.toml`).
    - `config_manager.config_for_pack(pack_path)` — returns the fully merged config for a specific pack (defaults + root + pack).

    Commands fetch a root config at startup and ask for a pack-specific config inside the per-pack loop.

5. Generation

    `dodot config gen` produces a starter `.dodot.toml` with every key commented out at its default value. The output is generated from the `DodotConfig` struct definition and its doc comments — the struct is the source of truth for both defaults and documentation.

    Implementation is in `commands::config`, which relays to clapfig's built-in generator.

6. Inspection

    `dodot config` with no subcommand prints the fully resolved configuration. This is the recommended way to debug "is dodot seeing my override?" — it shows exactly what the tool has after merging all layers.

7. Why Not Roll Our Own

    A few reasons we lean on clapfig + confique rather than writing config loading directly:

    - Derive-based defaults keep the struct the single source of truth. Add a field, write its default inline, done.
    - Ancestor-walk + git-boundary stop behavior is fiddly; having it in a library means it stays correct.
    - The generator (`dodot config gen`) comes for free.

    The trade-off is that clapfig is a somewhat opinionated library. When it's wrong for us, the fix belongs upstream rather than in dodot.

8. Pack-Level Override Gotchas

    Two non-obvious behaviors to keep in mind:

    - Array override is absolute. If you want to add to a default list at the pack level, you must restate the full default plus your additions. This is a clapfig design choice, not a bug.
    - `.dodot.toml` files above the dotfiles root are ignored (the git-boundary stop). If you test with a shallow directory layout, make sure your tests have a `.git` directory at the dotfiles root — or use `TempEnvironment`, which sets this up correctly.
