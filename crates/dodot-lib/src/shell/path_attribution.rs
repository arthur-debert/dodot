//! Per-pack `$PATH` mutation attribution
//! (`docs/proposals/path-precedence.lex` §5).
//!
//! Once [`crate::shell::compose_path_tier`] stopped composing `$PATH`
//! by sequential runtime prepend and started computing the declared
//! packs tier once, in Rust, a raw `export PATH=...` a pack's own shell
//! script issues had nowhere to land in that computation — it's only
//! known once some shell has actually sourced the script. The generated
//! init script closes that gap live: it captures `$PATH` before and
//! after each pack's shell scripts run
//! ([`crate::shell::generate_init_script`]'s per-pack diff wrap) and
//! records what changed to [`crate::paths::Pather::path_attribution_path`].
//!
//! This module owns both ends of that record: [`parse_path_attribution`]
//! / [`read_path_attribution`] read it back, and [`path_provenance`]
//! merges it with the declared packs tier into the single ordered view
//! `dodot probe shell-init` surfaces (§5.4) — read-only, no warning
//! language, no gating (§2.2, §6): it only answers "where did this
//! entry come from".

use std::path::PathBuf;

use serde::Serialize;

use crate::fs::Fs;
use crate::paths::Pather;
use crate::Result;

/// Header line the generated init script writes atop the attribution
/// file. [`parse_path_attribution`] tolerates a file missing it (or any
/// other `#`-prefixed line) rather than requiring it — the shell writer
/// and this reader agree on the format via one shared constant, not via
/// a strict round-trip.
pub const PATH_ATTRIBUTION_MARKER: &str = "# dodot path attribution v1";

/// One pack's raw `$PATH` mutation, captured live by the generated init
/// script's before/after diff around that pack's shell scripts
/// (§5.2). `dirs` is the ordered set-difference: entries present in
/// `$PATH` after the pack's scripts ran that were not present before,
/// left to right as they appear in the after-value. Pre-existing
/// entries that remain, and entries the pack only removed, are not
/// listed.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RawPathEntry {
    pub pack: String,
    pub dirs: Vec<PathBuf>,
}

/// Parse the attribution file's textual content.
///
/// Format: an optional `# `-prefixed header (ignored), then one
/// `<pack>\t<colon-joined dirs>` row per pack whose shell scripts
/// changed `$PATH` on the run that wrote the file. The colon-joined
/// `dirs` field is split here, in Rust — not by the shell diff that
/// wrote it, which is deliberately substring-only and never
/// field-splits (§5.3; the split here carries no portability risk,
/// since it runs on one platform, this binary, not inside whichever
/// shell a user happens to be running).
///
/// Malformed rows (no tab, an empty pack, or an empty dirs field) are
/// skipped rather than failing the whole parse — matches how the sibling
/// shell-init profile parser treats malformed rows
/// ([`crate::probe::parse_profile`]).
pub fn parse_path_attribution(content: &str) -> Vec<RawPathEntry> {
    content
        .lines()
        .filter(|line| !line.starts_with('#') && !line.trim().is_empty())
        .filter_map(|line| {
            let (pack, dirs) = line.split_once('\t')?;
            if pack.is_empty() || dirs.is_empty() {
                return None;
            }
            let dirs: Vec<PathBuf> = dirs
                .split(':')
                .filter(|s| !s.is_empty())
                .map(PathBuf::from)
                .collect();
            if dirs.is_empty() {
                return None;
            }
            Some(RawPathEntry {
                pack: pack.to_string(),
                dirs,
            })
        })
        .collect()
}

/// Read the attribution file back. Empty when it hasn't been written
/// yet — fresh install, `dodot down`, or a datastore with no pack
/// shell scripts at all — which is a normal state, not an error.
pub fn read_path_attribution(fs: &dyn Fs, paths: &dyn Pather) -> Result<Vec<RawPathEntry>> {
    let path = paths.path_attribution_path();
    if !fs.exists(&path) {
        return Ok(Vec::new());
    }
    Ok(parse_path_attribution(&fs.read_to_string(&path)?))
}

/// Which mechanism placed a directory in the composed packs tier: the
/// `path` handler (`Declared`, known at `dodot up` time) or a raw
/// `export PATH=` a pack's own shell script issued (`Raw`, only known
/// once some shell has actually sourced it — see [`read_path_attribution`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum PathOrigin {
    Declared,
    Raw,
}

/// One directory in the merged provenance view: which pack, which
/// directory, and how it got there.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PathProvenanceEntry {
    pub pack: String,
    pub dir: PathBuf,
    pub origin: PathOrigin,
}

/// Build the provenance view `dodot probe shell-init` surfaces (§5.4):
/// every directory in the packs tier, declared or raw, attributed to
/// its pack and ordered the same way the composed `$PATH` places them —
/// last on-disk pack first (§2.3) — with each pack's declared entries
/// immediately followed by that same pack's raw entries, one block per
/// pack (§5.2, "same pack, same lex position, one block").
///
/// Declared entries are re-derived from the live datastore scan through
/// [`crate::shell::compose_path_tier`] — the one owner of the tier/dedup
/// rule (§4.3) — so this view can never disagree with what the next
/// `dodot up` would actually compose. Raw entries come from
/// [`read_path_attribution`]: the most recent shell startup's live
/// capture, which can be stale relative to "right now" the same way any
/// recorded evidence can (a pack's script changed since that shell
/// started); the caller decides whether and how to flag that, the same
/// way it already does for shell-init profiles.
///
/// A pack with raw entries but no declared ones (a shell-only pack)
/// still gets its own block, positioned by its on-disk pack order like
/// any other.
pub fn path_provenance(
    fs: &dyn Fs,
    paths: &dyn Pather,
    homebrew: Option<&super::BrewBlocks>,
) -> Result<Vec<PathProvenanceEntry>> {
    let Some(scan) = super::scan_pack_contributions(fs, paths)? else {
        return Ok(Vec::new());
    };
    let declared =
        super::compose_path_tier(&scan.path_additions, &super::homebrew_known_dirs(homebrew));
    let raw = read_path_attribution(fs, paths)?;

    // Reversed on-disk pack order — same direction the composed `$PATH`
    // string is built in (§2.3): the pack read *last* wins the front.
    // `scan.pack_order` is already one entry per pack in ascending
    // on-disk order.
    let mut pack_order = scan.pack_order;
    pack_order.reverse();

    let mut out = Vec::new();
    for pack in &pack_order {
        for c in declared.iter().filter(|c| &c.pack == pack) {
            out.push(PathProvenanceEntry {
                pack: pack.clone(),
                dir: c.dir.clone(),
                origin: PathOrigin::Declared,
            });
        }
        if let Some(entry) = raw.iter().find(|r| &r.pack == pack) {
            for dir in &entry.dirs {
                out.push(PathProvenanceEntry {
                    pack: pack.clone(),
                    dir: dir.clone(),
                    origin: PathOrigin::Raw,
                });
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_header_and_rows() {
        let content = "# dodot path attribution v1\nvim\t/home/alice/extra/bin\ngit\t/a:/b\n";
        let entries = parse_path_attribution(content);
        assert_eq!(
            entries,
            vec![
                RawPathEntry {
                    pack: "vim".into(),
                    dirs: vec![PathBuf::from("/home/alice/extra/bin")],
                },
                RawPathEntry {
                    pack: "git".into(),
                    dirs: vec![PathBuf::from("/a"), PathBuf::from("/b")],
                },
            ]
        );
    }

    #[test]
    fn header_only_content_parses_to_no_entries() {
        assert!(parse_path_attribution("# dodot path attribution v1\n").is_empty());
        assert!(parse_path_attribution("").is_empty());
    }

    #[test]
    fn skips_malformed_rows() {
        // No tab, empty pack, empty dirs — each dropped rather than
        // failing the whole parse.
        let content = "no-tab-here\n\t/a\nvim\t\n";
        assert!(parse_path_attribution(content).is_empty());
    }

    #[test]
    fn tolerates_a_missing_header() {
        let content = "vim\t/x\n";
        let entries = parse_path_attribution(content);
        assert_eq!(entries[0].pack, "vim");
    }
}
