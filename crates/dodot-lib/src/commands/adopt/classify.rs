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
//! ## Where the rules apply
//!
//! Only at the positions the top-level walk reads a name: the first
//! component of the prospective in-pack path, and — because a gate
//! directory whose predicate holds on this host expands transparently
//! and surfaces its children at pack-root level — the first component
//! inside each leading passing gate directory. [`scan_positions`]
//! computes that list; everything below it belongs to whichever handler
//! claims the top-level entry, which for a wholesale-linked directory is
//! nobody.
//!
//! So `lua/plugins/init.lua` is classified on `lua` alone. Routing
//! prefixes (`_home/`, `_xdg/`, `_app/`, `_lib/`) are ordinary names at
//! a classified position: they are neither hidden nor reserved, and the
//! walk does not descend into them, so `_home/.gitconfig` is adoptable.

use std::path::{Component, Path};

use crate::fs::Fs;
use crate::gates::{parse_dir_gate_label, GateTable, HostFacts};
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
        }
    }
}

// ── Classification ───────────────────────────────────────────────────

/// The rule that stops a pack scan from reading `in_pack`, or `None`
/// when every classified position is one the scan reads.
///
/// Positions are tested outermost first and the first match wins: the
/// walk skips at the outermost position and never reads what is under
/// it, so an ignored `lua` decides the entry whatever `plugins` and
/// `init.lua` match.
///
/// Within one position the order is reserved, then ignore, then hidden.
/// Reserved comes first because it is the more specific fact about a
/// name that is also hidden, and the more useful one to report (§3.2).
/// Ignore comes before hidden so a `.DS_Store` — which both rules match
/// — reports the pattern the user can edit rather than the rule they
/// cannot.
pub(crate) fn classify(
    in_pack: &Path,
    ignore: &EffectiveIgnore,
    gates: &GateTable,
    host: &HostFacts,
) -> Option<SkipRule> {
    scan_positions(in_pack, gates, host)
        .into_iter()
        .find_map(|name| rule_for(&name, ignore))
}

/// The rule matching one name read at a classified position.
fn rule_for(name: &str, ignore: &EffectiveIgnore) -> Option<SkipRule> {
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
/// children are never read, so they are never classified either.
///
/// Routing prefixes are not gates ([`parse_dir_gate_label`] excludes
/// them) and the walk hands the whole directory to a handler, so the
/// list ends at `_home` for `_home/.gitconfig`.
///
/// An unrecognised `_<label>` segment also ends the list. The scan
/// treats an unknown gate label as a hard error, but that is a verdict
/// on the pack the user already has; adopt classifies the positions it
/// can resolve and leaves the rest to the scan that will read them.
fn scan_positions(in_pack: &Path, gates: &GateTable, host: &HostFacts) -> Vec<String> {
    let mut positions = Vec::new();
    for component in in_pack.components() {
        let Component::Normal(raw) = component else {
            continue;
        };
        let name = raw.to_string_lossy().into_owned();
        let descend = parse_dir_gate_label(&name)
            .and_then(|label| gates.lookup(label))
            .is_some_and(|predicate| predicate.matches(host));
        positions.push(name);
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
            classify(&PathBuf::from("_darwin/.DS_Store"), &ig, &gates(), &linux()),
            None
        );
    }

    #[test]
    fn routing_prefixes_are_ordinary_names_and_stop_the_walk() {
        let ig = ignore(&[]);
        assert_eq!(
            classify(&PathBuf::from("_home/.gitconfig"), &ig, &gates(), &linux()),
            None
        );
    }

    #[test]
    fn dot_config_is_the_hidden_rules_exception() {
        let ig = ignore(&[]);
        assert_eq!(
            classify(&PathBuf::from(".config"), &ig, &gates(), &linux()),
            None
        );
    }

    #[test]
    fn reserved_wins_over_hidden() {
        let ig = ignore(&[]);
        let rule = classify(&PathBuf::from(".dodot.toml"), &ig, &gates(), &linux()).unwrap();
        assert!(matches!(rule, SkipRule::Reserved { .. }));
    }

    #[test]
    fn ignore_wins_over_hidden() {
        let ig = ignore(&[".DS_Store"]);
        let rule = classify(&PathBuf::from(".DS_Store"), &ig, &gates(), &linux()).unwrap();
        assert!(matches!(rule, SkipRule::Ignored { .. }));
    }

    #[test]
    fn reserved_below_the_first_component_is_adoptable() {
        let ig = ignore(&[]);
        assert_eq!(
            classify(&PathBuf::from("lua/.dodot.toml"), &ig, &gates(), &linux()),
            None
        );
    }
}
