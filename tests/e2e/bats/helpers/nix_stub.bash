#!/usr/bin/env bash
# E2E test helper — stubs the `nix` binary at the location dodot
# probes, so the nix handler can be exercised end-to-end without a
# real Nix install.
#
# Not on PATH, deliberately. dodot locates a provisioner by testing a
# fixed list of absolute candidates and then spawns the path that
# answered (ADR-0007), so a stub reachable only through PATH would
# make every one of these tests report "nix not installed". The stub
# goes to `~/.nix-profile/bin/nix` — the first candidate, and where a
# real per-user Nix install puts the binary — and PATH is left alone,
# which makes these tests a regression guard for both halves: dodot
# must find the stub, and must run the absolute path it found.
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
# `secrets_stubs.bash` — write a shim inside the sandbox and let
# teardown scrub it with the rest of $SANDBOX.
#
# Install logging: every `nix profile install` invocation appends a
# line (the manifest path, when the stub can extract it) to
# `$SANDBOX/.nix-stub/install-log` so tests can assert on whether
# (and how often) the install path fired. The log path is written
# into the stub at generation time, since the stub is invoked by its
# absolute path and cannot derive the sandbox from its own location.
nix_stub_setup() {
	local bin_dir="$HOME/.nix-profile/bin"
	local log="$SANDBOX/.nix-stub/install-log"
	mkdir -p "$bin_dir" "$(dirname "$log")"
	: >"$log"

	{
		printf '#!/usr/bin/env bash\n'
		printf 'LOG=%q\n' "$log"
		cat <<'STUB'
# Stub nix for dodot e2e tests.
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
	} >"$bin_dir/nix"
	chmod +x "$bin_dir/nix"

	export DODOT_NIX_STUB_LOG="$log"
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
		tail -n 1 "$DODOT_NIX_STUB_LOG"
	fi
}
