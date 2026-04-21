Architecture

    This document describes how dodot is organized — the pipeline each command flows through, the layers that do the work, and the boundaries between them. It is the conceptual view. For concrete types, crate layout, and trait signatures, see [./../dev/types-and-structure.lex].

    :: note :: See [./terms-and-concepts.lex] for terminology used throughout.

1. The Unified Pipeline

    Every pack-based dodot command — `up`, `down`, `status`, and friends — runs through the same pipeline. Commands differ in what they do at each phase, not in which phases they visit. This is deliberate: it means `status` truly predicts what `up` will do, because they follow the same path and `status` simply stops short of executing.

    Pipeline phases:

        discover packs
        -> match rules against pack files
        -> run preprocessors on matched template/plist/encrypted files
        -> group matches by handler
        -> ask each handler for intents
        -> convert intents to operations
        -> dispatch operations to the datastore

    :: text ::

    Each arrow is a layer boundary. The phases before the arrow produce data; the phase after consumes that data and produces new data. There are no back-edges — no phase reaches backward into an earlier one. This is what `--dry-run` exploits: you stop the flow before the last arrow, and you have a complete picture of what _would_ have happened.

2. What Each Phase Does

    2.1. Pack Discovery

        dodot enumerates top-level directories of the dotfiles root, skipping any that contain `.dodotignore` and any that match global ignore patterns (`.git`, `node_modules`, etc.). The result is the set of packs to process. When a command is invoked with explicit pack names (`dodot up git nvim`), only those are discovered.

    2.2. Rule Matching

        Each pack's top-level entries (files and directories immediately under the pack root) are matched against the rule set. Rules declare patterns, the handler that should claim a match, and a priority. Higher priority wins; first match wins at equal priority. Exclusion rules (prefixed `!`) remove entries from consideration before any handler sees them.

        The scanner is top-level only. It does _not_ recurse into subdirectories. A handler that receives a directory entry decides for itself what to do with the contents.

    2.3. Preprocessing

        After rule matching, the set of matches is partitioned into preprocessor candidates (files with preprocessor extensions like `.tmpl`, `.plist.xml`, `.age`) and everything else. Preprocessor candidates are transformed — rendered, converted, decrypted — and their outputs are written to the datastore. Each output appears back in the pipeline as a virtual match for the stripped filename, which re-enters rule matching to determine the downstream handler.

        Collisions (e.g., a pack containing both `config.toml` and `config.toml.tmpl`) are detected here and raised as errors before any handler runs.

    2.4. Handler Grouping

        Matches are grouped by the handler that claimed them. Each group is handed to its handler as a batch. This lets handlers make decisions that span multiple files — the symlink handler, for example, can reason about whether a whole directory should be linked wholesale or per-file based on whether any nested path was independently claimed.

    2.5. Intents

        Each handler inspects its group and produces a list of intents. An intent is a declaration: "I want this file linked to that target"; "I want this script executable and sourced at login"; "I want this command run once, keyed by this sentinel." Intents are shape-limited to three kinds — link, stage, run — so the executor has a small surface to handle.

        Handlers never touch the filesystem. They read matches and write intents.

    2.6. Operations

        Intents are converted to operations by the executor. An operation is a concrete filesystem-level verb: create a data link, create a user link, run a command, check a sentinel. A single intent can produce multiple operations (a `Link` intent produces one `CreateDataLink` plus one `CreateUserLink`). Operations are what actually get dispatched.

        This is the layer that implements `--dry-run`: the operation list is printed instead of executed.

    2.7. Datastore Dispatch

        Operations are routed to the datastore, which executes them. The datastore is responsible for everything under `$XDG_DATA_HOME/dodot/` — creating intermediate symlinks, writing sentinel files, running commands, cleaning up on `down`. The datastore is an abstraction with a small API surface; the rest of the codebase does not know how state is stored.

3. Layers and Their Responsibilities

    The pipeline maps to a stack of layers, each with one job.

    Commands:
        Parse arguments, build an execution context, call into the orchestration layer. Commands know nothing about handlers, rules, or the filesystem.

    Orchestration:
        Run the pipeline for a command across the selected packs. This is the single entry point for any pack-based work.

    Rules and Scanner:
        Walk a pack, apply rules, produce matches. Deterministic, stateless.

    Preprocessing:
        Transform source files before handler dispatch. Declares the three transform shapes (generative, representational, opaque). See [./pre-processors.lex].

    Handlers:
        Convert matches into intents. One handler per concern. Small; focused; no filesystem access.

    Executor:
        Convert intents into operations and dispatch them. Handles `--dry-run` by short-circuiting dispatch.

    DataStore:
        Execute operations against the on-disk state. The single component that touches the datastore directory.

    Filesystem (Fs) and Pather:
        Low-level abstractions for filesystem access and path resolution. Swappable for testing.

    Each layer depends only on the layers below it. A handler can't reach into the orchestration layer; an operation can't look up a rule.

4. Commands and the Pipeline

    Most commands are thin wrappers around a specific shape of pipeline traversal.

    - `up`: runs the full pipeline, executes operations.
    - `down`: skips rule matching and handlers entirely; asks the datastore to remove all state for the pack.
    - `status`: runs through intents, asks each handler to report its current deployment state against the datastore, stops before executing.
    - `list`: pack discovery only.
    - `adopt`, `fill`, `init`, `addignore`: operate on pack content directly, not on deployment state. They use a smaller, pack-scoped API rather than the full pipeline.

    Two commands sit outside the pack pipeline entirely:

    - `config`: inspects resolved configuration.
    - `init-sh`: generates the shell integration script from the current datastore.

5. Execution Properties

    The pipeline gives you several properties that are worth naming explicitly.

    - _Deterministic._ Given the same inputs (pack files, rules, config), the operation list is identical across runs and machines.
    - _Idempotent at the boundary._ Running `dodot up` a second time produces the same operations but yields no change, because configuration handlers use idempotent filesystem operations and code-execution handlers are gated by sentinels.
    - _Previewable._ `--dry-run` reports the exact operation list that would have run.
    - _Fail-fast._ The pipeline stops on the first error in execution and reports it with context. Partial state is allowed (and legible, via the datastore); silent drift is not.
    - _Per-pack isolation._ Packs are processed independently. A failure in one pack does not prevent other packs from succeeding, though the command's exit code still reflects the failure.

6. What Lives Where

    For the mapping from this conceptual architecture to actual crates, modules, and types, see [./../dev/types-and-structure.lex]. For the DataStore API surface, see [./../dev/storage.lex]. For config resolution internals, see [./../dev/config-system.lex]. For the standout-based output layer, see [./../dev/cli-output.lex].
