CLI Output

    This guide explains dodot's unified output system and how it provides consistent, predictable output across all commands.

1. Core Concept: Pack Status Representation

    The fundamental unit of dodot's output is the pack status representation. This shows:

    - Pack name and overall status
    - All files managed by the pack
    - Each file's handler type and current state
    - Special files (.dodot.toml, .dodotignore)

    Every command that interacts with packs displays this same representation, ensuring users always see the current state of their dotfiles.

2. Unified Output Format

    All pack-related commands follow this format.

    Format:

        <Optional Message>

        <Pack Status Representation>

    :: text ::

    The message describes what action was taken, while the pack status shows the resulting state.

3. Example Output

    After running `dodot up vim`.

    Terminal output:

        The pack vim has been deployed.

        vim:
          -> .vimrc -> ~/.vimrc [deployed]
          -> .vim/colors/theme.vim -> ~/.vim/colors/theme.vim [deployed]

    :: text ::

4. Output Formats

    dodot supports multiple output formats via the `--output` flag:

    term (default):
        Rich output with colors and styling via standout-render.

    text:
        Plain text without styling, used when piping output or when NO_COLOR is set.

    json:
        Machine-readable JSON for programmatic access.

    term-debug:
        Terminal output with debug annotations showing template and theme resolution.

    yaml:
        Machine-readable YAML for programmatic access.

    4.1. Format Selection

        The `--output` flag is added automatically by standout's `App` builder. Format selection is handled by standout's automatic detection:

        - Explicit `--output` flag overrides everything
        - NO_COLOR environment variable forces text mode
        - Terminal capability detection falls back to text if no color support
        - Pipe detection switches to text when output is piped

5. Architecture

    The output system consists of:

    `dodot_lib::commands`:
        Result types are defined per command (e.g. `PackStatusResult`).

    `dodot_lib::render`:
        Theme and template definitions.

    Output modes (term, text, json, term-debug) are handled by standout-render's `OutputMode` enum. The CLI uses standout's `App` builder with embedded MiniJinja templates and CSS stylesheets.

6. Command Integration

    Commands create a CommandResult with:

    - An optional message describing the action
    - The current pack status from the status command

    Commands return serializable result types (e.g. `PackStatusResult`). The CLI handler wraps the result in `Output::Render(result)`, and standout renders it via the registered MiniJinja template with the configured theme. This keeps command logic free of formatting concerns: commands produce data, and the render layer decides how to display it.

7. Message Formatting

    The `FormatCommandMessage` helper provides consistent messaging:

    - Single pack: "The pack vim has been deployed."
    - Multiple packs: "The packs vim and git have been deployed."
    - Empty result: no message shown

8. Commands and Their Messages

    8.1. Pack-Altering Commands

        - `up`: "The pack(s) X have been deployed."
        - `down`: "The pack(s) X have been removed."

    8.2. Pack Creation/Modification

        - `init`: "The pack X has been initialized with N files."
        - `fill`: "The pack X has been filled with N placeholder files."
        - `adopt`: "N files have been adopted into the pack X."
        - `addignore`: "A .dodotignore file has been added to the pack X."

    8.3. Information Commands

        - `list`: no message, shows all pack statuses
        - `status`: no message, shows requested pack statuses

9. Implementation Guidelines

    When adding new commands:

    - Always show pack status for pack-related operations
    - Use CommandResult to combine message and status
    - Run StatusPacks to get current state after operations
    - Use FormatCommandMessage for consistent messaging
    - Let the renderer handle format-specific output

10. Benefits

    This unified approach provides:

    - *Consistency*: same pack representation everywhere
    - *Predictability*: users know what to expect
    - *Flexibility*: multiple output formats for different use cases
    - *Simplicity*: commands focus on logic, not formatting
    - *Testability*: output rendering is isolated and testable
