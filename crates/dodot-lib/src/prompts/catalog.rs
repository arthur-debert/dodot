//! Static catalog of known prompt keys with human-readable descriptions.
//!
//! The [`PromptRegistry`](super::PromptRegistry) itself is content-agnostic
//! — keys are opaque strings. This catalog gives `dodot prompts list` a
//! description column and serves as the canonical list of prompts the
//! codebase introduces. Adding a new prompt? Pick a key, register it
//! here.
//!
//! Keys follow `<area>.<name>` convention: `plist.install_filters`,
//! `template.first_render`, etc. Reverse-DNS-style nesting if an area
//! grows multiple sub-features.

/// A documented prompt key.
#[derive(Debug, Clone, Copy)]
pub struct PromptDescriptor {
    /// The key passed to [`PromptRegistry`](super::PromptRegistry) calls.
    pub key: &'static str,
    /// Human-readable summary of when this prompt fires and what it asks.
    pub description: &'static str,
}

/// Every prompt key the codebase uses. Keep alphabetised by key so
/// `dodot prompts list` output is stable across edits.
pub const KNOWN_PROMPTS: &[PromptDescriptor] = &[
    PromptDescriptor {
        key: "magic.install_ladder",
        description:
            "Single Y/n covering the post-`up` install ladder (pre-commit hook + plist filter \
             + template filter, whichever apply). Replaces the three sequential prompts that \
             were dismissed independently.",
    },
    PromptDescriptor {
        key: "plist.cfprefsd_invalidate",
        description:
            "macOS only: offer to run `killall cfprefsd` after a `dodot up` that touched plist \
             files, so running GUI apps re-read the new values from disk.",
    },
    PromptDescriptor {
        key: "plist.install_filters",
        description:
            "Per-component dismissal target for the plist clean/smudge filter rung of the \
             install ladder. The user-facing prompt is `magic.install_ladder`; this key tracks \
             whether that ladder rung has been dismissed.",
    },
    PromptDescriptor {
        key: "template.install_filter",
        description:
            "Per-component dismissal target for the template clean filter rung of the install \
             ladder. The user-facing prompt is `magic.install_ladder`; this key tracks whether \
             that ladder rung has been dismissed.",
    },
    PromptDescriptor {
        key: "template.install_hook",
        description: "Per-component dismissal target for the pre-commit hook rung of the install \
             ladder. The user-facing prompt is `magic.install_ladder`; this key tracks whether \
             that ladder rung has been dismissed.",
    },
];

/// Look up a descriptor by key, if registered.
pub fn lookup(key: &str) -> Option<&'static PromptDescriptor> {
    KNOWN_PROMPTS.iter().find(|d| d.key == key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_are_unique() {
        let mut seen = std::collections::HashSet::new();
        for d in KNOWN_PROMPTS {
            assert!(seen.insert(d.key), "duplicate prompt key: {}", d.key);
        }
    }

    #[test]
    fn keys_are_alphabetised() {
        let keys: Vec<_> = KNOWN_PROMPTS.iter().map(|d| d.key).collect();
        let mut sorted = keys.clone();
        sorted.sort();
        assert_eq!(keys, sorted, "KNOWN_PROMPTS must be sorted by key");
    }

    #[test]
    fn lookup_finds_known_keys() {
        assert!(lookup("plist.install_filters").is_some());
        assert!(lookup("nonexistent.key").is_none());
    }
}
