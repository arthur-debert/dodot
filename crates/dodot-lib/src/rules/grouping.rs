//! Grouping helpers: bucket [`RuleMatch`]es by handler and order
//! handlers for execution.

use std::collections::HashMap;

use crate::rules::RuleMatch;

/// Groups rule matches by handler name.
pub fn group_by_handler(matches: &[RuleMatch]) -> HashMap<String, Vec<RuleMatch>> {
    let mut groups: HashMap<String, Vec<RuleMatch>> = HashMap::new();
    for m in matches {
        groups.entry(m.handler.clone()).or_default().push(m.clone());
    }
    groups
}

/// Returns handler names in execution order.
///
/// Order is driven by each handler's [`ExecutionPhase`]
/// (see [`crate::handlers::ExecutionPhase`] for the full phase list and
/// why each slot is where it is). The phase enum's declaration order
/// *is* the execution order — `Provision` → `Setup` → `PathExport` →
/// `ShellInit` → `Link`.
///
/// Handler names not present in the registry are placed last in
/// alphabetical order (they get ignored by the pipeline anyway).
///
/// [`ExecutionPhase`]: crate::handlers::ExecutionPhase
pub fn handler_execution_order(
    groups: &HashMap<String, Vec<RuleMatch>>,
    registry: &HashMap<String, Box<dyn crate::handlers::Handler + '_>>,
) -> Vec<String> {
    let mut names: Vec<String> = groups.keys().cloned().collect();
    names.sort_by(|a, b| {
        let pa = registry.get(a).map(|h| h.phase());
        let pb = registry.get(b).map(|h| h.phase());
        match (pa, pb) {
            (Some(x), Some(y)) => x.cmp(&y),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => a.cmp(b),
        }
    });
    names
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn group_by_handler_groups_correctly() {
        let matches = vec![
            RuleMatch {
                relative_path: "vimrc".into(),
                absolute_path: "/d/vim/vimrc".into(),
                pack: "vim".into(),
                handler: "symlink".into(),
                is_dir: false,
                options: HashMap::new(),
                preprocessor_source: None,
                rendered_bytes: None,
            },
            RuleMatch {
                relative_path: "aliases.sh".into(),
                absolute_path: "/d/vim/aliases.sh".into(),
                pack: "vim".into(),
                handler: "shell".into(),
                is_dir: false,
                options: HashMap::new(),
                preprocessor_source: None,
                rendered_bytes: None,
            },
            RuleMatch {
                relative_path: "gvimrc".into(),
                absolute_path: "/d/vim/gvimrc".into(),
                pack: "vim".into(),
                handler: "symlink".into(),
                is_dir: false,
                options: HashMap::new(),
                preprocessor_source: None,
                rendered_bytes: None,
            },
        ];

        let groups = group_by_handler(&matches);
        assert_eq!(groups.len(), 2);
        assert_eq!(groups["symlink"].len(), 2);
        assert_eq!(groups["shell"].len(), 1);
    }

    #[test]
    fn handler_execution_order_follows_phase_declaration() {
        let mut groups = HashMap::new();
        groups.insert("symlink".into(), vec![]);
        groups.insert("install".into(), vec![]);
        groups.insert("shell".into(), vec![]);
        groups.insert("homebrew".into(), vec![]);
        groups.insert("path".into(), vec![]);

        let fs = crate::fs::OsFs::new();
        let registry = crate::handlers::create_registry(&fs);
        let order = handler_execution_order(&groups, &registry);

        // Exact order matches ExecutionPhase declaration:
        // Provision(homebrew) -> Setup(install) -> PathExport(path)
        //   -> ShellInit(shell) -> Link(symlink)
        assert_eq!(
            order,
            vec!["homebrew", "install", "path", "shell", "symlink"]
        );
    }

    #[test]
    fn handler_execution_order_places_unknown_handlers_last() {
        let mut groups = HashMap::new();
        groups.insert("symlink".into(), vec![]);
        groups.insert("zzz-unknown".into(), vec![]);
        groups.insert("homebrew".into(), vec![]);

        let fs = crate::fs::OsFs::new();
        let registry = crate::handlers::create_registry(&fs);
        let order = handler_execution_order(&groups, &registry);

        // Known handlers keep phase order; unknown lands at the end.
        assert_eq!(order, vec!["homebrew", "symlink", "zzz-unknown"]);
    }
}
