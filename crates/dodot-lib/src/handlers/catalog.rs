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
//! So the tables are no longer written by hand. [`render_registry_doc`]
//! renders `docs/reference/handler-registry.lex` from
//! [`create_registry`](crate::handlers::create_registry) plus
//! [`mappings_to_rules`](crate::config::mappings_to_rules) over the
//! default [`MappingsSection`](crate::config::MappingsSection), and a
//! test in this module fails when the checked-in file drifts from
//! what the registry would render. Every other document links to that
//! page instead of restating it.
//!
//! What a new handler therefore owes the docs is one [`HandlerDoc`]
//! row here (the test refuses a registry entry with no row, and a row
//! with no registry entry) and a `pixi run gen-docs` to regenerate the
//! page. Everything else in the tables comes from the code the
//! handler already had to write.
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
        effect: "Runs `brew bundle` once per content hash",
        claims: None,
    },
    HandlerDoc {
        handler: HANDLER_NIX,
        effect: "Runs `nix profile install` once per content hash",
        claims: None,
    },
    HandlerDoc {
        handler: HANDLER_INSTALL,
        effect: "Runs the script once per content hash",
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
             regenerate this page from `{}`; the test suite fails when it drifts. Every other \
             document links here rather than restating the tables.",
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
}
