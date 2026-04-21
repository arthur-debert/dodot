CLI Output

    This document covers how dodot renders its CLI output: the standout-based pipeline, the MiniJinja templates that format each command's result, and the integration with command result types. For the user-facing `--output` flag reference, see [./../user/commands.lex] §4.

    :: note :: See [./../reference/terms-and-concepts.lex] for terminology used throughout.

1. The Output System

    dodot's CLI output is driven by [standout](https://crates.io/crates/standout), an embedded rendering library that unifies terminal, text, JSON, and YAML output behind one template pipeline. Commands produce serializable result types; standout renders them via MiniJinja templates with CSS themes for terminal styling.

    The split is clean: commands contain no formatting logic, templates contain no business logic.

2. Wiring

    `dodot-cli/src/main.rs::build_app()` constructs a standout `App`:

    - Registers each command handler under a template name (`pack-status`, `list`, `message`)
    - Embeds MiniJinja templates from `dodot-cli/src/templates/` at compile time
    - Embeds CSS stylesheets from `dodot-cli/src/styles/` at compile time
    - Sets `dodot` as the default theme
    - Declares command groups for `--help` rendering

    The resulting `App` is parsed with a clap command tree. standout handles `--output` flag detection, output-mode selection, and final rendering. The command handler just returns data.

3. Result Types

    Every pack-based command returns a serializable result type from `dodot-lib`. These live alongside the commands they describe.

    - `up`, `down`, `status`, `adopt` → `PackStatusResult` (registered under the `pack-status` template)
    - `list` → `ListResult` (registered under the `list` template)
    - `init`, `fill`, `addignore` → distinct command-specific types (`InitResult`, `FillResult`, `AddIgnoreResult`), all registered under the `message` template

    Result types implement `serde::Serialize`. JSON and YAML output modes serialize them directly; terminal and text modes run them through the registered template.

4. Templates

    Under `dodot-cli/src/templates/`, one template per output shape:

    - `pack-status.jinja` — renders pack deployment state with per-file icons and status
    - `list.jinja` — renders the pack list
    - `message.jinja` — renders a plain message (used by init/fill/addignore)

    Templates are MiniJinja syntax. They receive the command's result type as their context and produce styled text. Styling uses standout's BBCode-style tags (`[bold]`, `[color=green]`); the CSS layer maps tags to terminal codes for term mode and strips them for text mode.

5. Output Modes

    standout's `OutputMode` enum governs rendering:

    - `term` — rich terminal output with colors and styling (default)
    - `text` — plain text with styling stripped (used when piping, when `NO_COLOR` is set, or when the terminal doesn't support color)
    - `json` — serde-serialized JSON
    - `yaml` — serde-serialized YAML
    - `term-debug` — term output with template/theme resolution annotations (for debugging template changes)

    Selection is automatic: `--output` flag overrides everything, then `NO_COLOR`, then terminal capability detection, then pipe detection. Commands never select a mode; they just produce data.

6. Theme

    `dodot-cli/src/styles/dodot.css` defines the default theme's color and style mappings. Theme switching is a standout feature; dodot ships one theme but the infrastructure supports more.

7. Integration Pattern

    A command handler in `dodot-cli/src/handlers.rs` looks like:

    Handler pattern:

        pub fn up_handler(matches: &ArgMatches) -> Result<Output> {
            let ctx = build_context()?;
            let packs = pack_names_from(matches);
            let result: PackStatusResult = commands::up::up(&ctx, &packs)?;
            Ok(Output::Render(result))
        }

    :: rust ::

    The handler does three things: build the execution context, call into `dodot-lib`, wrap the result in `Output::Render` so standout knows to template it. No formatting, no colors, no mode detection.

8. Passthrough Commands

    Two commands bypass standout entirely:

    - `init-sh` — emits raw shell script to stdout for `eval`. Any templating would corrupt the output.
    - `config` — uses clapfig's own output formatting, since the config struct is the authoritative source of its schema.

    These are handled in `main.rs` before the standout dispatch loop.

9. Testing Output

    Integration tests assert against `json` output rather than `term` output. JSON is structured, stable across theme changes, and easy to parse with `serde_json::from_str` or `jq` in shell tests. Snapshot tests on terminal output are possible but brittle — we avoid them where a JSON assertion would do.

10. Adding a New Template

    Adding output for a new command:

    - Define a `FooResult` type in `dodot-lib::commands::foo` with `Serialize`.
    - Write `dodot-cli/src/templates/foo.jinja` using MiniJinja syntax and standout BBCode tags.
    - Register in `build_app()`: `.command("foo", handlers::foo_handler, "foo")`.
    - In the handler, return `Output::Render(result)`.

    That's the full loop. JSON and YAML output modes work automatically from the `Serialize` derive; terminal and text modes use the template.
