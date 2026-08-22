//! Deciding whether a pack scan would read an entry adopt is about to
//! create.
//!
//! `docs/proposals/adopt-safety.lex` §3 states the predicate: an entry
//! is *adoptable* when a later pack scan would read it at the in-pack
//! position adopt would give it. Three rules decide that, and they are
//! the three [`crate::rules::Scanner`]'s top-level walk applies —
//! dodot's reserved filenames, the effective `[pack] ignore` list, and
//! the hidden-name rule that skips every dot-prefixed name except
//! `.config`. Adopting an entry any of them skips produces a pack member
//! no `dodot up` and no `dodot status` ever reads.
//!
//! Dispatch-layer filters are deliberately absent. `[mappings] ignore`,
//! `[mappings] skip`, and gate labels drop a file that discovery *did*
//! read, and the user changes that by editing config rather than by
//! moving files (`docs/user/filters.lex` §§2, 5).
//!
//! One more verdict joins the three, for the same reason and with a
//! harsher consequence: a directory named `_<label>` for a label no gate
//! table defines. The top-level walk does not skip it — it stops with a
//! hard error, and that error fails the scan of the whole pack, not just
//! that entry. Adopting behind an undefined label would therefore make
//! every later `dodot up` and `dodot status` fail on a pack that read
//! fine before the command ran.
//!
//! ## Where the rules apply
//!
//! Two kinds of position, because two scans stand between a user's
//! command and the entry adopt is about to create.
//!
//! The **pack directory** is read by the dotfiles-root scan, which
//! applies its own rules to the directory name — the root `[pack]
//! ignore` list, the same hidden-name rule, and the pack-name grammar.
//! [`packs::classify_pack_dir`](crate::packs::classify_pack_dir) holds
//! that predicate and [`pack_dir_refusal`] turns its verdict into a
//! message. It matters because adopt infers a pack name from the
//! source's own path: `~/.config/node_modules/settings.json` infers a
//! `node_modules` pack, which the default ignore list makes every later
//! scan skip.
//!
//! Inside the pack, the rules apply at the positions the top-level walk
//! reads a name: the first component of the prospective in-pack path,
//! and — because a gate directory whose predicate holds on this host
//! expands transparently and surfaces its children at pack-root level —
//! the first component inside each leading passing gate directory.
//! [`scan_positions`] computes that list; everything below it belongs to
//! whichever handler claims the top-level entry, which for a
//! wholesale-linked directory is nobody.
//!
//! So `lua/plugins/init.lua` is classified on `lua` alone. Routing
//! prefixes (`_home/`, `_xdg/`, `_app/`, `_lib/`) are ordinary names at
//! a classified position: they are neither hidden nor reserved, and the
//! walk does not descend into them, so `_home/.gitconfig` is adoptable.

use std::path::{Component, Path};

use crate::fs::Fs;
use crate::gates::{parse_dir_gate_label, GateTable, HostFacts};
use crate::packs::{classify_pack_dir, PackDirSkip};
use crate::rules::{matched_ignore_pattern, SPECIAL_FILES};

// ── The effective `[pack] ignore` list ───────────────────────────────

/// Which configuration layer supplied a pack's effective `[pack] ignore`
/// list.
///
/// Exactly one layer is ever in force: a pack-level list *replaces* the
/// root list, which replaces the built-in defaults
/// (`docs/user/filters.lex` §4). Adopt names the layer in its refusal so
/// the user edits the file that actually decides.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum IgnoreLayer {
    /// Nothing set `[pack] ignore`; the built-in default list is in force.
    Builtin,
    /// The dotfiles root's `.dodot.toml` set it.
    Root,
    /// The destination pack's own `.dodot.toml` set it.
    Pack,
}

impl IgnoreLayer {
    /// How the layer is named inside a message, e.g. "dodot's default
    /// list".
    fn describe(self, pack: &str) -> String {
        match self {
            IgnoreLayer::Builtin => "dodot's default list".to_string(),
            IgnoreLayer::Root => "the root .dodot.toml".to_string(),
            IgnoreLayer::Pack => format!("pack {pack}'s .dodot.toml"),
        }
    }
}

/// The one `[pack] ignore` list in force for the destination pack, and
/// the layer it came from.
///
/// `patterns` is the resolved list — the same value
/// [`ConfigManager::config_for_pack`](crate::config::ConfigManager::config_for_pack)
/// hands the scan for that pack — never a concatenation of layers.
pub(crate) struct EffectiveIgnore {
    pub patterns: Vec<String>,
    pub layer: IgnoreLayer,
}

impl EffectiveIgnore {
    /// Pair an already-resolved pattern list with the layer that set it.
    ///
    /// `patterns` must be `config_for_pack(pack_path).pack.ignore`: the
    /// config resolver has already applied replacement, so this only
    /// answers *which* file won. It answers it by reading the two
    /// `.dodot.toml` files that could have — the pack's, then the
    /// root's — because a layer that sets the list to the same value the
    /// next one down would have is still the layer the user edits.
    ///
    /// A `.dodot.toml` that cannot be read or parsed counts as not
    /// setting the key. The resolver has already rejected a genuinely
    /// broken file by the time this runs, so the only way here is a race
    /// with the user's editor, and guessing "Root" for a file this
    /// process cannot read would name the wrong layer with confidence.
    pub fn resolve(
        fs: &dyn Fs,
        dotfiles_root: &Path,
        pack_path: &Path,
        patterns: Vec<String>,
    ) -> Self {
        let layer = if sets_pack_ignore(fs, &pack_path.join(".dodot.toml")) {
            IgnoreLayer::Pack
        } else if sets_pack_ignore(fs, &dotfiles_root.join(".dodot.toml")) {
            IgnoreLayer::Root
        } else {
            IgnoreLayer::Builtin
        };
        EffectiveIgnore { patterns, layer }
    }

    /// The list the *dotfiles-root* scan applies to pack directory
    /// names, paired with the layer that set it.
    ///
    /// `patterns` must be `root_config().pack.ignore`, the value every
    /// [`packs::scan_packs`](crate::packs::scan_packs) caller passes. A
    /// pack-level list is deliberately not consulted: it lives inside a
    /// pack the root scan has already decided to read or skip, so it can
    /// never be the layer that hid the pack. That leaves two layers, and
    /// the root `.dodot.toml` is the only file that could be either.
    pub fn root(fs: &dyn Fs, dotfiles_root: &Path, patterns: Vec<String>) -> Self {
        let layer = if sets_pack_ignore(fs, &dotfiles_root.join(".dodot.toml")) {
            IgnoreLayer::Root
        } else {
            IgnoreLayer::Builtin
        };
        EffectiveIgnore { patterns, layer }
    }
}

/// Does this `.dodot.toml` set `[pack] ignore` itself?
fn sets_pack_ignore(fs: &dyn Fs, config_path: &Path) -> bool {
    let Ok(text) = fs.read_to_string(config_path) else {
        return false;
    };
    let Ok(value) = text.parse::<toml::Value>() else {
        return false;
    };
    value
        .get("pack")
        .and_then(|pack| pack.get("ignore"))
        .is_some()
}

// ── The verdict ──────────────────────────────────────────────────────

/// The discovery rule that makes a scanned name one no dodot run reads.
///
/// Each variant carries the name at the classified position, which is
/// not always the source's basename: adopting
/// `~/.config/nvim/lua/plugins/init.lua` when `lua` matches a pattern
/// reports `lua`, the component the walk skips.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum SkipRule {
    /// `.dodot.toml` or `.dodotignore` — a name dodot reads its own
    /// configuration from.
    Reserved { name: String },
    /// A match against the effective `[pack] ignore` list.
    Ignored {
        name: String,
        pattern: String,
        layer: IgnoreLayer,
    },
    /// A dot-prefixed name other than `.config`, which the top-level
    /// walk skips whatever the configuration says.
    Hidden { name: String },
    /// A `_<label>` directory naming a gate label no gate table
    /// defines. The walk does not skip this one — it fails the scan of
    /// the entire pack.
    UnknownGate { name: String, label: String },
}

impl SkipRule {
    /// The rule as a short phrase, for the columns of the
    /// zero-adoptable-children listing (`adopt-safety.lex` §3.4).
    pub fn short(&self, pack: &str) -> String {
        match self {
            SkipRule::Reserved { name } => format!("`{name}` is dodot's own"),
            SkipRule::Ignored { pattern, layer, .. } => {
                format!("[pack] ignore `{pattern}` ({})", layer.describe(pack))
            }
            SkipRule::Hidden { .. } => "hidden top-level name".to_string(),
            SkipRule::UnknownGate { label, .. } => format!("undefined gate label `{label}`"),
        }
    }

    /// The rule as a sentence, for the one-run report of entries left
    /// where they were (`adopt-safety.lex` §4).
    pub fn reported(&self, pack: &str) -> String {
        match self {
            SkipRule::Reserved { name } => {
                format!("`{name}` is a name dodot reads its own configuration from")
            }
            SkipRule::Ignored { pattern, layer, .. } => format!(
                "matches `{pattern}` in [pack] ignore ({})",
                layer.describe(pack)
            ),
            SkipRule::Hidden { .. } => {
                "a pack's top-level scan skips names starting with `.` (except .config)".to_string()
            }
            SkipRule::UnknownGate { label, .. } => format!(
                "`_{label}` names a gate label dodot does not define, and a pack scan                  fails on it rather than reading it"
            ),
        }
    }

    /// The refusal for a source the user typed on the command line
    /// (`adopt-safety.lex` §3.2).
    ///
    /// The three rules call for three different things from the user, so
    /// they say three different things. An ignore match quotes the
    /// pattern and names the layer, because editing that layer is the
    /// remedy. The other two have no configuration behind them: the
    /// reserved-name message says what dodot uses the name for, and the
    /// hidden-name message says outright that no setting changes it,
    /// rather than implying a fix that does not exist (§3.7).
    pub fn refusal(&self, source: &Path, in_pack: &Path, pack: &str) -> String {
        let source = source.display();
        match self {
            SkipRule::Reserved { name } if name == ".dodot.toml" => format!(
                "refusing to adopt {source}: `.dodot.toml` is dodot's own pack \
                 configuration file. Adopting one into a pack would replace that \
                 pack's configuration rather than add a managed file. Rename it if \
                 you want it deployed."
            ),
            SkipRule::Reserved { .. } => format!(
                "refusing to adopt {source}: `.dodotignore` is the marker that hides \
                 a pack from dodot. Adopting one into a pack would hide the whole \
                 pack rather than add a managed file. Rename it if you want it \
                 deployed."
            ),
            SkipRule::Ignored {
                name,
                pattern,
                layer,
            } => format!(
                "refusing to adopt {source}: it would land at `{}` in pack {pack}, \
                 where the pack scan reads `{name}` and skips it — `{name}` matches \
                 `{pattern}` in [pack] ignore ({}). No dodot run would read the \
                 entry. To manage it, override [pack] ignore for this pack in \
                 .dodot.toml.",
                in_pack.display(),
                layer.describe(pack),
            ),
            SkipRule::Hidden { name } => format!(
                "refusing to adopt {source}: it would land at `{}` in pack {pack}, \
                 and a pack's top-level scan skips names starting with `.` (except \
                 .config), so no dodot run would read `{name}`. No config setting \
                 changes that.",
                in_pack.display(),
            ),
            SkipRule::UnknownGate { name, label } => format!(
                "refusing to adopt {source}: it would land at `{}` in pack {pack}, \
                 where `{name}` names a gate label dodot does not define. A pack scan \
                 stops on an undefined gate directory, so adopting this would make \
                 every later `dodot up` and `dodot status` fail on pack {pack}. \
                 Define `{label}` under [gates] in .dodot.toml, or rename the \
                 directory. Built-ins: darwin, linux, macos, arm64, aarch64, x86_64.",
                in_pack.display(),
            ),
        }
    }
}

// ── Classification ───────────────────────────────────────────────────

/// The rule that stops a pack scan from reading `in_pack`, or `None`
/// when every classified position is one the scan reads.
///
/// `is_dir` says whether the prospective entry is itself a directory.
/// It decides the gate question at the last position: the walk only
/// parses a `_<label>` name as a gate directory when the entry is a
/// directory, so a *file* called `_darwin` is an ordinary name and a
/// file called `_bogus` is not the undefined-label failure a directory
/// of that name would be.
///
/// Positions are tested outermost first and the first match wins: the
/// walk skips at the outermost position and never reads what is under
/// it, so an ignored `lua` decides the entry whatever `plugins` and
/// `init.lua` match.
///
/// Within one position the order is reserved, then ignore, then hidden,
/// then undefined gate label. Reserved comes first because it is the
/// more specific fact about a name that is also hidden, and the more
/// useful one to report (§3.2). Ignore comes before hidden so a
/// `.DS_Store` — which both rules match — reports the pattern the user
/// can edit rather than the rule they cannot. The gate label comes last
/// because the walk itself reaches it last: it skips a hidden, reserved
/// or ignored name before ever parsing it as a gate directory.
pub(crate) fn classify(
    in_pack: &Path,
    is_dir: bool,
    ignore: &EffectiveIgnore,
    gates: &GateTable,
    host: &HostFacts,
) -> Option<SkipRule> {
    scan_positions(in_pack, is_dir, gates, host)
        .into_iter()
        .find_map(|position| rule_for(&position, ignore, gates))
}

/// The rule that keeps the dotfiles-root scan from reading the pack
/// adopt would publish into, rendered as a refusal, or `None` when the
/// scan reads it.
///
/// The predicate is
/// [`packs::classify_pack_dir`](crate::packs::classify_pack_dir) — the
/// one the scan itself applies — so this only chooses the words. It runs
/// on the *on-disk* directory name, prefix included, because that is the
/// name the scan reads.
///
/// The rules have nothing to do with the source, so the message names
/// the pack rather than the file, and each one ends at the remedy that
/// fits it. Only an ignore match has configuration behind it, so only
/// that message points at a config file. The other three are properties
/// of the name, and `--into <pack>` — an existing pack, which the root
/// scan has by definition already read — is the way past all of them.
pub(crate) fn pack_dir_refusal(
    pack_dir: &str,
    ignore: &EffectiveIgnore,
    dotfiles_root: &Path,
) -> Option<String> {
    let skip = classify_pack_dir(pack_dir, &ignore.patterns)?;
    let root = dotfiles_root.display();
    let head = format!(
        "refusing to adopt into pack `{pack_dir}`: it would be published at {root}/{pack_dir}, "
    );
    let body = match skip {
        PackDirSkip::Ignored(pattern) => format!(
            "which the scan of your dotfiles root skips — `{pack_dir}` matches `{pattern}` \
             in [pack] ignore ({}). No `dodot up` and no `dodot status` would read the \
             pack. Adopt into a pack dodot already reads with --into <pack>, or drop the \
             pattern from [pack] ignore in the root .dodot.toml.",
            ignore.layer.describe(pack_dir),
        ),
        PackDirSkip::Hidden => "and the scan of your dotfiles root skips directories whose \
             name starts with `.` (except .config), so no `dodot up` and no `dodot status` \
             would read the pack. No config setting changes that. Adopt into a pack dodot \
             already reads with --into <pack>."
            .to_string(),
        PackDirSkip::InvalidName => format!(
            "and a pack directory name may hold only letters, digits, `_`, `-` and `.`, \
             so the scan of your dotfiles root would pass `{pack_dir}` over and no dodot \
             run would read the pack. Adopt into a pack dodot already reads with \
             --into <pack>."
        ),
        PackDirSkip::EmptyStem => format!(
            "and `{pack_dir}` reads as an ordering prefix with no name after the \
             separator, which makes every pack scan fail rather than skip it. Adopt into \
             a pack dodot already reads with --into <pack>."
        ),
    };
    Some(head + &body)
}

/// One name the top-level walk reads on the way to the entry.
struct ScanPosition {
    /// The name at that position.
    name: String,
    /// `Some(label)` when the walk would parse this position as a gate
    /// directory — a directory named `_<label>` that is not a routing
    /// prefix.
    gate_label: Option<String>,
}

/// The rule matching one name read at a classified position.
fn rule_for(
    position: &ScanPosition,
    ignore: &EffectiveIgnore,
    gates: &GateTable,
) -> Option<SkipRule> {
    let name = position.name.as_str();
    if SPECIAL_FILES.contains(&name) {
        return Some(SkipRule::Reserved {
            name: name.to_string(),
        });
    }
    if let Some(pattern) = matched_ignore_pattern(name, &ignore.patterns) {
        return Some(SkipRule::Ignored {
            name: name.to_string(),
            pattern: pattern.to_string(),
            layer: ignore.layer,
        });
    }
    if name.starts_with('.') && name != ".config" {
        return Some(SkipRule::Hidden {
            name: name.to_string(),
        });
    }
    if let Some(label) = &position.gate_label {
        if gates.lookup(label).is_none() {
            return Some(SkipRule::UnknownGate {
                name: name.to_string(),
                label: label.clone(),
            });
        }
    }
    None
}

/// The names a pack's top-level walk would read on the way to
/// `in_pack`, outermost first.
///
/// Always the first component. Then, for as long as the component just
/// added is a gate directory whose predicate holds on this host, the
/// next one too: `Scanner::list_top_level` recurses into a passing gate
/// directory and surfaces its children at pack-root level, so
/// `_darwin/.DS_Store` is classified exactly as `.DS_Store` is. A gate
/// directory whose predicate fails is where the walk stops — its
/// children are never read, so they are never classified either. So is a
/// directory whose label no gate table defines, which the walk reaches
/// and fails on.
///
/// A position is a directory when another component follows it, or when
/// it is the last one and `is_dir` says the entry itself is a directory.
/// The walk only parses `_<label>` as a gate on a directory, so a file
/// named `_darwin` is not a gate and neither descends nor fails.
///
/// Routing prefixes are not gates ([`parse_dir_gate_label`] excludes
/// them) and the walk hands the whole directory to a handler, so the
/// list ends at `_home` for `_home/.gitconfig`.
fn scan_positions(
    in_pack: &Path,
    is_dir: bool,
    gates: &GateTable,
    host: &HostFacts,
) -> Vec<ScanPosition> {
    let names: Vec<String> = in_pack
        .components()
        .filter_map(|component| match component {
            Component::Normal(raw) => Some(raw.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect();

    let mut positions = Vec::new();
    for (index, name) in names.iter().enumerate() {
        let position_is_dir = index + 1 < names.len() || is_dir;
        let gate_label = if position_is_dir {
            parse_dir_gate_label(name).map(str::to_string)
        } else {
            None
        };
        let descend = gate_label
            .as_deref()
            .and_then(|label| gates.lookup(label))
            .is_some_and(|predicate| predicate.matches(host));
        positions.push(ScanPosition {
            name: name.clone(),
            gate_label,
        });
        if !descend {
            break;
        }
    }
    positions
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn ignore(patterns: &[&str]) -> EffectiveIgnore {
        EffectiveIgnore {
            patterns: patterns.iter().map(|p| p.to_string()).collect(),
            layer: IgnoreLayer::Builtin,
        }
    }

    fn gates() -> GateTable {
        GateTable::with_builtins()
    }

    fn darwin() -> HostFacts {
        HostFacts {
            os: "darwin".into(),
            arch: "x86_64".into(),
            hostname: None,
            username: None,
        }
    }

    fn linux() -> HostFacts {
        HostFacts {
            os: "linux".into(),
            arch: "x86_64".into(),
            hostname: None,
            username: None,
        }
    }

    #[test]
    fn only_the_first_component_is_classified() {
        let ig = ignore(&["plugins", "*.lua"]);
        assert_eq!(
            classify(
                &PathBuf::from("lua/plugins/init.lua"),
                /*is_dir=*/ false,
                &ig,
                &gates(),
                &linux()
            ),
            None
        );
    }

    #[test]
    fn an_ignored_first_component_names_that_component() {
        let ig = ignore(&["lua"]);
        let rule = classify(
            &PathBuf::from("lua/plugins/init.lua"),
            /*is_dir=*/ false,
            &ig,
            &gates(),
            &linux(),
        )
        .expect("`lua` is ignored");
        assert_eq!(
            rule,
            SkipRule::Ignored {
                name: "lua".into(),
                pattern: "lua".into(),
                layer: IgnoreLayer::Builtin,
            }
        );
    }

    #[test]
    fn a_passing_gate_directory_exposes_its_child_to_classification() {
        let ig = ignore(&[".DS_Store"]);
        let rule = classify(
            &PathBuf::from("_darwin/.DS_Store"),
            /*is_dir=*/ false,
            &ig,
            &gates(),
            &darwin(),
        )
        .expect("a passing gate dir surfaces its children at pack-root level");
        assert_eq!(
            rule,
            SkipRule::Ignored {
                name: ".DS_Store".into(),
                pattern: ".DS_Store".into(),
                layer: IgnoreLayer::Builtin,
            }
        );
    }

    #[test]
    fn a_failing_gate_directory_hides_its_child_from_classification() {
        let ig = ignore(&[".DS_Store"]);
        assert_eq!(
            classify(
                &PathBuf::from("_darwin/.DS_Store"),
                /*is_dir=*/ false,
                &ig,
                &gates(),
                &linux()
            ),
            None
        );
    }

    #[test]
    fn routing_prefixes_are_ordinary_names_and_stop_the_walk() {
        let ig = ignore(&[]);
        assert_eq!(
            classify(
                &PathBuf::from("_home/.gitconfig"),
                /*is_dir=*/ false,
                &ig,
                &gates(),
                &linux()
            ),
            None
        );
    }

    #[test]
    fn dot_config_is_the_hidden_rules_exception() {
        let ig = ignore(&[]);
        assert_eq!(
            classify(
                &PathBuf::from(".config"),
                /*is_dir=*/ false,
                &ig,
                &gates(),
                &linux()
            ),
            None
        );
    }

    #[test]
    fn reserved_wins_over_hidden() {
        let ig = ignore(&[]);
        let rule = classify(
            &PathBuf::from(".dodot.toml"),
            /*is_dir=*/ false,
            &ig,
            &gates(),
            &linux(),
        )
        .unwrap();
        assert!(matches!(rule, SkipRule::Reserved { .. }));
    }

    #[test]
    fn ignore_wins_over_hidden() {
        let ig = ignore(&[".DS_Store"]);
        let rule = classify(
            &PathBuf::from(".DS_Store"),
            /*is_dir=*/ false,
            &ig,
            &gates(),
            &linux(),
        )
        .unwrap();
        assert!(matches!(rule, SkipRule::Ignored { .. }));
    }

    #[test]
    fn an_undefined_gate_directory_is_not_adoptable() {
        let ig = ignore(&[]);
        let rule = classify(
            &PathBuf::from("_bogus/init.lua"),
            /*is_dir=*/ false,
            &ig,
            &gates(),
            &linux(),
        )
        .expect("a pack scan fails on an undefined gate directory");
        assert_eq!(
            rule,
            SkipRule::UnknownGate {
                name: "_bogus".into(),
                label: "bogus".into(),
            }
        );
    }

    #[test]
    fn an_undefined_gate_name_on_a_file_is_adoptable() {
        // The walk only parses `_<label>` as a gate on a directory, so
        // a file of that name is an ordinary top-level entry.
        let ig = ignore(&[]);
        assert_eq!(
            classify(
                &PathBuf::from("_bogus"),
                /*is_dir=*/ false,
                &ig,
                &gates(),
                &linux()
            ),
            None
        );
    }

    #[test]
    fn an_undefined_gate_directory_adopted_whole_is_not_adoptable() {
        let ig = ignore(&[]);
        let rule = classify(
            &PathBuf::from("_bogus"),
            /*is_dir=*/ true,
            &ig,
            &gates(),
            &linux(),
        )
        .expect("a pack scan fails on an undefined gate directory");
        assert!(matches!(rule, SkipRule::UnknownGate { .. }));
    }

    #[test]
    fn hidden_wins_over_an_undefined_gate_label() {
        // `list_top_level` skips a hidden name before it ever parses it
        // as a gate directory, so the message names the rule that fires.
        let ig = ignore(&[]);
        let rule = classify(
            &PathBuf::from("._bogus/init.lua"),
            /*is_dir=*/ false,
            &ig,
            &gates(),
            &linux(),
        )
        .unwrap();
        assert!(matches!(rule, SkipRule::Hidden { .. }));
    }

    #[test]
    fn a_defined_gate_directory_stays_adoptable() {
        let ig = ignore(&[]);
        assert_eq!(
            classify(
                &PathBuf::from("_darwin/init.lua"),
                /*is_dir=*/ false,
                &ig,
                &gates(),
                &darwin()
            ),
            None
        );
    }

    // ── The pack directory itself ────────────────────────────────

    #[test]
    fn an_inferred_pack_name_the_root_scan_ignores_refuses() {
        let ig = ignore(&["node_modules"]);
        let message = pack_dir_refusal("node_modules", &ig, Path::new("/dotfiles"))
            .expect("the root scan skips a pack named `node_modules`");
        assert!(
            message.contains("`node_modules` matches `node_modules`"),
            "{message}"
        );
        assert!(message.contains("dodot's default list"), "{message}");
    }

    #[test]
    fn a_hidden_inferred_pack_name_refuses() {
        let ig = ignore(&[]);
        let message = pack_dir_refusal(".foo", &ig, Path::new("/dotfiles"))
            .expect("the root scan skips a dot-prefixed pack directory");
        assert!(message.contains("starts with"), "{message}");
    }

    #[test]
    fn an_invalid_inferred_pack_name_refuses() {
        let ig = ignore(&[]);
        let message = pack_dir_refusal("has space", &ig, Path::new("/dotfiles"))
            .expect("the root scan skips a name outside the pack-name grammar");
        assert!(message.contains("letters, digits"), "{message}");
    }

    #[test]
    fn an_empty_stem_inferred_pack_name_refuses() {
        let ig = ignore(&[]);
        let message = pack_dir_refusal("010-", &ig, Path::new("/dotfiles"))
            .expect("an empty-stem prefix fails every pack scan");
        assert!(message.contains("ordering prefix"), "{message}");
    }

    #[test]
    fn a_discoverable_pack_name_passes() {
        let ig = ignore(&["node_modules"]);
        assert_eq!(pack_dir_refusal("nvim", &ig, Path::new("/dotfiles")), None);
        assert_eq!(
            pack_dir_refusal("010-nvim", &ig, Path::new("/dotfiles")),
            None
        );
        // `.config` is the hidden rule's exception at this position too.
        assert_eq!(
            pack_dir_refusal(".config", &ig, Path::new("/dotfiles")),
            None
        );
    }

    #[test]
    fn reserved_below_the_first_component_is_adoptable() {
        let ig = ignore(&[]);
        assert_eq!(
            classify(
                &PathBuf::from("lua/.dodot.toml"),
                /*is_dir=*/ false,
                &ig,
                &gates(),
                &linux()
            ),
            None
        );
    }
}
