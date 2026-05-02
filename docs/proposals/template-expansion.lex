Design Specification: Template Expansion

    :: note ::
        **Status: implemented and shipped.** The template preprocessor (Phase T1), reverse-merge framework (Phase T2 via burgertocow + diffy), and configuration / status surface (Phase T3) all landed across PRs #100–#107. The user-facing reference is at [./../reference/pre-processors.lex] §6 and [./../reference/template-magic.lex]. This proposal is preserved as historical design context — *not* a maintained spec. Where this document and the reference docs disagree, the reference docs are authoritative.

    This document specifies the template expansion preprocessor for dodot. It is a concrete implementation of a Generative (One-Way) transformation as defined in the Preprocessing Pipeline design [./preprocessing-pipeline.lex].

    Template expansion is the first preprocessor to be implemented. It renders Jinja2-style template files into final configuration files during deployment, enabling environment-specific settings while keeping the template source under version control.

1. Overview

    1.1. The Use Case

        A user has a config file that needs machine-specific values:

            ~/dotfiles/app/config.toml.tmpl:
                [database]
                host = "{{ env.DB_HOST }}"
                port = 5432
                data_dir = "{{ dodot.home }}/data"

        On `dodot up`, this is rendered and deployed:

            ~/.config/app/config.toml:
                [database]
                host = "localhost"
                port = 5432
                data_dir = "/home/alice/data"

        The template source stays in git. The rendered file lives in the datastore. The user location symlinks to the datastore file.

    1.2. Composition with Handlers

        Template expansion composes with all existing handlers. The preprocessor renders the file; the downstream handler deploys it:

            config.toml.tmpl    -> symlink handler  -> symlinked to user location
            aliases.sh.tmpl     -> shell handler    -> sourced via init script
            install.sh.tmpl     -> install handler  -> executed with sentinel tracking
            Brewfile.tmpl       -> homebrew handler  -> executed with sentinel tracking
            bin/mytool.tmpl     -> path handler     -> exposed via $PATH, auto-chmod

        The downstream handler is determined by rule-matching the stripped filename (e.g., `aliases.sh`) against the normal mapping rules. The template preprocessor does not need to know which handler will process the result.

2. Template Engine

    2.1. MiniJinja

        The rendering engine is MiniJinja, a Rust-native Jinja2 implementation by the creator of Jinja2 (Armin Ronacher).

        Syntax:
            Variable substitution: {{ variable }}
            Conditionals:          {% if condition %} ... {% endif %}
            Loops:                 {% for item in list %} ... {% endfor %}
            Comments:              {# comment #}
            Filters:               {{ value | upper }}

        MiniJinja is chosen for:
            - Jinja2 syntax is widely known (Ansible, Cookiecutter, Hugo, etc.)
            - Lightweight with fast compile times (dodot is a CLI tool)
            - First-class Rust support with custom functions and filters
            - Supports template introspection (needed for reverse merge heuristics)

    2.2. Rust Dependency

        minijinja = "2"

3. Variable System

    Three namespaces, strictly separated. No collisions possible.

    3.1. Built-in Variables: `dodot.*`

        Computed from the runtime environment. Always available, read-only.

            {{ dodot.os }}              "macos", "linux", "windows"
            {{ dodot.arch }}            "aarch64", "x86_64"
            {{ dodot.hostname }}        gethostname() result
            {{ dodot.username }}        current user
            {{ dodot.home }}            $HOME
            {{ dodot.dotfiles_root }}   dotfiles repo path

        These map directly to values already available via the Pather trait and standard library calls. Computed once per `dodot up` invocation and shared across all template renders.

    3.2. Environment Variables: `env.*`

        Direct passthrough to the process environment. Any environment variable is accessible:

            {{ env.SHELL }}
            {{ env.EDITOR }}
            {{ env.DB_HOST }}
            {{ env.MY_CUSTOM_VAR }}

        Implemented via MiniJinja's dynamic lookup function (calls std::env::var at render time). No enumeration needed.

        An undefined environment variable produces a clear error:

            error: undefined variable `env.DB_HOST` in app/config.toml.tmpl
                   set the DB_HOST environment variable or use a default:
                   {{ env.DB_HOST | default("localhost") }}

        Cache invalidation: env-var references are read live at render time and are intentionally not part of the cache-invalidation signal — see `preprocessing-pipeline.lex` §6.4. Rotating an env var that a template references will not by itself trigger a re-deploy; users pick up the new value with `dodot up --force`. Stable values that should participate in invalidation belong in `[preprocessor.template.vars]` (the `user_vars` namespace, §3.3), not `env.*`.

    3.3. User-Defined Variables: bare names

        Defined in `.dodot.toml` under `[preprocessor.template.vars]`:

            [preprocessor.template.vars]
            name = "Alice"
            email = "alice@example.com"
            theme = "dark"

        Used in templates without a namespace prefix:

            git_name = "{{ name }}"
            git_email = "{{ email }}"

        These follow the 3-layer config hierarchy (compiled defaults < root .dodot.toml < pack .dodot.toml), so per-pack variable overrides work naturally.

    3.4. Collision Prevention

        If a user-defined variable name matches a reserved namespace (`dodot` or `env`), config loading produces an error:

            error: variable name "dodot" in [preprocessor.template.vars] is reserved
                   use a different name

4. Rendering Flow

    4.1. During `dodot up`

        For each template file matched by the preprocessor:

        1. Read the template source from the pack directory
        2. Build the variable context:
            a. dodot.* from Pather + system calls (computed once, reused)
            b. env.* via dynamic lookup
            c. bare names from merged config
        3. Render via MiniJinja
        4. Check for dodot-conflict markers in the source (refuse if found)
        5. Write the rendered content to the datastore as a regular file
        6. Store the baseline in the cache (rendered hash, source hash, context hash)
        7. The virtual RuleMatch (with datastore path) enters the handler pipeline
        8. The downstream handler creates the user symlink (or stages, or executes)

    4.2. Sentinel Hashing for Executable Templates

        For templates that route to the install or homebrew handler (e.g., `install.sh.tmpl`), the sentinel must track both the template content hash AND the variable context hash. If either changes, the script re-runs. This extends the existing sentinel format:

            sentinel = "{filename}-{hash(template_content + context_hash)}"

    4.3. Error Handling

        Template errors produce actionable messages:

            Undefined variable:
                error: undefined variable `database_host` in app/config.toml.tmpl
                       add it to [preprocessor.template.vars] in .dodot.toml
                       or set it as an environment variable: {{ env.DATABASE_HOST }}

            Syntax error:
                error: template syntax error in app/config.toml.tmpl line 5:
                       unexpected end of block, expected `endif`

            Render failure:
                error: template render failed for app/config.toml.tmpl:
                       filter `upper` received an incompatible type

5. Reverse Merge Heuristics

    When a deployed file has been modified (divergence detected), template expansion applies heuristics to map changes back to the template source. This is best-effort by design: an exact reverse is mathematically impossible for generative transforms.

    5.1. The Algorithm

        Given three versions:
            Template:  the .tmpl source file (under version control)
            Baseline:  the rendered output cached at last deployment
            Current:   the deployed file as it exists now (possibly modified)

        Steps:
            1. Diff baseline vs current to identify the user's changes
            2. For each changed line, find the corresponding template line
            3. Classify the template line as static or dynamic:
                Static:  no {{ }}, {% %}, or {# #} syntax
                Dynamic: contains template expressions
            4. For static lines:  apply the change directly to the template source
            5. For dynamic lines: insert dodot-conflict markers

    5.2. Line Correspondence

        For simple templates (variable substitution only), there is a 1:1 line correspondence between template source and rendered output. Line N of the template produces line N of the output.

        For templates with conditionals ({% if %}):
            Lines inside a false branch do not appear in the output. They are preserved in the template unchanged. The line mapping shifts, but the algorithm can track this by walking both files in parallel.

        For templates with loops ({% for %}):
            One template line produces N output lines. If the user changed one iteration's output, the mapping is ambiguous. The entire loop body is flagged as a conflict.

        For complex templates where line tracking fails:
            Fall back to section-level conflict markers encompassing the ambiguous region.

    5.3. Applied Example

        Template source (config.toml.tmpl):
            [database]
            host = "{{ env.DB_HOST }}"
            port = 5432
            max_connections = 10

        Baseline (rendered at deploy time):
            [database]
            host = "localhost"
            port = 5432
            max_connections = 10

        Current (user/app modified):
            [database]
            host = "production.db.internal"
            port = 5432
            max_connections = 50

        After `dodot transform check` modifies the template:
            [database]
            # DODOT: deployed value was: host = "production.db.internal"
            >>>>>> dodot-conflict
            host = "{{ env.DB_HOST }}"
            <<<<<< dodot-conflict
            port = 5432
            max_connections = 50

        Line "host": dynamic (contains {{ }}), so conflict markers are inserted.
        Line "max_connections": static, changed from 10 to 50, applied directly.
        Line "port": unchanged, left alone.

        The user runs `git diff` and sees both the assertive change (max_connections) and the conflict (host). They resolve the conflict by choosing one side, then `git add` and `git commit`.

    5.4. Scope and Limitations

        This heuristic targets the common case: config files with mostly static content and a few template variables. It will handle the majority of real-world dotfile templates well.

        Known limitations:
            - Templates that are mostly dynamic (more {{ }} than static text) will produce more conflict markers than assertive changes. The heuristic degrades gracefully.
            - Complex Jinja2 features (macros, includes, inheritance) are not tracked for line correspondence. Changes near these features will be flagged as conflicts.
            - Multiline template expressions are treated as a single dynamic block.

        Users can opt out of the reverse merge for specific files:
            [preprocessor.template]
            no_reverse = ["complex-config.toml.tmpl"]

        These files still get divergence detection (warning that the file changed) but no source modification is attempted.

6. Configuration

    6.1. Full Configuration Schema

        [preprocessor.template]
        enabled = true                        # enable/disable template preprocessing
        extensions = ["tmpl", "template"]     # file extensions that trigger rendering
        no_reverse = []                       # files to skip reverse merge heuristics

        [preprocessor.template.vars]
        # user-defined variables, available as bare names in templates
        name = "Alice"
        email = "alice@example.com"

    6.2. Configuration Inheritance

        The 3-layer config hierarchy applies:

        Root .dodot.toml:
            [preprocessor.template.vars]
            name = "Alice"

        Pack .dodot.toml (e.g., git/.dodot.toml):
            [preprocessor.template.vars]
            name = "Alice (Work)"     # overrides root for this pack only

7. Commands

    7.1. `dodot transform check`

        Non-interactive divergence detection and reverse-merge application.

        Behavior:
            1. Scan all preprocessed files for divergence
            2. For generative transforms: apply reverse merge heuristics to source files
            3. For representational transforms: apply reverse conversion automatically
            4. For opaque transforms: report divergence only
            5. Exit 0 if no divergence, exit 1 if divergence was found/applied

        Usage:
            dodot transform check           # check all preprocessed files
            dodot transform check app       # check a specific pack

    7.2. `dodot transform install-hook`

        Installs a pre-commit hook that runs `dodot transform check`.

        Behavior:
            - No existing hook: creates one
            - Existing hook: appends dodot check (with guard comment, idempotent)
            - User declines: remembers choice, does not nag again

    7.3. `dodot transform status`

        Shows the current state of all preprocessed files:

            $ dodot transform status
            app/config.toml.tmpl   -> config.toml     deployed, clean
            zsh/zshrc.tmpl         -> .zshrc           deployed, modified (3 lines)
            vim/vimrc.tmpl         -> .vimrc           not deployed

8. Implementation Phases

    Template expansion depends on the preprocessing pipeline (Phase 1 of that proposal). Once the pipeline is in place:

    Phase T1: Core Rendering
        - MiniJinja integration
        - TemplatePreprocessor implementing the Preprocessor trait
        - Variable context building (dodot.*, env.*, user vars)
        - Basic render + deploy via the pipeline
        - Unit tests for rendering, variable resolution, error messages

    Phase T2: Reverse Merge
        - Line correspondence tracking
        - Static vs dynamic classification
        - Assertive change application
        - Dodot conflict marker insertion
        - Integration with `dodot transform check`

    Phase T3: Configuration and Polish
        - [preprocessor.template] config section
        - no_reverse per-file opt-out
        - Onboarding hint on first template deployment
        - dodot transform status for template files
