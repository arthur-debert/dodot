//! The orientation inventory shown before a user approves an implicit root.
//!
//! Its job is recognition, not prediction: root-wide counts per configured
//! handler category plus at most [`MAX_INVENTORY_PATHS`] example paths across
//! the whole prompt, prioritized so shell and code-execution entries — the
//! ones that can change every future shell — are the ones the user actually
//! sees. Omitted paths are counted, never silently dropped.
//!
//! Explicitly *not* a dry run: building this reads configuration and routing
//! metadata only. It never renders templates, resolves secrets, or inspects
//! candidate pack-entry contents (Spec, "Non-Goals"), which is also why a
//! first-ever run needs no rendered baseline.
//!
//! What that costs in precision is deliberate. A file is classified by the
//! handler the rules route it to, under the name it carries on disk: a
//! template appears as its source name because rendering it is exactly the
//! work this refuses to do. The prompt is asking "is this the right
//! directory?", and a source name answers that better than a rendered one
//! would.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use crate::config::{mappings_to_rules, ConfigManager};
use crate::fs::Fs;
use crate::gates::{pack_os_active, GateTable, HostFacts};
use crate::handlers::{create_registry, ExecutionPhase};
use crate::packs::scan_packs;
use crate::rules::{RuleMatch, Scanner};

use super::error::{Result, SafetyLockError};
use super::roots::ResolvedRoot;

/// The per-directory configuration file both the root and each pack may carry.
const DOTFILES_CONFIG_FILE: &str = ".dodot.toml";

/// The most paths the prompt may show, across all categories combined.
///
/// A cap, not a per-category quota: the point is that the confirmation
/// question stays on screen (Spec, story 3).
pub const MAX_INVENTORY_PATHS: usize = 10;

/// A configured-handler category, in prompt priority order.
///
/// The derived [`Ord`] *is* the priority: shell and code execution first
/// because they run or alter program state, then the categories that alter
/// path resolution and user-visible files. Sorting entries by
/// `(category, path)` therefore produces the prompt's order directly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum InventoryCategory {
    /// Shell startup files added to every future shell session.
    Shell,
    /// Install scripts, package manifests, and other provisioning inputs.
    CodeExecution,
    /// Directories added to `PATH`.
    Path,
    /// External handlers.
    External,
    /// Symlinked files.
    Link,
    /// Everything else the configured handlers recognize.
    Other,
}

impl InventoryCategory {
    /// The categories in the order the prompt lists them.
    pub const PROMPT_ORDER: [InventoryCategory; 6] = [
        InventoryCategory::Shell,
        InventoryCategory::CodeExecution,
        InventoryCategory::Path,
        InventoryCategory::External,
        InventoryCategory::Link,
        InventoryCategory::Other,
    ];

    /// The category a handler in `phase` contributes to, or `None` when it
    /// contributes nothing.
    ///
    /// Derived from the handler's declared phase rather than a table of
    /// handler names, so a handler added later is classified by what it does
    /// instead of by whether someone remembered to extend this module.
    ///
    /// [`Filter`](ExecutionPhase::Filter) is the `None`: `ignore`, `skip`, and
    /// `gate` claim a file precisely so that nothing is deployed from it, and
    /// counting those files would inflate every number the prompt shows with
    /// entries approving the root cannot act on.
    pub fn for_phase(phase: ExecutionPhase) -> Option<Self> {
        match phase {
            ExecutionPhase::Filter => None,
            ExecutionPhase::ShellInit => Some(InventoryCategory::Shell),
            ExecutionPhase::Provision | ExecutionPhase::Setup => {
                Some(InventoryCategory::CodeExecution)
            }
            ExecutionPhase::PathExport => Some(InventoryCategory::Path),
            ExecutionPhase::External => Some(InventoryCategory::External),
            ExecutionPhase::Link => Some(InventoryCategory::Link),
        }
    }

    /// Stable, user-facing name of the category.
    pub fn label(self) -> &'static str {
        match self {
            InventoryCategory::Shell => "shell",
            InventoryCategory::CodeExecution => "code execution",
            InventoryCategory::Path => "path",
            InventoryCategory::External => "external",
            InventoryCategory::Link => "link",
            InventoryCategory::Other => "other",
        }
    }
}

/// One example path shown in the prompt.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct InventoryEntry {
    /// The category this file was recognized as. First sort key, so
    /// high-impact entries survive the path cap.
    pub category: InventoryCategory,
    /// The file's path relative to the root being approved — the prompt names
    /// the root once and stays readable.
    pub relative_path: PathBuf,
}

/// How many files a category recognized, root-wide.
///
/// Counts are unaffected by the path cap: they stay visible even when their
/// example paths were omitted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CategoryCount {
    pub category: InventoryCategory,
    pub files: usize,
}

/// The bounded orientation inventory for one root.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RootInventory {
    /// Root-wide counts per category, in [`InventoryCategory::PROMPT_ORDER`].
    ///
    /// Categories that recognized nothing are absent rather than present with
    /// a zero: the prompt is a recognition aid, and a column of zeroes is
    /// noise between the user and the question being asked.
    pub counts: Vec<CategoryCount>,
    /// At most [`MAX_INVENTORY_PATHS`] example paths, category-priority first.
    pub sample: Vec<InventoryEntry>,
    /// How many recognized files are not in `sample`.
    pub omitted: usize,
}

impl RootInventory {
    /// Total recognized files across all categories.
    pub fn total_files(&self) -> usize {
        self.counts.iter().map(|count| count.files).sum()
    }
}

/// Build the orientation inventory for the root about to be approved.
///
/// Root-wide by construction: there is no pack filter to pass. A command that
/// operates on one pack still approves the whole path, so an inventory scoped
/// to the filter would describe less than the answer authorizes (Spec,
/// "Risks"). Every active pack under the root is walked.
///
/// The walk is the ordinary routing pipeline and nothing more: discover packs,
/// load each pack's merged configuration, generate its rules, and ask the
/// scanner which handler claims each top-level entry. No handler is
/// constructed to produce intents, no template is rendered, no secret is
/// resolved, and no candidate file's contents are read — the registry is
/// consulted only for each handler's declared phase. A directory entry counts
/// once, as the one thing routing recognized; the scanner does not descend
/// into it and neither does this.
///
/// `fs` and `host` are injected for the same reason every other Safety Lock
/// entry point takes its collaborators: this module captures no process state
/// of its own, so gating decisions stay stateable in a test rather than
/// depending on the machine the suite runs on. The injection is not total, and
/// callers should not read it as a virtualized filesystem: the pack walk,
/// routing, and registry go through `fs`, but configuration is loaded by
/// [`ConfigManager`], which reads `.dodot.toml` through `std::fs` and searches
/// ancestors for a `.git` marker. A test that varies configuration therefore
/// has to write real files, as this module's own tests do.
///
/// Fails when a `.dodot.toml` cannot be loaded
/// ([`DotfilesConfigUnusable`](SafetyLockError::DotfilesConfigUnusable)) or a
/// pack cannot be walked or routed
/// ([`PackRoutingUnusable`](SafetyLockError::PackRoutingUnusable)), each
/// naming the file or directory at fault. Both fail the inventory rather than
/// omitting the unreadable part, because the prompt's counts are the user's
/// only evidence about the root: an inventory that quietly skipped what it
/// could not read would understate what approval permits.
pub fn build_inventory(
    root: &ResolvedRoot,
    fs: &dyn Fs,
    host: &HostFacts,
) -> Result<RootInventory> {
    let root_path = root.as_path();
    let config = ConfigManager::new(root_path)
        .and_then(|manager| manager.root_config().map(|config| (manager, config)));
    let (manager, root_config) =
        config.map_err(|error| unusable_config(root_path.join(DOTFILES_CONFIG_FILE), error))?;

    let discovered = scan_packs(fs, root_path, &root_config.pack.ignore)
        .map_err(|error| unusable_routing(root_path, error))?;

    let registry = create_registry(fs);
    let scanner = Scanner::new(fs);

    let mut counts: BTreeMap<InventoryCategory, usize> = BTreeMap::new();
    let mut entries: Vec<InventoryEntry> = Vec::new();

    for pack in &discovered.packs {
        let pack_config = manager
            .config_for_pack(&pack.path)
            .map_err(|error| unusable_config(pack.path.join(DOTFILES_CONFIG_FILE), error))?;

        // A pack gated off this host deploys nothing here, so counting it
        // would describe a machine the user is not on.
        if !pack_os_active(&pack_config.pack.os, host) {
            continue;
        }

        let owner = |declared| {
            gate_configuration_owner(root_path, pack, &pack_config, &root_config, declared)
        };

        // Each failure domain is attributed from its own evidence: a
        // `merge_user` failure is always a `[gates]` entry, and a `scan_pack`
        // failure never is, because the gate table it uses was already merged
        // above. Reading one domain's verdict for the other's failure would
        // send the user to a file that is not the one that failed.
        let mut gates = GateTable::with_builtins();
        if !pack_config.gates.is_empty() {
            gates.merge_user(&pack_config.gates).map_err(|error| {
                unusable_config(
                    owner(broken_gate_entry_owner(&root_config, &pack_config)),
                    error,
                )
            })?;
        }

        let rules = mappings_to_rules(&pack_config.mappings);
        let matches = scanner
            .scan_pack(
                pack,
                &rules,
                &pack_config.pack.ignore,
                &gates,
                host,
                &pack_config.mappings.gates,
            )
            .map_err(|error| {
                let declared = broken_mapping_entry_owner(&root_config, &pack_config);
                unusable_scan(&pack.path, &owner(declared), error)
            })?;

        for matched in matches {
            let Some(category) = category_of(&matched.handler, &registry) else {
                continue;
            };

            *counts.entry(category).or_default() += 1;
            entries.push(InventoryEntry {
                category,
                // Pack-relative becomes root-relative: the prompt names the
                // root once, and the pack directory is part of what the user
                // is being asked to recognize.
                relative_path: Path::new(&pack.name)
                    .join(source_relative_path(&pack.path, &matched)),
            });
        }
    }

    // `(category, relative_path)`, which is the prompt's order — see
    // `InventoryCategory`. Sorting before the cap is what makes the cap
    // keep the highest-impact entries rather than the first ones walked.
    entries.sort();
    let recognized = entries.len();
    entries.truncate(MAX_INVENTORY_PATHS);

    Ok(RootInventory {
        counts: InventoryCategory::PROMPT_ORDER
            .iter()
            .filter_map(|category| {
                counts.get(category).map(|&files| CategoryCount {
                    category: *category,
                    files,
                })
            })
            .collect(),
        omitted: recognized - entries.len(),
        sample: entries,
    })
}

/// The entry's path as the root actually spells it, relative to its pack.
///
/// [`RuleMatch::relative_path`] is the *effective* path routing works from,
/// not the one on disk: a passing basename gate presents
/// `aliases._darwin.sh` under its stripped name `aliases.sh`, and a passing
/// directory gate lifts `_darwin/aliases.sh` to `aliases.sh` at the pack root.
/// That rewrite is right for handlers and wrong for this prompt, which asks
/// the user to recognize their own root by the names it carries — a path that
/// is not in the directory is evidence about a directory that does not exist.
/// The absolute path is never rewritten, so the displayed path is derived from
/// it; classification still uses the effective match.
fn source_relative_path<'a>(pack_path: &Path, matched: &'a RuleMatch) -> &'a Path {
    matched
        .absolute_path
        .strip_prefix(pack_path)
        .unwrap_or(matched.relative_path.as_path())
}

/// The inventory category a routed file contributes to, or `None` when it
/// contributes nothing.
///
/// A handler the registry does not know is [`Other`](InventoryCategory::Other)
/// rather than skipped: it was routed by the loaded configuration, so it is a
/// file approval would act on, and dropping it would undercount the root.
fn category_of(
    handler: &str,
    registry: &std::collections::HashMap<String, Box<dyn crate::handlers::Handler + '_>>,
) -> Option<InventoryCategory> {
    match registry.get(handler) {
        Some(handler) => InventoryCategory::for_phase(handler.phase()),
        None => Some(InventoryCategory::Other),
    }
}

fn unusable_config(config_file: PathBuf, error: crate::error::DodotError) -> SafetyLockError {
    SafetyLockError::DotfilesConfigUnusable {
        config_file,
        reason: error.to_string(),
    }
}

fn unusable_routing(
    directory: impl Into<PathBuf>,
    error: crate::error::DodotError,
) -> SafetyLockError {
    SafetyLockError::PackRoutingUnusable {
        directory: directory.into(),
        reason: error.to_string(),
    }
}

/// Which `.dodot.toml` declared the entry a gate failure is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GateLayer {
    Root,
    Pack,
}

/// Whether one `[gates]` entry stands up on its own.
///
/// Validated through [`GateTable::merge_user`] rather than a reimplementation
/// of it, so the question asked here cannot drift from the question the walk
/// asks.
fn gate_entry_is_valid(label: &str, dimensions: &HashMap<String, String>) -> bool {
    let one = HashMap::from([(label.to_string(), dimensions.clone())]);
    GateTable::with_builtins().merge_user(&one).is_ok()
}

/// The layer that declared the first invalid `[gates]` entry, if any.
///
/// Per-entry, because per-entry is where the provenance survives: labels merge
/// key by key, so the pack's resolved table holds every label either file
/// declared. For each broken one, the root owns it exactly when the root's own
/// version of that same label is broken too — which is also what makes the
/// deep merge safe to reason about. A pack that adds `os` to a root label
/// already carrying a bad dimension inherits the bad dimension, and the root's
/// version fails on its own, so the root is named; a pack that breaks a label
/// the root had valid is named itself.
///
/// Entries are visited in label order so a configuration with several broken
/// entries always reports the same one.
fn broken_gate_entry_owner(
    root_config: &crate::config::DodotConfig,
    pack_config: &crate::config::DodotConfig,
) -> Option<GateLayer> {
    let mut labels: Vec<&String> = pack_config.gates.keys().collect();
    labels.sort();

    labels.into_iter().find_map(|label| {
        let dimensions = pack_config.gates.get(label)?;
        if gate_entry_is_valid(label, dimensions) {
            return None;
        }

        let root_owns = root_config
            .gates
            .get(label)
            .is_some_and(|root_dimensions| !gate_entry_is_valid(label, root_dimensions));

        Some(if root_owns {
            GateLayer::Root
        } else {
            GateLayer::Pack
        })
    })
}

/// The layer that declared the `[mappings.gates]` glob the walk could not
/// compile, if that is why it stopped.
///
/// Deliberately narrower than "any entry that looks unusable". Glob
/// compilation is the one mapping check the walk runs *eagerly*, over the
/// whole table, before it looks at a single file — so if a glob does not
/// compile, that is necessarily what stopped the walk, and the entry naming it
/// is exact: this table is a flat glob-to-label map, so an entry belongs to
/// the pack precisely when the pack's value for that glob differs from the
/// root's.
///
/// Label resolution deliberately does *not* appear here, though an unresolved
/// label reads like a defect. The walk resolves a mapping's label only when
/// that mapping's glob matches an entry it actually met, so a mapping matching
/// nothing is accepted with a label nothing defines. Treating one as a defect
/// let it answer for a failure it had no part in — a filename gate token, say
/// — and point at the file that declared the dormant mapping instead of the
/// one the error is about. Which mapping, if any, participated in a given
/// failure is knowable only from the failure itself, which arrives as prose;
/// until it carries the entry, those failures take the scope fallback in
/// [`gate_configuration_owner`] rather than a guess.
///
/// Entries are visited in glob order, matching the order
/// [`compile_mapping_gates`](crate::gates::compile_mapping_gates) validates
/// them in, so the entry chosen here is the entry that actually failed.
fn broken_mapping_entry_owner(
    root_config: &crate::config::DodotConfig,
    pack_config: &crate::config::DodotConfig,
) -> Option<GateLayer> {
    let mut globs: Vec<&String> = pack_config.mappings.gates.keys().collect();
    globs.sort();

    globs.into_iter().find_map(|glob| {
        let label = pack_config.mappings.gates.get(glob)?;

        // One entry at a time through the shared compiler rather than
        // `glob::Pattern::new` directly. `compile_mapping_gates` exists so
        // that no two callers "disagree about which globs compile, in what
        // order, or with what failure mode" (its words), and this is a third
        // caller: the whole point of asking is to reach the same verdict the
        // walk reached. Calling the pattern API directly would be the cheaper
        // spelling of a check that is only useful while it stays identical.
        // Same reason `gate_entry_is_valid` goes through `merge_user`.
        let one = HashMap::from([(glob.clone(), label.clone())]);
        if crate::gates::compile_mapping_gates(&one, "<attribution>").is_ok() {
            return None;
        }

        Some(if root_config.mappings.gates.get(glob) == Some(label) {
            GateLayer::Root
        } else {
            GateLayer::Pack
        })
    })
}

/// The `.dodot.toml` to blame for gate configuration that fails during a walk.
///
/// A pack's configuration is the root's with the pack's merged over it, and the
/// merge keeps no per-key provenance, so a rejected gate setting does not say
/// which file wrote it. `declared` is that provenance recovered from the entry
/// that actually fails — [`broken_gate_entry_owner`] for a `[gates]` failure,
/// [`broken_mapping_entry_owner`] for a `[mappings.gates]` one. Deriving it
/// from the failing entry rather than from a summary of the root's health is
/// what keeps an unrelated defect in one file from pulling the blame off the
/// other: the two are independent, and either may be broken while the other
/// causes the failure at hand.
///
/// Some failures name no entry: a filename gate token referring to a label
/// nothing defines, and a gate-routing conflict, are both about a *file* the
/// walk met rather than a line either config wrote. `declared` is `None` there
/// and the fallback is scope — the pack when it contributed gate configuration
/// of its own, the root when it inherited all of it. That case matters most
/// for a pack with no `.dodot.toml`, where naming the pack would name a file
/// that is not on disk to open (story 14: the file named has to be the file
/// the user can act on).
///
/// The conflict stays genuinely ambiguous even so — a `[mappings.gates]` entry
/// against a filename token, either of which can be changed to settle it. Both
/// remedies are spelled out in the error's own message, which is where that
/// choice belongs.
fn gate_configuration_owner(
    root_path: &Path,
    pack: &crate::packs::Pack,
    pack_config: &crate::config::DodotConfig,
    root_config: &crate::config::DodotConfig,
    declared: Option<GateLayer>,
) -> PathBuf {
    let inherited = pack_config.gates == root_config.gates
        && pack_config.mappings.gates == root_config.mappings.gates;

    match declared {
        Some(GateLayer::Root) => root_path.join(DOTFILES_CONFIG_FILE),
        Some(GateLayer::Pack) => pack.path.join(DOTFILES_CONFIG_FILE),
        None if inherited => root_path.join(DOTFILES_CONFIG_FILE),
        None => pack.path.join(DOTFILES_CONFIG_FILE),
    }
}

/// A scan failure, classified by what the user can act on.
///
/// The scanner does two jobs at once: it walks the pack, and it interprets the
/// pack's configuration — `[mappings.gates]` globs, the gate labels those
/// entries and filename tokens name, and the conflict between the two all come
/// back as [`DodotError::Config`](crate::error::DodotError::Config). Reporting
/// those as a routing failure would name a *directory* when the thing the user
/// has to open is a `.dodot.toml` (story 14: name the bad file, not the
/// neighbourhood it is in); `gate_config_file` is which one, resolved by
/// [`gate_configuration_owner`]. Everything else the walk can fail on — an
/// unreadable directory, a layout Dodot refuses — is a routing failure and
/// names the directory.
fn unusable_scan(
    pack_path: &Path,
    gate_config_file: &Path,
    error: crate::error::DodotError,
) -> SafetyLockError {
    if matches!(error, crate::error::DodotError::Config(_)) {
        unusable_config(gate_config_file.to_path_buf(), error)
    } else {
        unusable_routing(pack_path, error)
    }
}

#[cfg(test)]
mod tests {
    use crate::fs::OsFs;
    use crate::testing::{TempEnvironment, TempEnvironmentBuilder};

    use super::super::roots::{RootIdentity, RootSource};
    use super::*;

    fn host() -> HostFacts {
        HostFacts::for_tests("darwin", "arm64")
    }

    /// The inventory of `env`'s dotfiles root, as an unapproved implicit root
    /// reaching the prompt.
    fn inventory_of(env: &TempEnvironment) -> RootInventory {
        build_inventory(&resolved(env), env.fs.as_ref(), &host()).unwrap()
    }

    fn resolved(env: &TempEnvironment) -> ResolvedRoot {
        ResolvedRoot::new(
            RootIdentity::new(canonical_root(env)).unwrap(),
            RootSource::Git,
        )
    }

    /// The root as selection would have canonicalized it — the spelling every
    /// diagnostic below names, since that is the path the inventory walks.
    fn canonical_root(env: &TempEnvironment) -> PathBuf {
        std::fs::canonicalize(&env.dotfiles_root).unwrap()
    }

    /// `(category, path)` pairs, which is what the prompt renders.
    fn sample_of(inventory: &RootInventory) -> Vec<(InventoryCategory, String)> {
        inventory
            .sample
            .iter()
            .map(|entry| {
                (
                    entry.category,
                    entry.relative_path.to_string_lossy().into_owned(),
                )
            })
            .collect()
    }

    fn counts_of(inventory: &RootInventory) -> Vec<(InventoryCategory, usize)> {
        inventory
            .counts
            .iter()
            .map(|count| (count.category, count.files))
            .collect()
    }

    /// One pack exercising every default handler mapping at once.
    fn representative_pack(builder: TempEnvironmentBuilder, name: &str) -> TempEnvironmentBuilder {
        builder
            .pack(name)
            .file("aliases.sh", "alias g=git")
            .file("install.sh", "#!/bin/sh")
            .file("Brewfile", "brew \"jq\"")
            .file("externals.toml", "")
            .file("bin/tool", "#!/bin/sh")
            .file("config", "x")
            .done()
    }

    /// Story 3, and the point of the whole module: what the user is shown is
    /// every recognized file, classified once, by the handler the loaded
    /// configuration routes it to.
    ///
    /// Once, including where two rules could claim a file: `install.sh` is
    /// matched by both the install rule and the `*.sh` shell glob, and the
    /// inventory counts whichever one routing gives it — not both, and not the
    /// lower-priority one.
    #[test]
    fn every_recognized_file_lands_in_exactly_one_category() {
        let env = representative_pack(TempEnvironment::builder(), "tools").build();

        let inventory = inventory_of(&env);

        assert_eq!(
            counts_of(&inventory),
            [
                (InventoryCategory::Shell, 1),
                // `install.sh` and `Brewfile` are both code execution: the
                // category is the handler's phase, not its name.
                (InventoryCategory::CodeExecution, 2),
                (InventoryCategory::Path, 1),
                (InventoryCategory::External, 1),
                (InventoryCategory::Link, 1),
            ]
        );
        assert_eq!(inventory.total_files(), 6);
        assert_eq!(inventory.omitted, 0);
        assert_eq!(
            sample_of(&inventory),
            [
                (InventoryCategory::Shell, "tools/aliases.sh".into()),
                (InventoryCategory::CodeExecution, "tools/Brewfile".into()),
                (InventoryCategory::CodeExecution, "tools/install.sh".into()),
                (InventoryCategory::Path, "tools/bin".into()),
                (InventoryCategory::External, "tools/externals.toml".into()),
                (InventoryCategory::Link, "tools/config".into()),
            ]
        );
    }

    /// Detail order is category priority first, then pack-relative path — so
    /// the entries that can change every future shell survive the cap, and two
    /// runs over an unchanged root show the same list.
    #[test]
    fn detail_is_ordered_by_category_then_path() {
        let env = TempEnvironment::builder()
            .pack("zsh")
            .file("zshrc.sh", "")
            .done()
            .pack("apps")
            .file("gitconfig", "")
            .file("install.sh", "")
            .done()
            .pack("alacritty")
            .file("alacritty.toml", "")
            .done()
            .build();

        let inventory = inventory_of(&env);

        assert_eq!(
            sample_of(&inventory),
            [
                (InventoryCategory::Shell, "zsh/zshrc.sh".into()),
                (InventoryCategory::CodeExecution, "apps/install.sh".into()),
                (InventoryCategory::Link, "alacritty/alacritty.toml".into()),
                (InventoryCategory::Link, "apps/gitconfig".into()),
            ]
        );
        assert_eq!(sample_of(&inventory_of(&env)), sample_of(&inventory));
    }

    /// Story 3 again, from the other side: the cap keeps the confirmation
    /// question on screen. What it must not do is quietly shrink the root —
    /// the counts stay whole and the dropped paths are counted.
    #[test]
    fn detail_stops_at_ten_paths_and_counts_what_it_dropped() {
        let mut builder = TempEnvironment::builder().pack("links");
        for index in 0..20 {
            builder = builder.file(&format!("config{index:02}"), "");
        }
        let env = representative_pack(builder.done(), "tools").build();

        let inventory = inventory_of(&env);

        assert_eq!(inventory.sample.len(), MAX_INVENTORY_PATHS);
        assert_eq!(inventory.total_files(), 26);
        assert_eq!(inventory.omitted, 16);
        assert_eq!(
            inventory.omitted + inventory.sample.len(),
            inventory.total_files(),
            "the cap lost paths instead of counting them"
        );

        // Counts are unaffected by the cap: `link` shows 21 while only four
        // of its paths are detailed.
        assert_eq!(
            counts_of(&inventory),
            [
                (InventoryCategory::Shell, 1),
                (InventoryCategory::CodeExecution, 2),
                (InventoryCategory::Path, 1),
                (InventoryCategory::External, 1),
                (InventoryCategory::Link, 21),
            ]
        );

        // And what survives is the high-impact end of the order.
        assert_eq!(
            sample_of(&inventory)[..6],
            [
                (InventoryCategory::Shell, "tools/aliases.sh".into()),
                (InventoryCategory::CodeExecution, "tools/Brewfile".into()),
                (InventoryCategory::CodeExecution, "tools/install.sh".into()),
                (InventoryCategory::Path, "tools/bin".into()),
                (InventoryCategory::External, "tools/externals.toml".into()),
                (InventoryCategory::Link, "links/config00".into()),
            ]
        );
    }

    /// Custom mappings are configuration, and configuration is what the
    /// inventory reads: routing the user changed must be the routing the
    /// prompt describes, or it would orient them against a deployment that
    /// never happens.
    #[test]
    fn custom_handler_mappings_are_honoured() {
        let env = TempEnvironment::builder()
            .pack("nvim")
            .config(
                "[mappings]\n\
                 path = \"scripts\"\n\
                 shell = [\"*.zsh\"]\n",
            )
            .file("scripts/tool", "")
            .file("profile.zsh", "")
            .file("aliases.sh", "")
            .done()
            .build();

        let inventory = inventory_of(&env);

        assert_eq!(
            sample_of(&inventory),
            [
                (InventoryCategory::Shell, "nvim/profile.zsh".into()),
                (InventoryCategory::Path, "nvim/scripts".into()),
                // `bin` is no longer the PATH directory and `*.sh` no longer
                // shell, so both fall through to the catchall.
                (InventoryCategory::Link, "nvim/aliases.sh".into()),
            ]
        );
    }

    /// Files that are claimed in order to *not* be deployed — ignored packs,
    /// ignored and skipped entries, and entries gated off this host — are not
    /// recognized files. Counting them would inflate every number the user
    /// reads with things approval cannot act on.
    #[test]
    fn what_deploys_nothing_is_not_counted() {
        let env = TempEnvironment::builder()
            .pack("archive")
            .file("vimrc", "")
            .ignored()
            .done()
            .pack("vim")
            .config("[mappings]\nignore = [\"notes.md\"]\n")
            .file("vimrc", "")
            .file("notes.md", "")
            .file("README.md", "")
            .file("linux-only.sh._linux", "")
            .done()
            .build();

        let inventory = inventory_of(&env);

        assert_eq!(counts_of(&inventory), [(InventoryCategory::Link, 1)]);
        assert_eq!(
            sample_of(&inventory),
            [(InventoryCategory::Link, "vim/vimrc".into())],
            "a filtered, gated, or ignored-pack file reached the prompt"
        );
    }

    /// A pack gated off this host deploys nothing here, so it describes a
    /// machine the user is not on.
    #[test]
    fn packs_gated_off_this_host_are_not_counted() {
        let env = TempEnvironment::builder()
            .pack("linux-tools")
            .config("[pack]\nos = [\"linux\"]\n")
            .file("aliases.sh", "")
            .done()
            .pack("vim")
            .file("vimrc", "")
            .done()
            .build();

        assert_eq!(
            sample_of(&inventory_of(&env)),
            [(InventoryCategory::Link, "vim/vimrc".into())]
        );
    }

    /// A gate that *passes* is invisible to routing by design — the scanner
    /// presents `aliases._darwin.sh` as `aliases.sh` and lifts `_darwin/vimrc`
    /// to the pack root — but it must stay visible to the prompt. The user is
    /// being asked to recognize their own root, and a path that is not in the
    /// directory is evidence about a directory that does not exist. The
    /// classification still follows the effective name: `aliases._darwin.sh`
    /// is a shell file because `aliases.sh` is what the rules match.
    #[test]
    fn a_passing_gate_is_listed_under_the_name_the_root_carries() {
        let env = TempEnvironment::builder()
            .pack("zsh")
            .file("aliases._darwin.sh", "alias g=git")
            .done()
            .pack("vim")
            .file("_darwin/vimrc", "")
            .done()
            .build();

        let inventory = inventory_of(&env);

        assert_eq!(
            counts_of(&inventory),
            [(InventoryCategory::Shell, 1), (InventoryCategory::Link, 1)],
            "the passing gates changed what the entries were classified as"
        );
        assert_eq!(
            sample_of(&inventory),
            [
                (InventoryCategory::Shell, "zsh/aliases._darwin.sh".into()),
                (InventoryCategory::Link, "vim/_darwin/vimrc".into()),
            ],
            "the prompt showed a path that does not exist under the root"
        );
    }

    /// The scanner stops at a pack's top level and so does the inventory: a
    /// directory is one entry, because one entry is what routing recognized
    /// and what a handler acts on. Recursing to make the count "more accurate"
    /// would be inventing detail the deployment does not have.
    #[test]
    fn a_directory_entry_counts_once_and_is_not_descended_into() {
        let env = TempEnvironment::builder()
            .pack("vim")
            .file("colors/one.vim", "")
            .file("colors/two.vim", "")
            .file("after/ftplugin/rust.vim", "")
            .done()
            .build();

        let inventory = inventory_of(&env);

        assert_eq!(counts_of(&inventory), [(InventoryCategory::Link, 2)]);
        assert_eq!(
            sample_of(&inventory),
            [
                (InventoryCategory::Link, "vim/after".into()),
                (InventoryCategory::Link, "vim/colors".into()),
            ]
        );
    }

    /// Spec, "Non-Goals": this is a recognition aid, not a dry run. A template
    /// is listed under its source name because rendering it — and resolving
    /// what it would render *from* — is exactly the work refused here, which
    /// is also why a first-ever run needs no rendered baseline.
    #[test]
    fn a_template_is_listed_by_its_source_name_without_being_rendered() {
        let env = TempEnvironment::builder()
            .pack("git")
            .file("gitconfig.tmpl", "[user]\n  name = {{ env.USER }}\n")
            .done()
            .build();

        let before = env.list_dir_names(&env.data_dir);

        assert_eq!(
            sample_of(&inventory_of(&env)),
            [(InventoryCategory::Link, "git/gitconfig.tmpl".into())]
        );
        assert_eq!(
            env.list_dir_names(&env.data_dir),
            before,
            "building the inventory wrote Dodot state"
        );
    }

    /// The same claim as the test above, made structurally: the implementation
    /// cannot render, preprocess, or read a candidate file because it never
    /// names the machinery that would.
    #[test]
    fn the_implementation_reads_no_candidate_contents() {
        let implementation = include_str!("inventory.rs")
            .split_once("#[cfg(test)]")
            .expect("this file carries a test module")
            .0;
        let code = implementation
            .lines()
            .filter(|line| !line.trim_start().starts_with("//"))
            .collect::<Vec<_>>()
            .join("\n");

        for forbidden in [
            "preprocess",
            "render",
            "secret",
            "read_to_string",
            "read_file",
            "to_intents",
            "DataStore",
            "walk_pack_recursive",
        ] {
            assert!(
                !code.contains(forbidden),
                "the inventory reaches past routing metadata through `{forbidden}`"
            );
        }
    }

    /// Story 14: configuration Dodot cannot load is named, with its problem,
    /// and stops the inventory — an approvable decision is never produced from
    /// a root Dodot could not describe.
    #[test]
    fn invalid_configuration_names_the_offending_file() {
        for (pack_config, root_config, offender) in [
            (Some("not valid toml"), None, "vim/.dodot.toml"),
            (None, Some("[pack]\nos = [\"linux\"]\n"), ".dodot.toml"),
        ] {
            let mut builder = TempEnvironment::builder().pack("vim").file("vimrc", "");
            if let Some(contents) = pack_config {
                builder = builder.config(contents);
            }
            let env = builder.done().build();
            if let Some(contents) = root_config {
                std::fs::write(env.dotfiles_root.join(".dodot.toml"), contents).unwrap();
            }

            let error =
                build_inventory(&resolved(&env), env.fs.as_ref(), &host()).expect_err("accepted");

            assert!(
                matches!(
                    &error,
                    SafetyLockError::DotfilesConfigUnusable { config_file, reason }
                        if config_file == &canonical_root(&env).join(offender) && !reason.is_empty()
                ),
                "unexpected error: {error}"
            );
        }
    }

    /// Story 14 again, for the configuration that fails in *use* rather than
    /// in parsing. The scanner interprets gate configuration as it walks, so
    /// an invalid `[mappings.gates]` glob, an unresolvable label, and a file
    /// gated two ways at once all surface from the walk — and naming the pack
    /// directory for them would point the user at a place instead of at the
    /// file they have to edit.
    ///
    /// Every case here declares gate configuration in the pack, which is what
    /// makes the pack's file the one to name; the root-owned half is
    /// `configuration_inherited_from_the_root_names_the_root_config_file`.
    #[test]
    fn configuration_the_walk_rejects_names_the_pack_config_file() {
        for pack_config in [
            // An invalid glob: rejected when the mapping gates compile.
            "[mappings.gates]\n\"[unclosed\" = \"darwin\"\n",
            // A label nothing defines, reached through `[mappings.gates]`.
            "[mappings.gates]\n\"aliases.sh\" = \"no-such-label\"\n",
            // A label nothing defines, reached through the filename grammar,
            // in a pack whose own `[gates]` could have defined it.
            "[gates]\nsomething-else = { os = \"darwin\" }\n",
            // One file gated two ways at once.
            "[mappings.gates]\n\"*.sh\" = \"darwin\"\n",
        ] {
            let env = TempEnvironment::builder()
                .pack("vim")
                .config(pack_config)
                .file("aliases.sh", "")
                .file("profile._no-such-label.sh", "")
                .done()
                .build();

            let error =
                build_inventory(&resolved(&env), env.fs.as_ref(), &host()).expect_err("accepted");

            assert!(
                matches!(
                    &error,
                    SafetyLockError::DotfilesConfigUnusable { config_file, reason }
                        if config_file == &canonical_root(&env).join("vim/.dodot.toml")
                            && !reason.is_empty()
                ),
                "unexpected error for config {pack_config:?}: {error}"
            );
        }
    }

    /// The same walk-time failures, declared one layer up. Pack configuration
    /// is the root's merged with the pack's, so gate settings the root owns
    /// reach every pack's scan — and blaming the pack for them would name a
    /// `.dodot.toml` the user never wrote and, for a pack with no
    /// configuration of its own, one that is not on disk to open.
    #[test]
    fn configuration_inherited_from_the_root_names_the_root_config_file() {
        for root_config in [
            "[mappings.gates]\n\"[unclosed\" = \"darwin\"\n",
            "[mappings.gates]\n\"aliases.sh\" = \"no-such-label\"\n",
            "",
            "[mappings.gates]\n\"*.sh\" = \"darwin\"\n",
        ] {
            let env = TempEnvironment::builder()
                .pack("vim")
                .file("aliases.sh", "")
                .file("profile._no-such-label.sh", "")
                .done()
                .build();
            std::fs::write(env.dotfiles_root.join(".dodot.toml"), root_config).unwrap();

            let error =
                build_inventory(&resolved(&env), env.fs.as_ref(), &host()).expect_err("accepted");

            assert!(
                matches!(
                    &error,
                    SafetyLockError::DotfilesConfigUnusable { config_file, reason }
                        if config_file == &canonical_root(&env).join(DOTFILES_CONFIG_FILE)
                            && !reason.is_empty()
                ),
                "unexpected error for root config {root_config:?}: {error}"
            );
        }
    }

    /// Both layers contributing is where "did the pack write any gate
    /// configuration?" stops being enough: a valid pack entry that has nothing
    /// to do with the failure would otherwise pull the blame onto the pack,
    /// where changing or deleting that entry cannot fix anything. A root whose
    /// own layer does not stand up is named even when the pack also wrote gate
    /// configuration.
    #[test]
    fn a_broken_root_is_named_even_when_the_pack_also_configures_gates() {
        for (root_config, pack_config) in [
            // Invalid `[gates]` entry in the root, unrelated valid one in the
            // pack — codex's counterexample: deleting `laptop` fixes nothing.
            (
                "[gates]\nbroken = { nonsense = \"x\" }\n",
                "[gates]\nlaptop = { os = \"darwin\" }\n",
            ),
            // Invalid root glob, unrelated valid pack mapping.
            (
                "[mappings.gates]\n\"[unclosed\" = \"darwin\"\n",
                "[mappings.gates]\n\"vimrc\" = \"darwin\"\n",
            ),
        ] {
            let env = TempEnvironment::builder()
                .pack("vim")
                .config(pack_config)
                .file("aliases.sh", "")
                .file("vimrc", "")
                .done()
                .build();
            std::fs::write(env.dotfiles_root.join(".dodot.toml"), root_config).unwrap();

            let error =
                build_inventory(&resolved(&env), env.fs.as_ref(), &host()).expect_err("accepted");

            assert!(
                matches!(
                    &error,
                    SafetyLockError::DotfilesConfigUnusable { config_file, reason }
                        if config_file == &canonical_root(&env).join(DOTFILES_CONFIG_FILE)
                            && !reason.is_empty()
                ),
                "unexpected error for root {root_config:?} + pack {pack_config:?}: {error}"
            );
        }
    }

    /// A `[mappings.gates]` entry the walk never consults cannot answer for a
    /// failure it had no part in.
    ///
    /// The walk resolves a mapping's label only when that mapping's glob
    /// matches an entry it met, so `"*.txt" = "no-such-label"` beside no `.txt`
    /// file is accepted — the first case here proves the inventory is built.
    /// The second puts that same dormant mapping in the root and lets the walk
    /// fail on a filename gate token instead: the diagnostic is about
    /// `a._foo.sh`, so it must name the file that has something to say about
    /// `foo` — the pack — not the one that happens to hold an unrelated
    /// unresolved mapping.
    #[test]
    fn a_mapping_the_walk_never_consults_does_not_claim_another_failure() {
        let dormant = "[mappings.gates]\n\"*.txt\" = \"no-such-label\"\n";

        let env = TempEnvironment::builder()
            .pack("vim")
            .config(dormant)
            .file("vimrc", "")
            .done()
            .build();

        assert_eq!(
            sample_of(&inventory_of(&env)),
            [(InventoryCategory::Link, "vim/vimrc".into())],
            "a mapping matching nothing was treated as a defect"
        );

        let env = TempEnvironment::builder()
            .pack("vim")
            .config("[gates]\nlaptop = { os = \"darwin\" }\n")
            .file("a._foo.sh", "")
            .done()
            .build();
        std::fs::write(env.dotfiles_root.join(DOTFILES_CONFIG_FILE), dormant).unwrap();

        let error =
            build_inventory(&resolved(&env), env.fs.as_ref(), &host()).expect_err("accepted");

        assert!(
            matches!(
                &error,
                SafetyLockError::DotfilesConfigUnusable { config_file, reason }
                    if config_file == &canonical_root(&env).join("vim/.dodot.toml")
                        && reason.contains("foo")
            ),
            "a dormant root mapping claimed a filename-gate failure: {error}"
        );
    }

    /// The mirror of the case above, and the reason blame is read off the
    /// failing entry rather than off either file's overall health: the two
    /// tables fail independently, so a defect in one file must not pull the
    /// blame away from the other file's defect that actually stopped the walk.
    ///
    /// Both directions of the interaction, in both tables.
    #[test]
    fn the_layer_that_declared_the_failing_entry_is_the_one_named() {
        struct Case {
            root: &'static str,
            pack: &'static str,
            owner: &'static str,
        }

        for case in [
            // A root mapping completed by a pack-defined label — unresolvable
            // read alone, fine merged — beside a pack glob that will not
            // compile. The walk stops on the pack's glob.
            Case {
                root: "[mappings.gates]\n\"aliases.sh\" = \"laptop\"\n",
                pack: "[gates]\nlaptop = { os = \"darwin\" }\n\
                       [mappings.gates]\n\"[unclosed\" = \"darwin\"\n",
                owner: "vim/.dodot.toml",
            },
            // A broken root `[mappings.gates]` glob beside a broken pack
            // `[gates]` entry. `merge_user` fails first, on the pack's entry,
            // and the root's unrelated mapping defect must not claim it.
            Case {
                root: "[mappings.gates]\n\"[unclosed\" = \"darwin\"\n",
                pack: "[gates]\nbroken = { nonsense = \"x\" }\n",
                owner: "vim/.dodot.toml",
            },
            // The same shape reversed: a broken root `[gates]` entry beside a
            // pack mapping that is merely unresolvable on its own. The
            // `[gates]` failure comes first and belongs to the root.
            Case {
                root: "[gates]\nbroken = { nonsense = \"x\" }\n",
                pack: "[mappings.gates]\n\"aliases.sh\" = \"darwin\"\n",
                owner: ".dodot.toml",
            },
        ] {
            let env = TempEnvironment::builder()
                .pack("vim")
                .config(case.pack)
                .file("aliases.sh", "")
                .file("vimrc", "")
                .done()
                .build();
            std::fs::write(env.dotfiles_root.join(".dodot.toml"), case.root).unwrap();

            let error =
                build_inventory(&resolved(&env), env.fs.as_ref(), &host()).expect_err("accepted");

            assert!(
                matches!(
                    &error,
                    SafetyLockError::DotfilesConfigUnusable { config_file, .. }
                        if config_file == &canonical_root(&env).join(case.owner)
                ),
                "root {:?} + pack {:?} should have named {}: {error}",
                case.root,
                case.pack,
                case.owner
            );
        }
    }

    /// Two broken entries owned by different files, and the one the user is
    /// told about has to be the one the named file declares.
    ///
    /// The walk and the attribution replay both visit entries in key order —
    /// `merge_user` and `compile_mapping_gates` sort, rather than taking
    /// whatever order the hash gives — so the entry that fails is the entry
    /// blame is read from. Without that, the message could quote one label
    /// while the path pointed at the file holding the other.
    #[test]
    fn the_reported_defect_and_the_named_file_are_the_same_entry() {
        for (root, pack, owner, quoted) in [
            // Root owns the lexically first broken label, pack the later one.
            (
                "[gates]\naaa = { nonsense = \"x\" }\n",
                "[gates]\nzzz = { nonsense = \"x\" }\n",
                ".dodot.toml",
                "aaa",
            ),
            // Reversed: the pack owns the lexically first one.
            (
                "[gates]\nzzz = { nonsense = \"x\" }\n",
                "[gates]\naaa = { nonsense = \"x\" }\n",
                "vim/.dodot.toml",
                "aaa",
            ),
            // The same, for globs that will not compile.
            (
                "[mappings.gates]\n\"[a\" = \"darwin\"\n",
                "[mappings.gates]\n\"[z\" = \"darwin\"\n",
                ".dodot.toml",
                "[a",
            ),
            (
                "[mappings.gates]\n\"[z\" = \"darwin\"\n",
                "[mappings.gates]\n\"[a\" = \"darwin\"\n",
                "vim/.dodot.toml",
                "[a",
            ),
        ] {
            let env = TempEnvironment::builder()
                .pack("vim")
                .config(pack)
                .file("vimrc", "")
                .done()
                .build();
            std::fs::write(env.dotfiles_root.join(DOTFILES_CONFIG_FILE), root).unwrap();

            let error =
                build_inventory(&resolved(&env), env.fs.as_ref(), &host()).expect_err("accepted");

            assert!(
                matches!(
                    &error,
                    SafetyLockError::DotfilesConfigUnusable { config_file, reason }
                        if config_file == &canonical_root(&env).join(owner)
                            && reason.contains(quoted)
                ),
                "root {root:?} + pack {pack:?} should report {quoted:?} against {owner}: {error}"
            );
        }
    }

    /// A pack that breaks a label the root had valid owns it, even though the
    /// deep merge means the resolved entry carries both files' dimensions.
    #[test]
    fn a_pack_that_breaks_an_inherited_label_is_named_for_it() {
        let env = TempEnvironment::builder()
            .pack("vim")
            .config("[gates]\nlaptop = { nonsense = \"x\" }\n")
            .file("vimrc", "")
            .done()
            .build();
        std::fs::write(
            env.dotfiles_root.join(".dodot.toml"),
            "[gates]\nlaptop = { os = \"darwin\" }\n",
        )
        .unwrap();

        let error =
            build_inventory(&resolved(&env), env.fs.as_ref(), &host()).expect_err("accepted");

        assert!(
            matches!(
                &error,
                SafetyLockError::DotfilesConfigUnusable { config_file, .. }
                    if config_file == &canonical_root(&env).join("vim/.dodot.toml")
            ),
            "unexpected error: {error}"
        );
    }

    /// The attribution replay is read only to place blame, never to assign it.
    ///
    /// `[mappings.gates]` and `[gates]` are separate tables, so the root can
    /// legitimately map a label that only a pack defines — valid merged, and
    /// unresolvable read on its own. The inventory must still be built. (The
    /// mirror case does not exist: gate labels deep-merge key by key, so a
    /// pack adding `os` to a root label that already carries a bad dimension
    /// inherits the bad dimension too. A pack cannot repair a broken root
    /// `[gates]` entry, only a dangling root reference to one.)
    #[test]
    fn a_root_layer_a_pack_completes_is_not_an_error() {
        let env = TempEnvironment::builder()
            .pack("vim")
            .config("[gates]\nlaptop = { os = \"darwin\" }\n")
            .file("aliases.sh", "")
            .done()
            .build();
        std::fs::write(
            env.dotfiles_root.join(".dodot.toml"),
            "[mappings.gates]\n\"aliases.sh\" = \"laptop\"\n",
        )
        .unwrap();

        let inventory = build_inventory(&resolved(&env), env.fs.as_ref(), &host())
            .expect("a root mapping a pack-defined label was refused");

        assert_eq!(
            sample_of(&inventory),
            [(InventoryCategory::Shell, "vim/aliases.sh".into())]
        );
    }

    /// A root that cannot be walked fails the inventory rather than reporting
    /// an empty one: "nothing here" and "I could not look" must not read the
    /// same to a user deciding whether to authorize the path.
    #[test]
    fn a_root_that_cannot_be_routed_fails_rather_than_reporting_nothing() {
        let env = TempEnvironment::builder().build();
        // Two packs that collide on one display name — a layout dodot
        // refuses to route.
        for name in ["010-vim", "020-vim"] {
            std::fs::create_dir(env.dotfiles_root.join(name)).unwrap();
        }

        let error = build_inventory(&resolved(&env), env.fs.as_ref(), &host()).expect_err("routed");

        assert!(
            matches!(
                &error,
                SafetyLockError::PackRoutingUnusable { directory, .. }
                    if directory == &canonical_root(&env)
            ),
            "unexpected error: {error}"
        );
    }

    /// An empty root is a real, describable state — the first thing a user
    /// pointed at the wrong directory should see is that Dodot recognized
    /// nothing in it.
    #[test]
    fn an_empty_root_inventories_to_nothing() {
        let env = TempEnvironment::builder().build();

        let inventory = build_inventory(&resolved(&env), &OsFs::new(), &host()).unwrap();

        assert!(inventory.counts.is_empty());
        assert!(inventory.sample.is_empty());
        assert_eq!(inventory.omitted, 0);
        assert_eq!(inventory.total_files(), 0);
    }

    /// Categories follow the handler's declared phase, so a handler added
    /// later is classified by what it does rather than by whether this module
    /// was updated. The match is exhaustive: a new phase will not compile
    /// until it is placed.
    #[test]
    fn categories_follow_handler_phases() {
        use ExecutionPhase::*;

        assert_eq!(
            InventoryCategory::for_phase(ShellInit),
            Some(InventoryCategory::Shell)
        );
        assert_eq!(
            InventoryCategory::for_phase(Setup),
            Some(InventoryCategory::CodeExecution)
        );
        assert_eq!(
            InventoryCategory::for_phase(Provision),
            Some(InventoryCategory::CodeExecution)
        );
        assert_eq!(
            InventoryCategory::for_phase(PathExport),
            Some(InventoryCategory::Path)
        );
        assert_eq!(
            InventoryCategory::for_phase(External),
            Some(InventoryCategory::External)
        );
        assert_eq!(
            InventoryCategory::for_phase(Link),
            Some(InventoryCategory::Link)
        );
        assert_eq!(
            InventoryCategory::for_phase(Filter),
            None,
            "a handler that deploys nothing was counted"
        );
    }

    /// A handler the registry does not know is still a file the configuration
    /// routes, so it counts — as `other`, the category that exists for
    /// exactly this.
    #[test]
    fn an_unknown_handler_counts_as_other() {
        let fs = OsFs::new();
        let registry = create_registry(&fs);

        assert_eq!(
            category_of("a-handler-from-the-future", &registry),
            Some(InventoryCategory::Other)
        );
        assert_eq!(
            category_of(crate::handlers::HANDLER_SHELL, &registry),
            Some(InventoryCategory::Shell)
        );
        assert_eq!(
            category_of(crate::handlers::HANDLER_IGNORE, &registry),
            None
        );
    }

    /// The prompt's priority is encoded in the type's ordering rather than a
    /// separate table, so a sort on `(category, path)` cannot drift from the
    /// documented order.
    #[test]
    fn category_ordering_is_the_prompt_priority() {
        let mut shuffled = vec![
            InventoryCategory::Other,
            InventoryCategory::Link,
            InventoryCategory::CodeExecution,
            InventoryCategory::External,
            InventoryCategory::Shell,
            InventoryCategory::Path,
        ];
        shuffled.sort();

        assert_eq!(shuffled, InventoryCategory::PROMPT_ORDER);
    }

    #[test]
    fn entries_sort_by_category_then_relative_path() {
        let mut entries = [
            InventoryEntry {
                category: InventoryCategory::Link,
                relative_path: PathBuf::from("vim/vimrc"),
            },
            InventoryEntry {
                category: InventoryCategory::Shell,
                relative_path: PathBuf::from("zsh/zshrc"),
            },
            InventoryEntry {
                category: InventoryCategory::Shell,
                relative_path: PathBuf::from("bash/bashrc"),
            },
        ];
        entries.sort();

        assert_eq!(
            entries
                .iter()
                .map(|entry| entry.relative_path.to_str().unwrap())
                .collect::<Vec<_>>(),
            ["bash/bashrc", "zsh/zshrc", "vim/vimrc"]
        );
    }

    #[test]
    fn counts_stay_whole_when_paths_are_capped() {
        let inventory = RootInventory {
            counts: vec![
                CategoryCount {
                    category: InventoryCategory::Shell,
                    files: 12,
                },
                CategoryCount {
                    category: InventoryCategory::Link,
                    files: 30,
                },
            ],
            sample: Vec::new(),
            omitted: 42,
        };

        assert_eq!(inventory.total_files(), 42);
        assert!(MAX_INVENTORY_PATHS < inventory.total_files());
    }

    #[test]
    fn category_labels_are_stable() {
        assert_eq!(
            InventoryCategory::PROMPT_ORDER
                .iter()
                .map(|category| category.label())
                .collect::<Vec<_>>(),
            [
                "shell",
                "code execution",
                "path",
                "external",
                "link",
                "other"
            ]
        );
    }
}
