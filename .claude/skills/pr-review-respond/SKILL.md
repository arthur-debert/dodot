---
name: pr-review-respond
description: "Reply to and resolve PR review comments using only the `gh` CLI — no local helper scripts required. Use when processing Copilot or human review feedback on a PR in Claude Code Cloud, CI agents, or any environment without ~/h/release/bin scripts on $PATH. Triggered by: 'address review comments', 'resolve review threads', 'reply to Copilot', or any iteration on PR review feedback."
---

# pr-review-respond

Read, reply to, and resolve PR review comments using only `gh` and `jq`. Self-contained — no dependency on the `gh-*` helper scripts in `~/h/release/bin/` (those aren't available in Claude Code Cloud sessions or any environment that hasn't sourced the dotfiles).

## When to use

- Processing Copilot or human review feedback on a PR.
- Resolving review threads after fix-and-push or rationale-reply.
- Any environment without `~/h/release/bin/` on `$PATH`.

If you're working locally with the helper scripts available, the broader `gh-pr-review-loop` skill is a superset — it adds Copilot request/wait, check waiting, and onboarding flows. This skill is the comment-handling subset, in a form that works anywhere `gh` is authenticated.

## The three primitives

All commands assume you're inside the PR's repo. Set up once:

```sh
PR=<pr-number>
OWNER=$(gh repo view --json owner -q .owner.login)
REPO=$(gh repo view --json name -q .name)
```

### 1. List unresolved review threads

```sh
gh api graphql -F owner="$OWNER" -F name="$REPO" -F pr="$PR" -f query='
  query($owner: String!, $name: String!, $pr: Int!) {
    repository(owner: $owner, name: $name) {
      pullRequest(number: $pr) {
        reviewThreads(first: 100) {
          nodes {
            id
            isResolved
            path
            line
            originalLine
            comments(first: 20) {
              nodes {
                databaseId
                author { login }
                body
              }
            }
          }
        }
      }
    }
  }' | jq '[.data.repository.pullRequest.reviewThreads.nodes[]
           | select(.isResolved == false)
           | { threadId: .id,
               path: .path,
               line: (.line // .originalLine),
               firstCommentId: .comments.nodes[0].databaseId,
               author: .comments.nodes[0].author.login,
               body: .comments.nodes[0].body }]'
```

The output gives you everything needed to triage and act:
- `threadId` — the `PRRT_*` GraphQL ID to pass to `resolveReviewThread` (step 3).
- `firstCommentId` — the numeric REST `databaseId` to POST replies against (step 2).
- `path`, `line`, `author`, `body` — for triage.

### 2. Reply to a comment

```sh
COMMENT_ID=<firstCommentId from step 1>

gh api "repos/$OWNER/$REPO/pulls/$PR/comments/$COMMENT_ID/replies" \
  -X POST -f body="$(cat <<'EOF'
Reply markdown here. Multiple lines fine.

For rationale-style pushbacks, end with a searchable line so future passes can find it:
Recording for future review passes: don't ask us to <X>.
EOF
)"
```

Heredoc keeps quoting predictable for multi-line bodies. An empty body errors out.

### 3. Resolve the thread

```sh
THREAD_ID=<threadId from step 1, looks like "PRRT_kw...">

gh api graphql -F threadId="$THREAD_ID" -f query='
  mutation($threadId: ID!) {
    resolveReviewThread(input: { threadId: $threadId }) {
      thread { isResolved }
    }
  }'
```

GitHub does **not** auto-resolve threads when you push a fix or reply. Without this step, every addressed comment stays "Unresolved" through every subsequent round and the PR becomes unreadable. Resolve aggressively.

## Triage rules

For each unresolved thread, pick one:

**A) Real, project-specific issue → fix the code, push, resolve the thread.** The diff is the proof. Examples seen in this ecosystem:
- `cargo clippy -D warnings` must be `cargo clippy -- -D warnings` (the `--` forwards `-D` to rustc).
- `permissions: { pull-requests: write }` alone removes default `contents: read`; add `contents: read` explicitly.
- Fork PRs need a `github.event.pull_request.head.repo.fork == false` guard before posting reviewers.

**B) Project ethos drift → rationale-reply, then resolve. Do not change the file.** End the reply with `Recording for future review passes: don't ask us to <X>.` so it's grep-able next round. Examples that always get pushback in this ecosystem:
- "Pin org-internal reusable workflows to a SHA." Same owner controls both repos; pinning defeats the "fix once, propagate" point.
- "Per-repo customize the multi-repo template." The template is intentionally generic — pointing at only what's local defeats its purpose.
- "Match fallback flags exactly to CI." The fallback is a generic approximation; CI is the source of truth and varies per project.

**C) Cosmetic nit in already-merged style → skip.** Don't reply. Only push back if the same nit recurs across PRs — then generally, in copilot-instructions.md.

Healthy end state: only genuinely contested threads (and the original review summary, which isn't itself a thread) remain unresolved.

## After fixup pushes

CI re-runs automatically on push. **Do not** re-request Copilot for minor follow-up rounds — the canonical `copilot-review.yml` workflow only auto-triggers on `pull_request: [opened, ready_for_review]`, and one Copilot review per PR is the convention. Re-request manually only if the round is substantial:

```sh
gh pr edit "$PR" --add-reviewer @copilot
```

(`requested_reviewers` REST POST silently no-ops for Copilot — must go through `gh pr edit`, which uses GraphQL with the bot's real node_id.)

## Stop at "ready to merge"

When all addressed threads are resolved and checks are green, the comment-handling phase is done. Report status and stop. The user does the final read and merges.

```sh
gh pr view "$PR" --json mergeStateStatus,mergeable
# Ready when: mergeStateStatus=CLEAN, mergeable=MERGEABLE
```

Wait for checks before declaring done:

```sh
gh pr checks "$PR" --watch
```

Merge only on explicit authorization from the user ("merge it", "go ahead and merge", "merge when green"). Then:

```sh
gh pr merge "$PR" --squash --delete-branch
```

## Pitfalls

- **REST `databaseId` vs GraphQL `id` are different namespaces.** The reply REST endpoint takes the numeric `databaseId`; the resolve mutation takes the `PRRT_*` string. Don't cross them.
- **`line` is null for outdated/multi-line comments.** Fall back to `originalLine` (the step-1 query already does this).
- **Already-resolved thread → mutation returns `isResolved: true`.** Safe to no-op; check `isResolved` upstream if you want to skip the call entirely.
- **Empty reply body errors out.** Always send text.
- **Copilot `requested_reviewers` REST add silently no-ops.** Use `gh pr edit --add-reviewer @copilot`.
- **Don't `--admin` merge unprompted.** If the PR is blocked by an unrelated pre-existing CI failure, surface it and ask before bypassing.
