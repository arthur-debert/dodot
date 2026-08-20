#!/usr/bin/env bash
# E2E test helper — stubs the `nix` binary on PATH so the nix handler
# can be exercised end-to-end without a real Nix install.
#
# The handler invokes a single nix subcommand at apply time:
#   nix profile install --impure \
#     --extra-experimental-features 'nix-command flakes' \
#     --argstr manifest /abs/path/packages.nix --expr <wrapper-expr>
#
# The manifest travels as the value of `--argstr manifest`, so the
# stub reads it straight off the command line to log which manifest
# was installed — that's the assertion surface bats tests care about
# (was install fired? against which manifest?).
#
# Real Nix is out of scope for the bats suite (no nix binary in CI;
# expensive setup); tier-0 unit tests in
# `crates/dodot-lib/src/handlers/nix.rs` cover argv construction
# and the wrapper-expression contents.
#
# Same precedent as the secrets-pass / secrets-bw stubs in
# `secrets_stubs.bash` and the brew muzzle in `helpers/setup.bash` —
# write a shim, prepend its dir to PATH, scrub at teardown alongside
# the rest of $SANDBOX.
#
# Install logging: every `nix profile install` invocation appends a
# line (the manifest path, when the stub can extract it) to
# `$SANDBOX/.nix-stub/install-log` so tests can assert on whether
# (and how often) the install path fired.
nix_stub_setup() {
	local stub_dir="$SANDBOX/.nix-stub"
	mkdir -p "$stub_dir"
	: >"$stub_dir/install-log"

	cat >"$stub_dir/nix" <<'STUB'
#!/usr/bin/env bash
# Stub `nix` for dodot e2e tests.
DIR="$(dirname "$0")"
LOG="$DIR/install-log"

case "$1" in
    --version)
        echo "nix-stub (Nix) 2.18.0"
        exit 0
        ;;
    profile)
        if [[ "$2" == "install" ]]; then
            shift 2
            path=""
            while [[ $# -gt 0 ]]; do
                case "$1" in
                    --argstr)
                        if [[ "$2" == "manifest" ]]; then path="$3"; fi
                        shift 3
                        ;;
                    *) shift ;;
                esac
            done
            # No `--argstr manifest` means the handler stopped naming
            # its manifest on the command line — log a marker rather
            # than a path so the drift is visible in test failures.
            if [[ -z "$path" ]]; then
                path="(no --argstr manifest)"
            fi
            printf '%s\n' "$path" >> "$LOG"
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
		wc -l <"$DODOT_NIX_STUB_LOG" | tr -d ' '
	else
		echo 0
	fi
}

# The manifest path from the most recent `nix profile install`.
nix_stub_last_manifest() {
	if [[ -f "$DODOT_NIX_STUB_LOG" ]]; then
		tail -1 "$DODOT_NIX_STUB_LOG"
	fi
}
