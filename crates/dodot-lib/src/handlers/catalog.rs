//! The handler catalog: one descriptor per registered handler, and
//! the renderer that turns the registry into the handler tables the
//! documentation publishes.
//!
//! # Why this exists
//!
//! The same handful of facts about each handler — its phase, its
//! category, what it claims by default, what it does with a claim —
//! used to be transcribed by hand into roughly eight documentation
//! tables. Adding `nix`, `external`, and `gate` updated some of them
//! and missed the rest, so the published taxonomy described a
//! registry dodot had not shipped for months.
//!
//! So the canonical table is no longer written by hand.
//! [`render_registry_doc`] renders `docs/reference/handler-registry.lex`
//! from [`create_registry`](crate::handlers::create_registry) plus
//! [`mappings_to_rules`](crate::config::mappings_to_rules) over the
//! default [`MappingsSection`](crate::config::MappingsSection), and a
//! test in this module fails when the checked-in file drifts from
//! what the registry would render. That page is the copy that cannot
//! disagree with the shipped registry, and it is what a document
//! should link to when it needs the whole roster.
//!
//! # The rosters that stay hand-written
//!
//! Generation did not remove every other list, and was not meant to.
//! A handful of pages name the handlers *in the middle of explaining
//! something else* — the README's tour, the priority ladder in
//! `mappings.lex`, the phase table in `execution-order.lex` — where a
//! link to a separate page would cost the reader more than the
//! duplication does. Those pages are enumerated in
//! [`CONTEXTUAL_ROSTERS`]; each marks its roster with
//! [`ROSTER_BEGIN`] / [`ROSTER_END`], and the test
//! `contextual_rosters_list_every_handler` reads what sits between
//! those markers and holds it to naming every registered handler.
//! Reading only the marked region is the point: names like `path` and
//! `gate` turn up all over ordinary prose, so a page-wide search would
//! stay green even after a handler was dropped from the table it
//! belongs in.
//!
//! So a new handler owes the docs three things, not one:
//!
//! 1. A [`HandlerDoc`] row here — the test refuses a registry entry
//!    with no row, and a row with no registry entry.
//! 2. A `pixi run gen-docs`, which regenerates the page from the
//!    registry; everything in its table comes from code the handler
//!    already had to write.
//! 3. A line inside the marked roster of each page in
//!    [`CONTEXTUAL_ROSTERS`], plus a user-facing snippet under
//!    `docs/user/handlers/`. The roster test catches a roster that
//!    forgot the handler; it cannot catch one that describes it
//!    wrongly, so read what you are editing.
//!
//! # What lives in the descriptor, and what does not
//!
//! A [`HandlerDoc`] carries only what cannot be read off the code:
//! the one-line description of what a claimed file gets, and — for
//! the handlers whose matches are not produced by `[mappings]`
//! patterns — how to describe what they claim. Phase, category, match
//! mode, scope, default patterns, and rule priority are all read from
//! the registry and the config defaults at render time, so they
//! cannot disagree with the shipped behaviour.

use std::collections::BTreeMap;

use crate::config::{mappings_to_rules, MappingsSection};
use crate::fs::Fs;
use crate::handlers::{
    create_registry, ExecutionPhase, HandlerCategory, HandlerScope, MatchMode, HANDLER_EXTERNAL,
    HANDLER_GATE, HANDLER_HOMEBREW, HANDLER_IGNORE, HANDLER_INSTALL, HANDLER_NIX, HANDLER_PATH,
    HANDLER_SHELL, HANDLER_SKIP, HANDLER_SYMLINK,
};

/// The path of the generated page, relative to the repository root.
pub const REGISTRY_DOC_PATH: &str = "docs/reference/handler-registry.lex";

/// The opening marker of a hand-written roster region.
///
/// Written as `<!-- handler-roster:begin -->` in Markdown and
/// `:: handler-roster:begin ::` in lex; both render as an invisible
/// comment, and both carry this token verbatim in the source, which
/// is what [`roster_regions`] looks for.
pub const ROSTER_BEGIN: &str = "handler-roster:begin";

/// The closing marker of a hand-written roster region.
pub const ROSTER_END: &str = "handler-roster:end";

/// The pages that keep a hand-written handler roster, relative to the
/// repository root.
///
/// Each names the handlers as part of explaining something else, so
/// generation would not fit and a bare link would read worse. The
/// price is an update obligation per new handler, and
/// `contextual_rosters_list_every_handler` is what collects it:
/// adding a handler fails the test until every page here lists it.
///
/// The obligation is scoped to the roster itself, not to the page.
/// Every page below marks its roster with [`ROSTER_BEGIN`] /
/// [`ROSTER_END`], and the test reads only what sits between them —
/// otherwise a page that drops a handler from its table would still
/// pass on the strength of the name appearing in unrelated prose,
/// which is precisely the drift the test exists to catch.
///
/// A page whose roster is replaced by a link to the generated table
/// should come off this list, and its markers should go with it.
pub const CONTEXTUAL_ROSTERS: &[&str] = &[
    "README.md",
    "docs/dev/handlers.lex",
    "docs/reference/handlers.lex",
    "docs/user/handlers.lex",
    "docs/user/configuration.lex",
    "docs/user/handlers/mappings.lex",
    "docs/user/handlers/execution-order.lex",
    "skills/using-dodot/SKILL.md",
    "skills/using-dodot/HANDLERS.md",
];

/// The documentation-facing prose for one handler.
///
/// One row per registered handler; the test
/// `catalog_covers_the_registry_exactly` holds the two sets equal.
pub struct HandlerDoc {
    /// The registry name, e.g. `"symlink"`.
    pub handler: &'static str,

    /// What the handler does with a file it claims, in one line and
    /// in the present tense — the *Effect* column.
    pub effect: &'static str,

    /// How to describe what this handler claims when its matches do
    /// not come from a `[mappings]` pattern list.
    ///
    /// `None` means the *Claims by default* cell is rendered from the
    /// default rules, which is the case for every handler whose
    /// matches come from `[mappings]`. `Some` covers the two that
    /// cannot: `gate`, whose matches the scanner mints from host
    /// facts, and `symlink`, whose single `*` rule reads better as
    /// prose.
    pub claims: Option<&'static str>,
}

/// Every registered handler, with the prose the tables need.
///
/// Order is irrelevant — rendering sorts by phase and then by name.
pub const HANDLER_CATALOG: &[HandlerDoc] = &[
    HandlerDoc {
        handler: HANDLER_IGNORE,
        effect: "Drops the file silently — nothing runs, nothing is listed",
        claims: None,
    },
    HandlerDoc {
        handler: HANDLER_SKIP,
        effect: "Drops the file visibly — listed in `dodot status` as `skipped`",
        claims: None,
    },
    HandlerDoc {
        handler: HANDLER_GATE,
        effect: "Drops a file whose gate does not hold on this host, listed as `gated out`",
        claims: Some("Nothing by rule — the scanner mints gate matches from `._<label>` names, `_<label>/` directories, and `[mappings.gates]`"),
    },
    HandlerDoc {
        handler: HANDLER_EXTERNAL,
        effect: "Fetches each declared resource, and re-fetches when its upstream signature moves",
        claims: None,
    },
    HandlerDoc {
        handler: HANDLER_HOMEBREW,
        effect: "Runs `brew bundle` once, then holds an edited `Brewfile` at `older version` until `--provision-rerun`",
        claims: None,
    },
    HandlerDoc {
        handler: HANDLER_NIX,
        effect: "Runs `nix profile install` once, then holds an edited manifest at `older version` until `--provision-rerun`",
        claims: None,
    },
    HandlerDoc {
        handler: HANDLER_INSTALL,
        effect: "Runs the script once, then holds an edited script at `older version` until `--provision-rerun`",
        claims: None,
    },
    HandlerDoc {
        handler: HANDLER_PATH,
        effect: "Stages the directory onto `$PATH`",
        claims: None,
    },
    HandlerDoc {
        handler: HANDLER_SHELL,
        effect: "Sources the file at shell startup",
        claims: None,
    },
    HandlerDoc {
        handler: HANDLER_SYMLINK,
        effect: "Links the entry into `$HOME` or `$XDG_CONFIG_HOME`",
        claims: Some("Anything no other handler claimed (catchall)"),
    },
];

/// Look up a handler's documentation row.
pub fn doc_for(handler: &str) -> Option<&'static HandlerDoc> {
    HANDLER_CATALOG.iter().find(|d| d.handler == handler)
}

/// One rendered row: a handler as the registry and the default config
/// describe it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HandlerRow {
    /// Registry name.
    pub handler: String,
    /// The phase the handler runs in.
    pub phase: ExecutionPhase,
    /// Configuration or code execution, derived from the phase.
    pub category: HandlerCategory,
    /// Precise or catchall.
    pub match_mode: MatchMode,
    /// Exclusive or shared.
    pub scope: HandlerScope,
    /// What the handler claims with no user configuration, as prose
    /// for the roster table.
    pub claims: String,
    /// The patterns the default rules route to this handler, empty
    /// when it receives no default rule.
    pub default_patterns: Vec<String>,
    /// The priority of the default rules that route to it, if any.
    pub priority: Option<i32>,
    /// The one-line effect from [`HANDLER_CATALOG`].
    pub effect: &'static str,
}

/// Every registered handler, in execution order.
///
/// Sorted by phase and then by name: phase order is the order the
/// handlers actually run in, and the by-name tiebreak only makes the
/// rendering deterministic. It is *not* a claim about the order two
/// handlers sharing a phase run in — nothing pins that today.
///
/// # Panics
///
/// If a registered handler has no [`HANDLER_CATALOG`] row. The test
/// in this module catches that at development time; the panic is the
/// backstop for a caller that reaches this at runtime.
pub fn handler_rows(fs: &dyn Fs, mappings: &MappingsSection) -> Vec<HandlerRow> {
    let registry = create_registry(fs);
    let defaults = default_rule_facts(mappings);

    let mut rows: Vec<HandlerRow> = registry
        .iter()
        .map(|(name, handler)| {
            let doc = doc_for(name)
                .unwrap_or_else(|| panic!("handler `{name}` is registered but has no HandlerDoc"));
            let (patterns, priority) = defaults
                .get(name.as_str())
                .cloned()
                .unwrap_or((Vec::new(), None));
            let claims = match doc.claims {
                Some(prose) => prose.to_string(),
                None if patterns.is_empty() => {
                    "Nothing by default — set the matching `[mappings]` key".to_string()
                }
                None => backticked(&patterns),
            };
            HandlerRow {
                handler: name.clone(),
                phase: handler.phase(),
                category: handler.category(),
                match_mode: handler.match_mode(),
                scope: handler.scope(),
                claims,
                default_patterns: patterns,
                priority,
                effect: doc.effect,
            }
        })
        .collect();

    rows.sort_by(|a, b| {
        a.phase
            .cmp(&b.phase)
            .then_with(|| a.handler.cmp(&b.handler))
    });
    rows
}

/// The patterns and rule priority per handler, read from the rules
/// `mappings` produces.
///
/// Callers pass the *default* mappings, so a change to the shipped
/// defaults shows up in the docs without anyone retyping it.
fn default_rule_facts(
    mappings: &MappingsSection,
) -> BTreeMap<&'static str, (Vec<String>, Option<i32>)> {
    let rules = mappings_to_rules(mappings);
    let mut facts: BTreeMap<&'static str, (Vec<String>, Option<i32>)> = BTreeMap::new();

    for rule in rules {
        let name = match rule.handler.as_str() {
            HANDLER_IGNORE => HANDLER_IGNORE,
            HANDLER_SKIP => HANDLER_SKIP,
            HANDLER_GATE => HANDLER_GATE,
            HANDLER_EXTERNAL => HANDLER_EXTERNAL,
            HANDLER_HOMEBREW => HANDLER_HOMEBREW,
            HANDLER_NIX => HANDLER_NIX,
            HANDLER_INSTALL => HANDLER_INSTALL,
            HANDLER_PATH => HANDLER_PATH,
            HANDLER_SHELL => HANDLER_SHELL,
            HANDLER_SYMLINK => HANDLER_SYMLINK,
            other => panic!("default rules route to unknown handler `{other}`"),
        };
        let entry = facts.entry(name).or_insert((Vec::new(), None));
        entry.0.push(rule.pattern.clone());
        entry.1 = Some(rule.priority);
    }

    facts
}

/// The phases in execution order, with the handlers each one holds.
pub fn phase_rows(fs: &dyn Fs, mappings: &MappingsSection) -> Vec<(ExecutionPhase, Vec<String>)> {
    let mut phases: Vec<(ExecutionPhase, Vec<String>)> = Vec::new();
    for row in handler_rows(fs, mappings) {
        match phases.last_mut() {
            Some((phase, handlers)) if *phase == row.phase => handlers.push(row.handler),
            _ => phases.push((row.phase, vec![row.handler])),
        }
    }
    phases
}

/// The generated `docs/reference/handler-registry.lex`.
///
/// Everything factual comes from the registry and the default
/// mappings; the prose columns come from [`HANDLER_CATALOG`]. The
/// output is a complete lex document, including the "do not edit"
/// banner, and is byte-compared against the checked-in file by
/// `handler_registry_doc_is_current`.
pub fn render_registry_doc(fs: &dyn Fs, mappings: &MappingsSection) -> String {
    let rows = handler_rows(fs, mappings);
    let mut out = String::new();

    out.push_str("Handler Registry\n\n");
    out.push_str(&indent(
        1,
        &format!(
            "*Generated from the handler registry — do not edit.* Run `pixi run gen-docs` to \
             regenerate this page from `{}`; the test suite fails when it drifts. This is the \
             roster that cannot disagree with the shipped registry, so link here rather than \
             copying the tables. A few pages do keep a hand-written roster where naming the \
             handlers is part of explaining something else; they are listed in that same file, \
             each marks its roster with a `handler-roster` comment, and a test holds what is \
             inside those markers to naming every registered handler.",
            "crates/dodot-lib/src/handlers/catalog.rs"
        ),
    ));
    out.push('\n');
    out.push_str(&indent(
        1,
        "For what each handler is *for*, see [./handlers.lex]; for the per-handler user guides, \
         see [./../user/handlers.lex].",
    ));
    out.push('\n');

    out.push_str("1. Handlers\n\n");
    out.push_str(&indent(
        1,
        "Every handler dodot registers, in phase order. `Claims by default` is what the handler \
         matches when the user configures nothing.",
    ));
    out.push('\n');
    out.push_str(&indent(1, "Registered handlers:"));
    out.push('\n');
    let handler_table: Vec<Vec<String>> = std::iter::once(vec![
        "Handler".to_string(),
        "Phase".to_string(),
        "Category".to_string(),
        "Match mode".to_string(),
        "Scope".to_string(),
        "Claims by default".to_string(),
        "Effect".to_string(),
    ])
    .chain(rows.iter().map(|row| {
        vec![
            format!("`{}`", row.handler),
            format!("`{}`", phase_name(row.phase)),
            format!("`{}`", category_name(row.category)),
            match_mode_name(row.match_mode).to_string(),
            scope_name(row.scope).to_string(),
            row.claims.clone(),
            row.effect.to_string(),
        ]
    }))
    .collect();
    out.push_str(&render_table(2, &handler_table, "lllllll"));
    out.push('\n');

    out.push_str("2. Phases\n\n");
    out.push_str(&indent(
        1,
        "Phases run in the order below — the order the `ExecutionPhase` variants are declared in. \
         A phase may hold more than one handler, and the order two handlers sharing a phase run \
         in is not pinned down; nothing in dodot depends on it.",
    ));
    out.push('\n');
    out.push_str(&indent(1, "Execution phases:"));
    out.push('\n');
    let phase_table: Vec<Vec<String>> = std::iter::once(vec![
        "Phase".to_string(),
        "Handlers".to_string(),
        "Category".to_string(),
    ])
    .chain(
        phase_rows(fs, mappings)
            .into_iter()
            .map(|(phase, handlers)| {
                vec![
                    format!("`{}`", phase_name(phase)),
                    handlers
                        .iter()
                        .map(|h| format!("`{h}`"))
                        .collect::<Vec<_>>()
                        .join(", "),
                    format!("`{}`", category_name(phase.category())),
                ]
            }),
    )
    .collect();
    out.push_str(&render_table(2, &phase_table, "lll"));
    out.push('\n');

    out.push_str("3. Default Mappings\n\n");
    out.push_str(&indent(
        1,
        "The rules `[mappings]` produces with no user configuration. Higher priority wins: a \
         `README.md` is claimed by `skip` at 50 rather than by `symlink` at 0. The handlers with \
         no row here receive no default rule — `gate` matches are minted by the scanner, and \
         `ignore` waits for a pattern you set.",
    ));
    out.push('\n');
    out.push_str(&indent(1, "Default rules, highest priority first:"));
    out.push('\n');
    let mut mapping_rows: Vec<&HandlerRow> = rows.iter().filter(|r| r.priority.is_some()).collect();
    mapping_rows.sort_by(|a, b| {
        b.priority
            .cmp(&a.priority)
            .then_with(|| a.handler.cmp(&b.handler))
    });
    let mapping_table: Vec<Vec<String>> = std::iter::once(vec![
        "Priority".to_string(),
        "Handler".to_string(),
        "Patterns".to_string(),
    ])
    .chain(mapping_rows.iter().map(|row| {
        vec![
            row.priority.expect("filtered to Some").to_string(),
            format!("`{}`", row.handler),
            backticked(&row.default_patterns),
        ]
    }))
    .collect();
    out.push_str(&render_table(2, &mapping_table, "rll"));

    out
}

/// Whether a line is a roster marker, and which one.
///
/// A marker is a line that is *only* a marker comment, in one of the
/// two forms the documentation uses: `<!-- handler-roster:begin -->`
/// in Markdown, `:: handler-roster:begin ::` in lex. Matching the
/// whole line rather than searching for the token is what lets a
/// document *describe* the markers — as the handler-authoring guide
/// does — without the prose being mistaken for one.
fn marker_kind(line: &str) -> Option<Marker> {
    let trimmed = line.trim();
    let inner = trimmed
        .strip_prefix("<!--")
        .and_then(|rest| rest.strip_suffix("-->"))
        .or_else(|| {
            trimmed
                .strip_prefix("::")
                .and_then(|rest| rest.strip_suffix("::"))
        })?
        .trim();

    match inner {
        _ if inner == ROSTER_BEGIN => Some(Marker::Begin),
        _ if inner == ROSTER_END => Some(Marker::End),
        _ => None,
    }
}

/// Which end of a roster region a marker line opens or closes.
enum Marker {
    Begin,
    End,
}

/// Every marked roster region in `text`, concatenated.
///
/// A region runs from a [`ROSTER_BEGIN`] marker line to the next
/// [`ROSTER_END`] marker line; the marker lines themselves are not
/// part of it. A page may mark more than one region — several of them
/// name the handlers with no default rule separately from the table
/// of the ones that have one.
///
/// # Errors
///
/// When the page carries no region at all, or a marker is unbalanced.
/// Both are the page having drifted away from the contract rather
/// than a handler being missing, so they are reported apart from it.
pub fn roster_regions(text: &str) -> Result<String, String> {
    let mut out = String::new();
    let mut depth = 0usize;
    let mut regions = 0usize;

    for (number, line) in text.lines().enumerate() {
        let marker = marker_kind(line);
        let begins = matches!(marker, Some(Marker::Begin));
        let ends = matches!(marker, Some(Marker::End));
        if begins {
            if depth > 0 {
                return Err(format!(
                    "line {}: a roster region opens inside another",
                    number + 1
                ));
            }
            depth = 1;
            regions += 1;
            continue;
        }
        if ends {
            if depth == 0 {
                return Err(format!(
                    "line {}: a roster region closes without opening",
                    number + 1
                ));
            }
            depth = 0;
            continue;
        }
        if depth > 0 {
            out.push_str(line);
            out.push('\n');
        }
    }

    if depth != 0 {
        return Err("a roster region opens and never closes".to_string());
    }
    if regions == 0 {
        return Err(format!(
            "no roster region found — mark it with `{ROSTER_BEGIN}` / `{ROSTER_END}`"
        ));
    }
    Ok(out)
}

/// Patterns as a comma-separated list of inline-code cells.
fn backticked(patterns: &[String]) -> String {
    patterns
        .iter()
        .map(|pattern| format!("`{pattern}`"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// The `ExecutionPhase` variant name, as written in the source.
fn phase_name(phase: ExecutionPhase) -> &'static str {
    match phase {
        ExecutionPhase::Filter => "Filter",
        ExecutionPhase::External => "External",
        ExecutionPhase::Provision => "Provision",
        ExecutionPhase::Setup => "Setup",
        ExecutionPhase::PathExport => "PathExport",
        ExecutionPhase::ShellInit => "ShellInit",
        ExecutionPhase::Link => "Link",
    }
}

/// The `HandlerCategory` variant name, as written in the source.
fn category_name(category: HandlerCategory) -> &'static str {
    match category {
        HandlerCategory::Configuration => "Configuration",
        HandlerCategory::CodeExecution => "CodeExecution",
    }
}

/// The `MatchMode` variant name, as written in the source.
fn match_mode_name(mode: MatchMode) -> &'static str {
    match mode {
        MatchMode::Precise => "Precise",
        MatchMode::Catchall => "Catchall",
    }
}

/// The `HandlerScope` variant name, as written in the source.
fn scope_name(scope: HandlerScope) -> &'static str {
    match scope {
        HandlerScope::Exclusive => "Exclusive",
        HandlerScope::Shared => "Shared",
    }
}

/// A paragraph at `level` (four spaces per level), with a trailing
/// newline.
fn indent(level: usize, text: &str) -> String {
    format!("{}{text}\n", "    ".repeat(level))
}

/// A lex table: pipe rows at `level`, column-aligned, followed by the
/// `:: table ::` annotation that marks the block as tabular.
fn render_table(level: usize, rows: &[Vec<String>], align: &str) -> String {
    let columns = rows.iter().map(Vec::len).max().unwrap_or(0);
    let widths: Vec<usize> = (0..columns)
        .map(|column| {
            rows.iter()
                .filter_map(|row| row.get(column))
                .map(|cell| cell.chars().count())
                .max()
                .unwrap_or(0)
        })
        .collect();

    let pad = "    ".repeat(level);
    let mut out = String::new();
    for row in rows {
        out.push_str(&pad);
        out.push('|');
        for (column, width) in widths.iter().enumerate() {
            let cell = row.get(column).map(String::as_str).unwrap_or("");
            let filler = width.saturating_sub(cell.chars().count());
            out.push(' ');
            out.push_str(cell);
            out.push_str(&" ".repeat(filler));
            out.push_str(" |");
        }
        out.push('\n');
    }
    out.push('\n');
    out.push_str(&"    ".repeat(level.saturating_sub(1)));
    out.push_str(&format!(":: table align={align} ::\n"));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ConfigManager;
    use crate::fs::OsFs;
    use crate::testing::TempEnvironment;
    use std::collections::BTreeSet;
    use std::path::PathBuf;

    /// The shipped `[mappings]` defaults, resolved the way dodot
    /// resolves them for a dotfiles root with no `.dodot.toml`.
    fn default_mappings(env: &TempEnvironment) -> MappingsSection {
        ConfigManager::new(&env.dotfiles_root)
            .expect("build a config resolver")
            .root_config()
            .expect("resolve the default config")
            .mappings
    }

    /// The repository root, from this crate's manifest directory.
    fn repo_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|crates| crates.parent())
            .expect("crates/dodot-lib sits two levels below the repo root")
            .to_path_buf()
    }

    #[test]
    fn catalog_covers_the_registry_exactly() {
        let fs = OsFs::new();
        let registered: BTreeSet<String> = create_registry(&fs).keys().cloned().collect();
        let documented: BTreeSet<String> = HANDLER_CATALOG
            .iter()
            .map(|doc| doc.handler.to_string())
            .collect();

        assert_eq!(
            registered, documented,
            "every registered handler needs a HandlerDoc row in HANDLER_CATALOG (and vice versa) \
             — add the row, then run `pixi run gen-docs`"
        );
    }

    #[test]
    fn rows_are_in_phase_order() {
        let env = TempEnvironment::builder().build();
        let fs = OsFs::new();
        let rows = handler_rows(&fs, &default_mappings(&env));
        let phases: Vec<ExecutionPhase> = rows.iter().map(|row| row.phase).collect();
        let mut sorted = phases.clone();
        sorted.sort();
        assert_eq!(phases, sorted, "rows must come out in execution order");
    }

    #[test]
    fn every_phase_that_holds_handlers_is_listed_once() {
        let env = TempEnvironment::builder().build();
        let fs = OsFs::new();
        let phases = phase_rows(&fs, &default_mappings(&env));
        let distinct: BTreeSet<String> = phases
            .iter()
            .map(|(phase, _)| phase_name(*phase).to_string())
            .collect();
        assert_eq!(
            distinct.len(),
            phases.len(),
            "a phase must be grouped into one row, not several"
        );
        assert!(
            phases.iter().any(|(_, handlers)| handlers.len() > 1),
            "the phase table has to be able to show a phase with more than one handler"
        );
    }

    /// The generated page must match what the registry renders.
    ///
    /// Set `DODOT_UPDATE_DOCS=1` (or run `pixi run gen-docs`) to
    /// rewrite the page instead of failing.
    #[test]
    fn handler_registry_doc_is_current() {
        let env = TempEnvironment::builder().build();
        let fs = OsFs::new();
        let rendered = render_registry_doc(&fs, &default_mappings(&env));
        let path = repo_root().join(REGISTRY_DOC_PATH);

        if std::env::var_os("DODOT_UPDATE_DOCS").is_some() {
            std::fs::write(&path, &rendered).expect("write the generated handler registry page");
            return;
        }

        let checked_in = std::fs::read_to_string(&path)
            .unwrap_or_else(|err| panic!("read {}: {err}", path.display()));
        assert_eq!(
            checked_in, rendered,
            "{REGISTRY_DOC_PATH} is out of date with the handler registry — run `pixi run gen-docs`"
        );
    }

    /// Every roster in [`CONTEXTUAL_ROSTERS`] lists every registered
    /// handler.
    ///
    /// These pages keep a hand-written roster on purpose (see the
    /// module docs), so the generated page cannot cover them. What
    /// this test covers is the drift that actually happened: `nix`,
    /// `external`, and `gate` were added to the registry and left out
    /// of roster after roster. A brand-new handler fails here until
    /// each page has been told about it.
    ///
    /// The check reads only what sits between the roster markers, not
    /// the whole page. That distinction is the test: names like
    /// `path`, `shell`, and `gate` occur throughout ordinary prose, so
    /// a page-wide search would stay green even after a handler was
    /// deleted from the table it belongs in.
    ///
    /// It still cannot see that a roster put a handler in the wrong
    /// phase or gave it the wrong priority — presence is all it
    /// checks. The generated page remains the only copy that cannot
    /// drift in its details.
    #[test]
    fn contextual_rosters_list_every_handler() {
        let fs = OsFs::new();
        let registered: Vec<String> = create_registry(&fs).keys().cloned().collect();
        let root = repo_root();

        let mut problems: Vec<String> = Vec::new();
        for page in CONTEXTUAL_ROSTERS {
            let path = root.join(page);
            let text = std::fs::read_to_string(&path)
                .unwrap_or_else(|err| panic!("read {}: {err}", path.display()));

            let roster = match roster_regions(&text) {
                Ok(roster) => roster,
                Err(reason) => {
                    problems.push(format!("{page}: {reason}"));
                    continue;
                }
            };

            let absent: Vec<&str> = registered
                .iter()
                .filter(|handler| !mentions_word(&roster, handler))
                .map(String::as_str)
                .collect();
            if !absent.is_empty() {
                problems.push(format!("{page}: roster omits {}", absent.join(", ")));
            }
        }

        assert!(
            problems.is_empty(),
            "each of these pages keeps a hand-written handler roster between \
             `{ROSTER_BEGIN}` / `{ROSTER_END}` markers, and the roster has to name every \
             registered handler — add it there (a mention elsewhere on the page does not \
             count), or drop the page from CONTEXTUAL_ROSTERS if its roster is gone:\n  {}",
            problems.join("\n  ")
        );
    }

    /// A name outside the markers does not satisfy the roster check.
    ///
    /// This is the property the page-wide version of this test lacked:
    /// `gate` below is named twice in prose and never in the roster,
    /// which has to read as missing.
    #[test]
    fn a_mention_outside_the_roster_does_not_count() {
        let page = "\
Some prose about the gate handler and how gating works.

<!-- handler-roster:begin -->
| `symlink` | links |
| `shell`   | sources |
<!-- handler-roster:end -->

More prose, mentioning gate again for good measure.
";
        let roster = roster_regions(page).expect("the fixture marks one region");

        assert!(mentions_word(&roster, "symlink"));
        assert!(mentions_word(&roster, "shell"));
        assert!(
            !mentions_word(&roster, "gate"),
            "`gate` appears only outside the markers, so it must not count as listed"
        );
        assert!(
            mentions_word(page, "gate"),
            "the fixture is only meaningful if a page-wide search WOULD have found it"
        );
    }

    /// Several regions on one page are concatenated.
    #[test]
    fn roster_regions_accumulates_every_marked_block() {
        let page = "\
:: handler-roster:begin ::
`homebrew`
:: handler-roster:end ::

filler

:: handler-roster:begin ::
`gate`
:: handler-roster:end ::
";
        let roster = roster_regions(page).expect("two regions");
        assert!(mentions_word(&roster, "homebrew"));
        assert!(mentions_word(&roster, "gate"));
        assert!(!roster.contains("filler"));
    }

    /// Prose that *names* the markers is not a marker.
    ///
    /// The handler-authoring guide documents the marker syntax, so its
    /// text carries both tokens on one line. Matching whole lines
    /// rather than searching for the token is what keeps that from
    /// opening a bogus region.
    #[test]
    fn prose_describing_the_markers_is_not_a_marker() {
        let page = "\
Mark it with a `handler-roster:begin` / `handler-roster:end` comment.

<!-- handler-roster:begin -->
`symlink`
<!-- handler-roster:end -->
";
        let roster = roster_regions(page).expect("exactly one real region");
        assert!(mentions_word(&roster, "symlink"));
        assert!(
            !roster.contains("Mark it with"),
            "the sentence describing the markers must stay outside the region"
        );
    }

    /// A page that lost its markers is reported as such, not as a
    /// page that dropped every handler at once.
    #[test]
    fn roster_regions_reports_missing_and_unbalanced_markers() {
        let err = roster_regions("no markers here").expect_err("no region");
        assert!(err.contains("no roster region found"), "{err}");

        let err = roster_regions("<!-- handler-roster:begin -->\nrow\n").expect_err("unclosed");
        assert!(err.contains("never closes"), "{err}");

        let err = roster_regions("<!-- handler-roster:end -->\n").expect_err("unopened");
        assert!(err.contains("without opening"), "{err}");
    }

    /// Whether `text` names `word` as a word of its own.
    ///
    /// A hit is disqualified by a preceding alphanumeric, `_`, `-`,
    /// `.`, or `/` — so `packages.nix` is not a mention of the `nix`
    /// handler and `handlers/path.lex` is not a mention of `path` —
    /// and by a trailing alphanumeric, `_`, or `-`, so
    /// `externals.toml` is not a mention of `external`. A trailing
    /// `.` is allowed, since a name can end a sentence.
    fn mentions_word(text: &str, word: &str) -> bool {
        text.match_indices(word).any(|(start, _)| {
            let before_ok = text[..start]
                .chars()
                .next_back()
                .is_none_or(|c| !c.is_alphanumeric() && !matches!(c, '_' | '-' | '.' | '/'));
            let after_ok = text[start + word.len()..]
                .chars()
                .next()
                .is_none_or(|c| !c.is_alphanumeric() && !matches!(c, '_' | '-'));
            before_ok && after_ok
        })
    }
}
