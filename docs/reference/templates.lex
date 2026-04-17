Template Expansion

    Some dotfiles need to vary between machines: a different git user per host, an OS-specific Homebrew prefix, a laptop-vs-workstation shell prompt. Rather than forking configs or hand-editing after every clone, dodot renders Jinja2-style templates at deploy time. One versioned source file can produce different outputs on different hosts.

    Any pack file whose name ends in `.tmpl` or `.template` is a template. dodot strips that extension, renders the content, and hands the result to the normal handler pipeline. `git/gitconfig.tmpl` is rendered and then symlinked as `~/.gitconfig`, exactly as if `gitconfig` had been there all along.

    Rendering is transparent: there is no `dodot render` step, no staging area, no "please remember to regenerate". Every `dodot up` re-renders, so editing the template or changing a variable picks up on the next deploy.

1. A First Example

    A git config you want on every machine, with a different user per host:

    Rendering a variable:

        $ cat ~/dotfiles/git/gitconfig.tmpl
        [user]
            name = {{ name }}
            email = {{ name | lower }}@example.com

        $ cat ~/dotfiles/git/.dodot.toml
        [preprocessor.template.vars]
        name = "Alice"

        $ dodot up git
        ... symlink:  git/gitconfig -> ~/.gitconfig: deployed

        $ cat ~/.gitconfig
        [user]
            name = Alice
            email = alice@example.com

    :: shell ::

    The `.tmpl` extension is the only trigger — no flags, no opt-in. `dodot status` shows the file under its stripped name (`gitconfig`, not `gitconfig.tmpl`), because that's what actually gets deployed.

2. What You Can Reference in a Template

    Three namespaces are always available inside a template:

    - `dodot.*` — built-in values describing the machine and your dotfiles setup.
    - `env.*` — lookup of the current process's environment variables.
    - Bare names — variables you define under `[preprocessor.template.vars]`.

    Dodot built-ins:

        {{ dodot.os }}              host os    (e.g. "linux", "macos")
        {{ dodot.arch }}            cpu arch   (e.g. "aarch64", "x86_64")
        {{ dodot.hostname }}        machine hostname
        {{ dodot.username }}        current user
        {{ dodot.home }}            absolute $HOME
        {{ dodot.dotfiles_root }}   absolute dotfiles root

    :: jinja ::

    `os`, `arch`, `home`, and `dotfiles_root` are always populated. `hostname` and `username` are best-effort: if dodot can't detect them, the key is omitted rather than silently set to an empty string. See section 5 for how to tolerate that.

    The `env` namespace is looked up on demand. `{{ env.EDITOR }}` calls the equivalent of `std::env::var("EDITOR")` at render time — so whatever the user has in their shell when `dodot up` runs is what gets baked in.

3. Defining Your Own Variables

    User variables live under `[preprocessor.template.vars]` in any `.dodot.toml` — root or pack. Values are strings; you reference them by bare name.

    Declaring vars in a root config:

        # ~/dotfiles/.dodot.toml
        [preprocessor.template.vars]
        editor = "nvim"
        host_tier = "workstation"

    :: toml ::

    Using them in a template:

        # ~/dotfiles/tools/aliases.sh.tmpl
        alias e='{{ editor }}'
        # tier: {{ host_tier }}

    :: shell ::

    Pack-level vars override root-level vars of the same name. The natural workflow: set defaults in the root `.dodot.toml`, then override per pack when a specific pack needs something different. No merging inside a single value — override replaces.

    Reserved names: `dodot` and `env` are the built-in namespaces, and cannot be used as variable names. dodot refuses to start up with a clear error if it finds `dodot = "..."` or `env = "..."` in `[preprocessor.template.vars]`.

4. Branching on Host or OS

    The common case for templates is conditional content. Jinja's `{% if %}` / `{% else %}` block handles it:

    OS-specific content:

        # ~/dotfiles/shell/profile.sh.tmpl
        {% if dodot.os == "macos" %}
        export HOMEBREW_PREFIX=/opt/homebrew
        {% else %}
        export HOMEBREW_PREFIX=/home/linuxbrew/.linuxbrew
        {% endif %}

    :: shell ::

    Hostname-based branching works the same way:

    Per-host branching:

        {% if dodot.hostname == "laptop" %}
        export PROMPT_SYMBOL="💻 "
        {% else %}
        export PROMPT_SYMBOL="🖥  "
        {% endif %}

    :: shell ::

    For anything more complex than OS or hostname, define your own classifier variable (e.g. `host_role = "work"` in the per-machine pack config) and branch on that.

5. Undefined Variables Are Errors; Defaults Are Explicit

    dodot runs templates in strict mode: referencing a variable that doesn't exist is a render error, and `dodot up` refuses to deploy that pack. This is deliberate — silently substituting an empty string on a typo is exactly how you end up with a `.gitconfig` that has `email = @example.com` and don't notice for six months.

    Undefined variable surfaces clearly:

        $ cat ~/dotfiles/vim/gvimrc.tmpl
        set background={{ theme }}

        $ dodot up vim
        ... template render failed for ~/dotfiles/vim/gvimrc.tmpl:
              undefined value (in <string>:1)
              hint: define the variable in [preprocessor.template.vars] in .dodot.toml,
              or reference an environment variable with {{ env.NAME }} (with a default filter if optional)

    :: shell ::

    When a value is genuinely optional, say so explicitly with Jinja's `default` filter:

    Tolerating optional values:

        editor = {{ env.EDITOR | default("nvim") }}
        host   = {{ dodot.hostname | default("unknown") }}
        tier   = {{ host_tier | default("unspecified") }}

    :: jinja ::

    `default` works for all three namespaces: env lookups, `dodot.*` keys that may not be detected, and user-defined vars. The rule of thumb: if a template can render without a value, make that explicit.

6. Disabling Preprocessing

    Two kill switches, both in `.dodot.toml`.

    Global kill switch:

        [preprocessor]
        enabled = false

    :: toml ::

    With `enabled = false`, `.tmpl` files are not rendered — they're deployed verbatim, extension intact. Useful when a pack ships a `.tmpl` file as-is (say, an example template that the user will customize later), or as a temporary circuit-breaker during debugging.

    Pack-level override: the same key under a pack's `.dodot.toml` overrides the root setting, so you can enable preprocessing globally and disable it for one specific pack.

7. Custom Extensions

    The default trigger extensions are `tmpl` and `template`. Override them in config:

    Custom extensions:

        [preprocessor.template]
        extensions = ["j2", "jinja"]

    :: toml ::

    A leading dot is tolerated: `".j2"` and `"j2"` are treated the same. When a filename matches more than one configured extension — e.g. `config.j2.tmpl` with both `"tmpl"` and `"j2.tmpl"` registered — dodot strips the longest match, independently of the order in which extensions are listed.

8. Collisions with Regular Files

    A pack cannot contain both `config.toml` and `config.toml.tmpl`: they would map to the same deployed name. Rather than picking one silently, dodot refuses to deploy the pack and reports the conflict:

    Collision diagnostic:

        $ dodot up app
        ... intent collection error: preprocessing collision in pack "app":
              config.toml.tmpl expands to config.toml, which conflicts with
              an existing pack file or another preprocessor's output

    :: shell ::

    The rule applies symmetrically to multiple preprocessors: if two preprocessors produce the same output name, the second one raises the same collision error.

9. For Developers: Where Rendered Output Lives

    Each rendered template is written to:

    Rendered file path:

        $XDG_DATA_HOME/dodot/packs/<pack>/preprocessed/<stripped-name>

    :: text ::

    That file is what the handlers see. The symlink handler creates `~/.gitconfig` pointing at the rendered file; the install handler executes the rendered `install.sh`; the path handler stages the rendered script into the PATH directory. Directory structure is preserved — `pack/sub/file.tmpl` renders to `.../preprocessed/sub/file`.

    Two consequences worth knowing:

    - _Diagnostics_ — if you want to see exactly what dodot produced for a given template, that directory is the source of truth.
    - _Hashing_ — the install handler derives its completion sentinel from the hash of the *rendered* script, not the `.tmpl` source. Changing a variable (or the value of `dodot.hostname` after moving machines) re-triggers the install step even if the template itself didn't change.
