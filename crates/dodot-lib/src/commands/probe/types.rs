//! Display shapes returned by every `probe` subcommand.
//!
//! Pure data: structs and enums consumed by the Jinja templates. The
//! [`ProbeResult`] enum is the single tagged-serde return type all
//! entry points produce.

use serde::Serialize;

/// Default max depth for `probe show-data-dir`. Enough to show
/// `packs / <pack> / <handler> / <entry>` without scrolling off
/// screen for reasonable installs; deeper subtrees are summarised.
pub const DEFAULT_SHOW_DATA_DIR_DEPTH: usize = 4;

/// Default cap on the number of history rows emitted, so a user with
/// hundreds of profiles doesn't get a page-filling race down their
/// terminal.
pub const DEFAULT_HISTORY_LIMIT: usize = 50;

/// Default window for the filtered drill-down view. Wider than
/// `RUNTIME_FAILURE_WINDOW` (used by `status`) so a user looking at
/// `dodot probe shell-init <file>` gets enough history to see whether
/// the failure is recurring or one-off, but bounded so the rendered
/// output stays readable.
pub const DEFAULT_FILTER_RUNS: usize = 20;

/// Display-shaped deployment-map row. Paths are pre-shortened to
/// `~/…` where they live under HOME so the rendered table stays
/// narrow; the machine-readable TSV on disk keeps absolute paths.
#[derive(Debug, Clone, Serialize)]
pub struct DeploymentDisplayEntry {
    pub pack: String,
    pub handler: String,
    pub kind: String,
    /// Pre-shortened (`~/…`) absolute source path; empty for
    /// non-symlink entries (sentinels, rendered files).
    pub source: String,
    /// Pre-shortened absolute datastore path.
    pub datastore: String,
}

/// One line of tree output, pre-flattened for the template.
///
/// The template for a tree is annoying to write directly in Jinja
/// (indentation, prefix characters, etc.), so we flatten the tree to
/// a list of `(indent, name, annotation)` triples here.
#[derive(Debug, Clone, Serialize)]
pub struct TreeLine {
    /// Indent prefix (e.g. `"  │  ├─ "`).
    pub prefix: String,
    /// The node's display name (basename for non-root nodes).
    pub name: String,
    /// A dim-styled annotation shown after the name (size, link target,
    /// truncation count). Empty when the node has nothing extra to say.
    pub annotation: String,
}

/// Result of any `probe` invocation. Serialises with a `kind` tag so
/// the Jinja template can dispatch on it.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
pub enum ProbeResult {
    /// `dodot probe` with no subcommand — a summary pointing the user
    /// at the real subcommands.
    Summary {
        data_dir: String,
        available: Vec<ProbeSubcommandInfo>,
    },
    /// `dodot probe deployment-map` — the source↔deployed map.
    DeploymentMap {
        data_dir: String,
        map_path: String,
        entries: Vec<DeploymentDisplayEntry>,
    },
    /// `dodot probe show-data-dir` — a bounded tree view of
    /// `<data_dir>`.
    ShowDataDir {
        data_dir: String,
        /// Flattened, template-ready lines.
        lines: Vec<TreeLine>,
        total_nodes: usize,
        /// Size in bytes of the whole tree (symlinks counted by their
        /// link-entry size).
        total_size: u64,
    },
    /// `dodot probe shell-init` — the most recent shell-startup profile,
    /// grouped by pack and handler.
    ShellInit(ShellInitView),
    /// `dodot probe shell-init --runs N` — per-target percentile stats
    /// across the last N runs.
    ShellInitAggregate(ShellInitAggregateView),
    /// `dodot probe shell-init --history` — one summary line per recent
    /// run, newest first (matches every other dated listing in the tool;
    /// the user can pipe through `tac` if they want the inverse).
    ShellInitHistory(ShellInitHistoryView),
    /// `dodot probe shell-init <pack>[/<file>]` — drill-down view of
    /// one target (or one pack) across recent runs. Emits per-run
    /// duration, exit status, and captured stderr (when any) so the
    /// user can pinpoint *what* a failing source file printed.
    ShellInitFilter(ShellInitFilterView),
    /// `dodot probe shell-init --errors-only` — every target with at
    /// least one non-zero exit across the examined window, grouped by
    /// target and sorted by failure count (most-broken first).
    ShellInitErrors(ShellInitErrorsView),
    /// `dodot probe app <pack>` — advisory introspection of macOS
    /// app-support paths for a single pack: which folder names this
    /// pack will route to, whether they exist, matching homebrew cask
    /// metadata, and `.app` bundle / bundle-id pairs from Spotlight.
    /// See `docs/proposals/macos-paths.lex` §8.4.
    App(AppProbeView),
}

/// Display payload for `dodot probe app <pack>`.
#[derive(Debug, Clone, Serialize)]
pub struct AppProbeView {
    pub pack: String,
    /// Whether the host platform supports the macOS-only probes
    /// (homebrew cask + Spotlight). On Linux this is `false` and the
    /// `entries` list reflects only the deterministic info available
    /// from the resolver — no cask/bundle data.
    pub macos: bool,
    /// One row per app-folder name this pack would route to. May be
    /// empty for a pack with no `_app/`/`force_app`/`app_aliases`
    /// entries.
    pub entries: Vec<AppProbeEntry>,
    /// Sibling-adoption suggestions surfaced from the matching cask's
    /// zap stanza (e.g. `~/Library/Preferences/<bundle>.plist`).
    pub suggested_adoptions: Vec<String>,
}

/// One row per app-support folder a pack will deploy to.
#[derive(Debug, Clone, Serialize)]
pub struct AppProbeEntry {
    /// The destination folder name, e.g. `"Code"`.
    pub folder: String,
    /// `<app_support_dir>/<folder>/` path. Always populated, even when
    /// the folder doesn't exist on disk — the renderer shortens to
    /// `~/...` for display.
    pub target_path: String,
    /// Whether `target_path` exists on the local filesystem.
    pub target_exists: bool,
    /// Source rule that produced this folder: `"alias"`, `"force_app"`,
    /// or `"_app/"`. Drives display.
    pub source_rule: String,
    /// Matching homebrew cask token, when found. Always an
    /// *installed* cask (matching only iterates `brew list --cask
    /// --versions`); a `Some` value implies "installed". A `None`
    /// value means either no installed cask declared this folder in
    /// its zap stanza, or we're not on macOS.
    pub cask: Option<String>,
    /// `.app` bundle name derived from cask metadata, e.g.
    /// `"Visual Studio Code.app"`.
    pub app_bundle: Option<String>,
    /// `kMDItemCFBundleIdentifier` for the `.app` bundle, when
    /// resolvable via `mdls`.
    pub bundle_id: Option<String>,
}

/// Display payload for `--runs N`.
#[derive(Debug, Clone, Serialize)]
pub struct ShellInitAggregateView {
    /// How many profiles were actually loaded (may be smaller than the
    /// requested N if there aren't enough on disk yet).
    pub runs: usize,
    /// User-requested N (echoed back so the renderer can say
    /// "showing 4 of last 10 requested").
    pub requested_runs: usize,
    pub profiling_enabled: bool,
    pub profiles_dir: String,
    pub rows: Vec<ShellInitAggregateRow>,
    /// True when the newest aggregated profile was captured before the
    /// most recent `dodot up`. The renderer prints a freshness banner
    /// in that case so the user knows to open a new shell.
    pub stale: bool,
    /// `YYYY-MM-DD HH:MM` capture time of the newest aggregated
    /// profile; empty when no profiles were loaded.
    pub latest_profile_when: String,
    /// `YYYY-MM-DD HH:MM` of the most recent `dodot up`; empty when
    /// `up` has never run on this machine.
    pub last_up_when: String,
}

/// One per-target aggregate row, durations pre-humanised for the
/// template.
#[derive(Debug, Clone, Serialize)]
pub struct ShellInitAggregateRow {
    pub pack: String,
    pub handler: String,
    pub target: String,
    pub p50_label: String,
    pub p95_label: String,
    pub max_label: String,
    pub p50_us: u64,
    pub p95_us: u64,
    pub max_us: u64,
    /// e.g. `"7/10"` — formatted at the lib so JSON consumers and the
    /// template both render identically.
    pub seen_label: String,
    pub runs_seen: usize,
    pub runs_total: usize,
}

/// Display payload for `--history`.
#[derive(Debug, Clone, Serialize)]
pub struct ShellInitHistoryView {
    pub profiling_enabled: bool,
    pub profiles_dir: String,
    pub rows: Vec<ShellInitHistoryRow>,
    /// True when the newest row was captured before the most recent
    /// `dodot up`. Older rows in the history are obviously older —
    /// they're not flagged individually.
    pub stale: bool,
    /// `YYYY-MM-DD HH:MM` capture time of the newest history row;
    /// empty when no profiles exist.
    pub latest_profile_when: String,
    /// `YYYY-MM-DD HH:MM` of the most recent `dodot up`; empty when
    /// `up` has never run on this machine.
    pub last_up_when: String,
}

/// One per-run row in `--history`.
#[derive(Debug, Clone, Serialize)]
pub struct ShellInitHistoryRow {
    /// Filename of the underlying TSV — useful for cross-reference and
    /// keeps history rows traceable to the on-disk artefact.
    pub filename: String,
    /// Unix timestamp parsed from the filename (or `0` when the
    /// filename doesn't follow the expected pattern). Surfaced in JSON
    /// so machine consumers can do their own date math without
    /// re-parsing `filename`.
    pub unix_ts: u64,
    /// Compact `YYYY-MM-DD HH:MM` formatted from the unix timestamp in
    /// the filename. Empty when the timestamp couldn't be parsed.
    pub when: String,
    pub shell: String,
    pub total_label: String,
    pub user_total_label: String,
    pub total_us: u64,
    pub user_total_us: u64,
    pub failed_entries: usize,
    pub entry_count: usize,
}

/// Display payload for `probe shell-init`. Pulled into its own struct
/// so the JSON view stays clean and the variant constructor in
/// `shell_init()` reads naturally.
#[derive(Debug, Clone, Serialize)]
pub struct ShellInitView {
    /// Source filename of the report (for "which run is this?" UX).
    /// Empty when no profile has been written yet.
    pub filename: String,
    /// Shell label as recorded in the preamble (e.g. `bash 5.3.9`).
    pub shell: String,
    /// True when the profiling wrapper is enabled in config.
    pub profiling_enabled: bool,
    /// True when the directory exists and contained a parseable file.
    pub has_profile: bool,
    /// Pre-grouped rows for the template; empty when `has_profile` is
    /// false.
    pub groups: Vec<ShellInitGroup>,
    pub user_total_us: u64,
    pub framing_us: u64,
    pub total_us: u64,
    /// Where the profiles live on disk (so the user can `ls` it).
    pub profiles_dir: String,
    /// True when the displayed profile was captured before the most
    /// recent `dodot up`. The renderer prints a freshness banner so
    /// the user knows the timings reflect a pre-up shell.
    pub stale: bool,
    /// `YYYY-MM-DD HH:MM` capture time of the displayed profile;
    /// empty when no profile is available.
    pub profile_when: String,
    /// `YYYY-MM-DD HH:MM` of the most recent `dodot up`; empty when
    /// `up` has never run on this machine.
    pub last_up_when: String,
}

/// Display row for one entry in a shell-init group.
#[derive(Debug, Clone, Serialize)]
pub struct ShellInitRow {
    pub target: String,
    pub duration_us: u64,
    pub duration_label: String,
    pub exit_status: i32,
    /// `"deployed"` (success — rendered green) or `"error"` (non-zero
    /// source exit). These map directly to existing styles in
    /// `crate::render`'s theme; using fresh names here would require
    /// theme additions for no UX gain.
    pub status_class: &'static str,
}

/// Display group: one (pack, handler) bucket of shell-init rows.
#[derive(Debug, Clone, Serialize)]
pub struct ShellInitGroup {
    pub pack: String,
    pub handler: String,
    pub rows: Vec<ShellInitRow>,
    pub group_total_us: u64,
    pub group_total_label: String,
}

/// Display payload for the filtered drill-down view.
///
/// Renders per-run history of one target (when the filter narrows to a
/// single file) or every target in a pack across recent runs. Emits the
/// captured stderr inline so the user can see exactly what each failing
/// source printed without leaving the terminal.
#[derive(Debug, Clone, Serialize)]
pub struct ShellInitFilterView {
    pub profiling_enabled: bool,
    pub profiles_dir: String,
    /// Filter as the user typed it — echoed in the header.
    pub filter: String,
    /// Pack portion of the filter (always set).
    pub filter_pack: String,
    /// Filename portion of the filter, if any (the part after `/`).
    pub filter_filename: Option<String>,
    /// Number of profiles examined.
    pub runs_examined: usize,
    /// One block per matching target. When the filter is a specific
    /// file, this contains at most one block. When it's a pack-only
    /// filter, one block per target seen in the pack across the
    /// examined runs.
    pub targets: Vec<ShellInitFilterTarget>,
    pub stale: bool,
    pub latest_profile_when: String,
    pub last_up_when: String,
}

/// One target's runs across the examined window.
#[derive(Debug, Clone, Serialize)]
pub struct ShellInitFilterTarget {
    /// Full source path as recorded in the profile.
    pub target: String,
    /// Basename for header display.
    pub display_target: String,
    /// Pack the target belongs to.
    pub pack: String,
    /// Handler (`shell` for sourced files, `path` for PATH exports).
    pub handler: String,
    /// Per-run rows, newest first.
    pub runs: Vec<ShellInitFilterRun>,
    /// How many of `runs` had a non-zero exit status.
    pub failure_count: usize,
}

/// Display payload for `--errors-only`. Same shape as the filter view
/// minus the user-typed filter string — the implicit filter is "non-
/// zero exit, any pack, any target".
#[derive(Debug, Clone, Serialize)]
pub struct ShellInitErrorsView {
    pub profiling_enabled: bool,
    pub profiles_dir: String,
    pub runs_examined: usize,
    /// Targets with at least one failed run in the window, sorted by
    /// failure count desc (then by pack/target asc as a tiebreaker so
    /// the order is stable across runs with the same counts).
    pub targets: Vec<ShellInitFilterTarget>,
    pub stale: bool,
    pub latest_profile_when: String,
    pub last_up_when: String,
}

/// One per-run row inside a target block.
#[derive(Debug, Clone, Serialize)]
pub struct ShellInitFilterRun {
    /// `YYYY-MM-DD HH:MM` of the run.
    pub when: String,
    /// Pre-humanised duration label (e.g. `"83 µs"`).
    pub duration_label: String,
    pub duration_us: u64,
    pub exit_status: i32,
    /// `"deployed"` (success) or `"error"` (non-zero exit) — maps to
    /// the same theme styles used by the unfiltered view.
    pub status_class: &'static str,
    /// Captured stderr split into individual lines. Empty when the
    /// source printed nothing to stderr in this run. Pre-split because
    /// the template engine doesn't expose a `.split()` filter, and
    /// rendering each line with its own indent is cleaner than fighting
    /// the template language.
    pub stderr_lines: Vec<String>,
    /// Source TSV filename, for cross-reference.
    pub profile_filename: String,
}

/// One entry in the `probe` summary listing.
#[derive(Debug, Clone, Serialize)]
pub struct ProbeSubcommandInfo {
    pub name: &'static str,
    pub description: &'static str,
}

/// The full list of probe subcommands, used by the summary view.
/// Keeping them in one array keeps the CLI registration, clap
/// registration, and summary output trivially in sync.
pub const PROBE_SUBCOMMANDS: &[ProbeSubcommandInfo] = &[
    ProbeSubcommandInfo {
        name: "deployment-map",
        description: "Source↔deployed map — what dodot linked where.",
    },
    ProbeSubcommandInfo {
        name: "shell-init",
        description: "Per-source timings for the most recent shell startup.",
    },
    ProbeSubcommandInfo {
        name: "show-data-dir",
        description: "Tree of dodot's data directory, with sizes.",
    },
];
