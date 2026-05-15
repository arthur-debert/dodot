#!/usr/bin/env bash
# scripts/setup-dev-env.sh — per-session dev-environment setup, invoked by
# the SessionStart hook in .claude/settings.json.
#
# Cloud-only: local sessions exit early (devs already have their env set up).
# Detects stack by filesystem signals — works for rust, node-flavored
# (npm/yarn/pnpm), ruby (bundle), and nvim/zed/static-site (no project
# deps, just lefthook wiring). Stack-specific extras (e.g. resource
# download scripts, submodule init) can be added below the universal
# section as needed for the particular repo.
#
# Idempotent — safe to re-run. Errors are best-effort: a failure in one
# step doesn't abort the rest (e.g. transient registry hiccup on cargo
# fetch shouldn't block the lefthook install).

set -euo pipefail

# Cloud-only gate. Local sessions already have their env set up.
[ "${CLAUDE_CODE_REMOTE:-}" = "true" ] || exit 0

REPO_ROOT="$(git rev-parse --show-toplevel)"
cd "${REPO_ROOT}"

# 1. Project dep cache — pick the right tool based on lockfile / manifest.

# Rust: cargo fetch with --locked so we don't silently mutate Cargo.lock
# in the per-session clone. Stale lockfile produces a non-fatal exit;
# the agent's later cargo build/test surfaces the real issue.
if [ -f Cargo.toml ] && command -v cargo >/dev/null 2>&1; then
  cargo fetch --locked --quiet || true
fi

# Node-based (npm / yarn / pnpm). Skip if node_modules already exists
# (warm from a previous session within the same env-snapshot).
if [ -f package.json ] && [ ! -d node_modules ]; then
  if [ -f package-lock.json ] && command -v npm >/dev/null 2>&1; then
    npm ci 2>/dev/null || npm install
  elif [ -f yarn.lock ] && command -v yarn >/dev/null 2>&1; then
    yarn install --frozen-lockfile 2>/dev/null || yarn install
  elif [ -f pnpm-lock.yaml ] && command -v pnpm >/dev/null 2>&1; then
    pnpm install --frozen-lockfile 2>/dev/null || pnpm install
  fi
fi

# Ruby / Bundler.
if [ -f Gemfile ] && command -v bundle >/dev/null 2>&1; then
  bundle install --quiet || true
fi

# 2. Pre-commit hook wiring (lefthook).
# Binary is installed at env-setup time (arthur-debert/release env/setup.sh);
# this just wires .git/hooks/pre-commit to call it. Errors are surfaced
# loudly — the whole point of the script is the hook install.
if [ -f lefthook.yml ] && command -v lefthook >/dev/null 2>&1; then
  if ! lefthook install; then
    echo "warning: lefthook install failed — pre-commit hook NOT wired" >&2
  fi
fi

exit 0
