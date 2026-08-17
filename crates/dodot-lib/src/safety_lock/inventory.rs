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

use std::collections::BTreeMap;
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

    // Only `Handler::phase` is read, so the run-once handlers never reach a
    // subprocess — the same posture `handlers::configuration_handler_names`
    // takes when it inspects the registry without exercising it.
    let runner = crate::datastore::NoopCommandRunner;
    let registry = create_registry(fs, &runner);
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

        let mut gates = GateTable::with_builtins();
        if !pack_config.gates.is_empty() {
            gates
                .merge_user(&pack_config.gates)
                .map_err(|error| unusable_config(pack.path.join(DOTFILES_CONFIG_FILE), error))?;
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
            .map_err(|error| unusable_scan(&pack.path, error))?;

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

/// A scan failure, classified by what the user can act on.
///
/// The scanner does two jobs at once: it walks the pack, and it interprets the
/// pack's configuration — `[mappings.gates]` globs, the gate labels those
/// entries and filename tokens name, and the conflict between the two all come
/// back as [`DodotError::Config`](crate::error::DodotError::Config). Reporting
/// those as a routing failure would name the pack *directory* when the thing
/// the user has to open is the pack's `.dodot.toml` (story 14: name the bad
/// file, not the neighbourhood it is in). Everything else the walk can fail on
/// — an unreadable directory, a layout Dodot refuses — is a routing failure and
/// names the directory.
fn unusable_scan(pack_path: &Path, error: crate::error::DodotError) -> SafetyLockError {
    if matches!(error, crate::error::DodotError::Config(_)) {
        unusable_config(pack_path.join(DOTFILES_CONFIG_FILE), error)
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
    #[test]
    fn configuration_the_walk_rejects_names_the_pack_config_file() {
        for pack_config in [
            // An invalid glob: rejected when the mapping gates compile.
            "[mappings.gates]\n\"[unclosed\" = \"darwin\"\n",
            // A label nothing defines, reached through `[mappings.gates]`.
            "[mappings.gates]\n\"aliases.sh\" = \"no-such-label\"\n",
            // A label nothing defines, reached through the filename grammar.
            "",
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
        let runner = crate::datastore::NoopCommandRunner;
        let registry = create_registry(&fs, &runner);

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
