#!/usr/bin/env bash
# E2E test helper — stubs the `nix` binary on PATH so the nix handler
# can be exercised end-to-end without a real Nix install.
#
# The stub covers exactly the surface
# `crates/dodot-lib/src/handlers/nix.rs` calls:
#   - `nix eval --file <path> --json --apply <expr> ...` for shape probing
#   - `nix profile install --file <path> ...` for installation
#
# Same precedent as the secrets-pass / secrets-bw stubs in
# `secrets_stubs.bash` and the brew muzzle in `helpers/setup.bash` —
# write a shim, prepend its dir to PATH, scrub at teardown alongside
# the rest of $SANDBOX.
#
# Shape classification: rather than parse Nix syntax in bash, the stub
# inspects the manifest for a `# stub-shape: <list|drv|set|unsupported>`
# marker comment. Manifests without a marker default to `"list"` (the
# most common test shape). Tests exercising the non-list paths set the
# marker explicitly.
#
# Install logging: every `nix profile install` invocation appends a
# line to `$SANDBOX/.nix-stub/install-log` so tests can assert on
# whether (and how often) the install path fired.
nix_stub_setup() {
    local stub_dir="$SANDBOX/.nix-stub"
    mkdir -p "$stub_dir"
    : > "$stub_dir/install-log"

    cat > "$stub_dir/nix" <<'STUB'
#!/usr/bin/env bash
# Stub `nix` for dodot e2e tests.
DIR="$(dirname "$0")"
LOG="$DIR/install-log"

case "$1" in
    --version)
        echo "nix-stub (Nix) 2.18.0"
        exit 0
        ;;
    eval)
        # Expected shape:
        #   nix eval --file <path> --json --apply <expr> \
        #     --extra-experimental-features 'nix-command flakes'
        path=""
        shift
        while [[ $# -gt 0 ]]; do
            case "$1" in
                --file) path="$2"; shift 2 ;;
                *) shift ;;
            esac
        done
        shape="list"
        if [[ -n "$path" && -r "$path" ]]; then
            marker="$(grep -m1 -oE '# *stub-shape: *[a-z]+' "$path" 2>/dev/null \
                || true)"
            if [[ -n "$marker" ]]; then
                # Strip everything up to and including the last `:` or
                # space — handles `# stub-shape: list`, `#stub-shape:list`,
                # and the awkward `#stub-shape: list` consistently.
                shape="${marker##*[: ]}"
            fi
        fi
        # Mimic `nix eval --json` output: a JSON-encoded string.
        printf '"%s"\n' "$shape"
        exit 0
        ;;
    profile)
        if [[ "$2" == "install" ]]; then
            shift 2
            file=""
            while [[ $# -gt 0 ]]; do
                case "$1" in
                    --file) file="$2"; shift 2 ;;
                    *) shift ;;
                esac
            done
            printf '%s\n' "$file" >> "$LOG"
            exit 0
        fi
        echo "nix stub: unsupported profile subcommand: $2" >&2
        exit 2
        ;;
    *)
        echo "nix stub: unsupported command: $1" >&2
        exit 2
        ;;
esac
STUB
    chmod +x "$stub_dir/nix"

    export PATH="$stub_dir:$PATH"
    export DODOT_NIX_STUB_LOG="$stub_dir/install-log"
}

# Count of `nix profile install` invocations the stub has logged.
nix_stub_install_count() {
    if [[ -f "$DODOT_NIX_STUB_LOG" ]]; then
        wc -l < "$DODOT_NIX_STUB_LOG" | tr -d ' '
    else
        echo 0
    fi
}
