Design Specification: Preprocessing Pipeline

    This document specifies the generic preprocessing pipeline for dodot. The pipeline provides a unified architecture for files where the version-controlled source must be transformed before deployment. Template expansion and secret decryption are concrete implementations of this design. Plist support, originally drafted as a Representational preprocessor, ships instead as a pair of git clean/smudge filters — see [./plists.lex] §2.3 for the rationale. The pipeline is still illustrated with plists in places below for didactic reasons (they remain a useful canonical example of a Representational transform), but the actual plist implementation does not flow through this pipeline.

    The preprocessing pipeline is a new phase in dodot's execution model, not a handler. It runs before handler dispatch, producing expanded files that downstream handlers (symlink, shell, path, install, homebrew) consume transparently.

1. The Core Problem

    dodot's strength is its symlink-based model: the file in the dotfiles repo IS the deployed file, linked through the datastore. However, many use cases require a transformation step where the version-controlled source differs from the deployed artifact:

    - Templates: a `.tmpl` source is rendered with variables into a config file.
    - Plists: a human-readable XML representation is converted to a binary plist.
    - Secrets: an encrypted source is decrypted on deployment.

    These transformations share common infrastructure but differ in their safety models and reversal capabilities. The preprocessing pipeline provides the shared layer; individual preprocessors provide the specialization.

2. Transformation Types

    The pipeline recognizes two distinct types, each with a different safety model.

    2.1. Generative (One-Way) Transformations

        The source generates a destination artifact. The process cannot be reliably reversed because the transformation removes information.

        Example:
            Template rendering. `config.toml.tmpl` becomes `config.toml`. The rendered output does not contain enough information to reconstruct the template expressions.

        Source of Truth:
            The source file (e.g., `.tmpl`) is the canonical source. The rendered output is a derived artifact.

        Divergence Handling:
            Edits to the deployed file must be merged back into the source via heuristics and user review. This is best-effort by design.

    2.2. Representational (Two-Way) Transformations

        The source and destination are different formats representing the same data. The transformation is lossless and perfectly reversible.

        Example:
            Plist conversion. `plutil` converts between XML and binary without data loss.

        Source of Truth:
            The live destination file (e.g., the binary plist) is the source of truth, as applications modify it directly. The source file (XML) is a git-friendly representation.

        Divergence Handling:
            Edits to the deployed file can be automatically converted back to the source format. No heuristics needed.

    2.3. Opaque (Write-Only) Transformations

        The source is decrypted or decoded on deployment, but the deployed file must never be written back to the source format (for security or practical reasons).

        Example:
            GPG-encrypted files. The source `.gpg` file is decrypted on deployment. The plaintext must never be committed to git. Changes to the deployed file cannot be re-encrypted without key management that dodot should not own.

        Source of Truth:
            The encrypted source file. The deployed plaintext is ephemeral.

        Divergence Handling:
            No reverse path. Divergence detection reports changes but offers no merge. The user must manually update the source and re-encrypt.

3. Architecture: Preprocessing as a Pipeline Phase

    Preprocessing is NOT a handler. It is a separate phase that runs before handler dispatch and composes with all existing handlers.

    3.1. Pipeline Flow

        The current dodot execution pipeline is:

            scan(pack_dir) -> match(rules) -> group(handler) -> to_intents() -> execute()

        The new pipeline inserts a preprocessing phase:

            scan(pack_dir)
                -> partition(preprocessor_matches, regular_matches)
                -> preprocess(preprocessor_matches) -> expanded files in datastore
                -> create virtual RuleMatches for expanded files
                -> merge(virtual_matches, regular_matches) with collision detection
                -> match(rules) -> group(handler) -> to_intents() -> execute()

        After preprocessing, expanded files appear as virtual RuleMatches. The downstream handler pipeline sees them as regular files. Handlers do not know preprocessing occurred.

    3.2. Why Not a Handler

        Three reasons:

        Composition:
            `aliases.sh.tmpl` must be rendered AND sourced. `install.sh.tmpl` must be rendered AND executed. If preprocessing were a handler, it would replace the downstream handler. As a pipeline phase, it composes with any handler.

        Coupling:
            A preprocessing handler would need to internally re-match the stripped filename against the rule system to determine the downstream handler. This violates handler isolation and creates coupling that multiplies with each new preprocessor.

        Extensibility:
            Future user-provided preprocessor plugins should not require awareness of the handler system. A clean preprocessing trait makes this possible.

    3.3. Virtual Match Injection

        After a preprocessor expands a file, the result is a virtual RuleMatch:

            relative_path:    "config.toml"       (the stripped, logical filename)
            absolute_path:    <datastore_path>     (where the expanded file was written)
            pack:             "app"
            handler:          ""                   (determined by rule matching, not the preprocessor)
            template_source:  Some("config.toml.tmpl")  (pointer to the original source)

        This virtual match enters the normal rule-matching pipeline. The rule system determines which handler processes it (symlink, shell, path, etc.) based on the logical filename.

    3.4. Collision Detection

        At the merge step, virtual matches are checked against regular matches. If a pack contains both `config.toml` and `config.toml.tmpl`, the expanded form would clobber the regular file. This is detected and rejected with a clear error:

            error: collision in pack "app": config.toml.tmpl expands to config.toml,
                   which already exists as a regular file. Remove one or the other.

        This check is a core responsibility of the preprocessing layer, not of individual preprocessors.

4. The Preprocessor Trait

    All preprocessors implement a shared trait. Individual preprocessors provide the transformation logic; the pipeline provides the orchestration.

    4.1. Shared Interface

        trait Preprocessor:
            name()             -> &str               ("template", "gpg", ...)
            transform_type()   -> TransformType       (Generative, Representational, Opaque)
            matches_file()     -> bool                (extension check against config)
            expanded_filename() -> String             (strip .tmpl, .gpg, etc.)
            expand()           -> Result<Vec<u8>>     (the core transformation)
            contract()         -> Option<Vec<u8>>     (reverse, only for Representational)

    4.2. Shared Responsibilities (Pipeline Level)

        These are handled by the preprocessing pipeline, not by individual preprocessors:

        - Partitioning scanned files into preprocessor vs regular matches
        - Calling expand() and writing the result to the datastore
        - Creating virtual RuleMatches for expanded files
        - Collision detection against regular files
        - Caching baselines for divergence detection
        - Divergence checking and the reverse-merge git workflow

    4.3. Specialized Responsibilities (Preprocessor Level)

        Each preprocessor provides:

        - The transformation engine (MiniJinja for templates, gpg for secrets, etc.)
        - File extension matching rules
        - Filename stripping logic
        - For Representational: the reverse transformation (contract)

5. Storage

    5.1. Expanded Files: Datastore

        Expanded files are written as regular files (not symlinks) in the datastore:

            ~/.local/share/dodot/packs/{pack}/{handler}/{expanded_filename}

        The DataStore trait gains a new method:

            write_rendered_file(pack, handler, filename, content) -> PathBuf

        This writes a regular file at handler_data_dir(pack, handler)/filename. The user symlink then points to this regular file, just as it would point to a data-link symlink for non-preprocessed files.

    5.2. Baselines: Cache

        Baseline data for divergence detection lives in the XDG cache directory:

            ~/.cache/dodot/preprocessor/{pack}/{handler}/{filename}.json

        Contents:
            rendered_hash:   SHA-256 of the expanded content
            rendered_content: the full expanded content (for diffing)
            source_hash:     SHA-256 of the source file
            context_hash:    SHA-256 of the transformation context (variables, etc.)
            timestamp:       when the baseline was created

        The cache is separate from the datastore because their lifecycles differ. Clearing the cache does not affect deployments. Removing deployment state does not lose divergence tracking.

6. Divergence Detection and the Git Workflow

    6.1. Detection

        Divergence is detected by comparing the current deployed file against the cached baseline:

            current_hash = hash(read(deployed_file))
            if current_hash != baseline.rendered_hash:
                diverged

        Additionally, input change is detected by comparing source and context hashes:

            source_changed  = hash(source_file) != baseline.source_hash
            context_changed = hash(context) != baseline.context_hash

        This produces a 4-state matrix:

            Output same, Input same:     No action needed
            Output same, Input changed:  Re-expand (safe, no user edits to lose)
            Output changed, Input same:  Diverged (user edited the deployed file)
            Output changed, Input changed: Conflict (both sides changed)

    6.2. The Git Integration Flow

        The workflow is designed to inject into the user's existing git workflow rather than building a parallel one.

        `dodot transform check`:
            Scans all preprocessed files for divergence. For generative transforms with divergence, it applies reverse-merge heuristics directly to the source file in the working tree:

            - Static lines (no preprocessing syntax): changes applied assertively
            - Dynamic lines (contain preprocessing syntax): dodot conflict markers inserted

            The command then blocks (exit code 1) and instructs the user to review with git:

                dodot: syncing changes from deployed files back to sources:
                  app/config.toml.tmpl: 2 lines updated, 1 line needs review

                  Review:  git diff app/config.toml.tmpl
                  Keep:    git add app/config.toml.tmpl
                  Discard: git checkout -- app/config.toml.tmpl

            For representational transforms, the reverse conversion is applied automatically (lossless).

            For opaque transforms, divergence is reported but no source modification occurs.

        `dodot transform install-hook`:
            Installs a pre-commit hook that runs `dodot transform check`. On first preprocessed-file deployment, dodot prompts the user to install it.

    6.3. Dodot Conflict Markers

        For ambiguous changes that require user decision, dodot inserts its own conflict markers:

            >>>>>> dodot-conflict
            deployed: host = "production.db.internal"
            template: host = "{{ env.DB_HOST }}"
            <<<<<< dodot-conflict

        These markers are:
            - Format-agnostic (not a comment in any config language, works everywhere)
            - Machine-detectable (`dodot up` greps for them and refuses to expand)
            - Visible in `git diff` for user review
            - Resolvable with standard editor search/replace

        For files where `#` is a valid comment character, explanatory comments may precede the markers:

            # DODOT: line below was changed in deployed file but uses a template variable
            # DODOT: deployed value: host = "production.db.internal"
            >>>>>> dodot-conflict
            host = "{{ env.DB_HOST }}"
            <<<<<< dodot-conflict

        `dodot up` checks source files for unresolved markers before expanding. If found:

            error: unresolved conflict in app/config.toml.tmpl
                   resolve with `git diff` and remove dodot-conflict markers before deploying

    6.4. Re-deploy Behavior

        When `dodot up` encounters a previously-expanded file:

            Output same, Input same:       Skip (already correct)
            Output same, Input changed:    Re-expand (safe)
            Output changed, Input same:    Warn and skip (user edits preserved)
            Output changed, Input changed: Warn and skip (conflict — user edits preserved)

        `--force` overrides: always re-expands, discarding divergence.

        Staleness is defined from file content, not the runtime environment. The four-state matrix compares hashes of the source file and the deployed file against the cached baseline. Env vars referenced in templates (`{{ env.X }}`) are read live at render time and intentionally are not part of the staleness signal; rotating an env var does not invalidate the cache. Users who change a referenced env var pick up the new value with `dodot up --force`. The reasons for keeping env vars out of the invalidation signal are spelled out below.

        Implementation note: rows 3 and 4 collapse to the same outcome — `dodot up` never overwrites a deployed file whose bytes have diverged from the cached baseline. The clever 3-way merge (apply user's deployed-file edits back into the new render) lives in `dodot transform check` and the git clean filter, not in `up`. This keeps `up`'s contract crisp ("I will not destroy your work") at the cost of pushing the merge step into the commit cycle. Users resolve a row-3/row-4 skip via `dodot transform check` (auto-merge through the clean filter) or `dodot up --force` (overwrite).

        Why env vars are out of scope:

        - Layer boundary. Env vars are ambient runtime state, not configuration. Tracking them couples the template engine to the cache-invalidation contract — a leaky abstraction.

        - User expectations. No standard Unix tool (git, make, shells) treats env-var changes as cache-invalidating inputs. A `dodot up` that re-rendered because `$EDITOR` flipped would feel like a bug.

        - Ergonomics. AST-walking templates to enumerate env references is fragile across template-engine versions; hashing a projection of the entire process env into the sentinel is noisy (PWD, SHELL_PID, terminal env all change between invocations).

        Users who want a value to be stable and tracked should put it in `[preprocessor.template.vars]` (the `user_vars` namespace) rather than reach for `env.*`. The `env.*` namespace is the explicitly-marked "live read, you're on your own for invalidation" zone, with `dodot up --force` as the discoverable escape hatch.

7. Configuration

    7.1. Preprocessor Section

        Preprocessor configuration lives in its own section of `.dodot.toml`, following the same 3-layer hierarchy (defaults < root < pack):

            [preprocessor]
            enabled = true                   # global kill switch (default: true)

            [preprocessor.template]
            extensions = ["tmpl", "template"] # which extensions trigger this preprocessor
            # ... preprocessor-specific config (variables, engine options, etc.)

            # Note: plists do not appear here — they ship as git clean/smudge
            # filters, not as a preprocessor. See [./plists.lex] §2.3.

    7.2. Opt-Out Levels

        Global:
            [preprocessor]
            enabled = false

        Per-preprocessor:
            [preprocessor.template]
            enabled = false

        Per-pack:
            Pack-level .dodot.toml can override any of the above.

        Per-file:
            [mappings]
            skip = ["config.toml.tmpl"]

        The existing skip mechanism in [mappings] works because preprocessing files go through the scan phase where skip patterns are honored.

8. Testability

    Testability of the preprocessing pipeline is a first-class concern. The existing codebase has strong patterns for this: the Fs trait for filesystem abstraction, TempEnvironment for integration tests, and mock CommandRunners.

    8.1. Mock Preprocessor

        A test-only MockPreprocessor that implements the Preprocessor trait with configurable behavior:

            MockPreprocessor::new("template")
                .extension("tmpl")
                .expand_fn(|source| Ok(source.replace("{{NAME}}", "Alice")))

        This allows testing the preprocessing pipeline (partitioning, collision detection, virtual match injection) without depending on MiniJinja, plutil, or gpg.

    8.2. Virtual Match Helpers

        Test utilities for creating virtual RuleMatches directly, bypassing the preprocessing phase:

            VirtualMatch::new("app", "config.toml")
                .from_template("config.toml.tmpl")
                .with_content("rendered content")

        This allows testing downstream handlers with preprocessed files without running the full pipeline.

    8.3. TempEnvironment Extensions

        The existing TempEnvironment builder gains methods for preprocessed file scenarios:

            TempEnvironment::builder()
                .pack("app")
                    .file("config.toml.tmpl", "host = {{env.DB_HOST}}")
                    .done()
                .build()

        And assertion helpers:

            env.assert_rendered_file("app", "symlink", "config.toml", "host = localhost")
            env.assert_baseline_cached("app", "config.toml")
            env.assert_no_collision("app", "config.toml")

9. Concrete Use Cases

    The following are brief sketches of preprocessors this pipeline is designed to support. Each would have its own detailed proposal.

    9.1. Template Expansion (Generative)

        Source: `config.toml.tmpl` with Jinja2-style expressions
        Expanded: `config.toml` with variables resolved
        Reverse: Heuristic line-matching, conflict markers for ambiguous lines
        Engine: MiniJinja (Rust)

        This is the most universal use case and the first to be implemented. See companion proposal: Template Expansion.

    9.2. Plist Conversion — Representational, but Not Through This Pipeline

        Plists are the canonical Representational transform: `plutil` (and the `plist` Rust crate) round-trip XML and binary losslessly, so reverse is exact and no conflict markers are ever needed.

        Despite that fit on paper, plist support does *not* ship as a preprocessor in this pipeline. It ships as git clean/smudge filters instead. The reasoning is in [./plists.lex] §2.3; in summary: plists drift continuously (apps rewrite them on settings changes), and the pipeline's pre-commit-hook reverse path leaves drift invisible to `git status` between commits. Clean/smudge closes that gap by attaching the reverse to every git read, not just to commits.

        This pipeline retains plists as a didactic example of the Representational category, and the `Preprocessor::contract()` hook stays in the trait so that future Representational preprocessors with less continuous drift (one-off binary configs that don't rewrite themselves) can still go through it.

    9.3. Encrypted Files (Opaque)

        Source: `credentials.conf.gpg` (GPG-encrypted)
        Expanded: `credentials.conf` (plaintext, deployed to user location)
        Reverse: None. dodot does not handle encryption. Divergence is reported but not actionable.
        Engine: system `gpg` command

        Key property: the expanded plaintext must never be written back to git. Divergence detection warns the user, but there is no merge path. The user must manually update the source and re-encrypt outside dodot.

        This is the simplest preprocessor: expand-only, no reverse, no merge. It demonstrates that the pipeline supports the full spectrum from "fully automatic" (plists) to "hands off" (GPG).

10. Implementation Strategy

    Phase 1: Core Pipeline
        - Preprocessor trait definition
        - Pipeline partitioning (preprocessor matches vs regular matches)
        - Virtual match injection into the handler pipeline
        - Collision detection
        - DataStore.write_rendered_file()
        - Baseline caching infrastructure
        - Mock preprocessor and test helpers

    Phase 2: Divergence Detection
        - Baseline comparison logic (4-state matrix)
        - `dodot transform check` command
        - Re-deploy behavior (warn-and-skip, force override)

    Phase 3: Git Integration
        - Dodot conflict markers (format, detection, safety gate)
        - `dodot transform install-hook` command
        - Pre-commit hook script generation
        - First-deployment onboarding hint

    Phase 4: Reverse Merge Framework
        - Static vs dynamic line classification (generic, preprocessor-agnostic)
        - Assertive change application
        - Conflict marker insertion
        - Integration with `dodot transform check`
