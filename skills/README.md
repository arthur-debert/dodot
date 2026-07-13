# dodot skills

Installable agent skills for [dodot](../README.md), in the open `SKILL.md` format —
detectable by Claude Code, Copilot, Cursor, and skill CLIs.

| Skill                                         | What it does                                                                                                                                                                                             |
|-----------------------------------------------|----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------|
| [`using-dodot`](using-dodot/SKILL.md)         | Operate an existing dotfiles repo with dodot: deploy packs, check what's live, add or adopt config files, undo deployments.                                                                              |
| [`dodot-templates`](dodot-templates/SKILL.md) | Author and sync dodot templates and secrets: config that varies per host/OS, value injection and whole-file encryption, and reverse-merging deployed-side edits back to source. Builds on `using-dodot`. |

## Install

Copy a skill directory into your agent's skills location, e.g. for Claude Code:

```bash
# project-scoped
cp -r skills/using-dodot .claude/skills/

# or user-scoped (all projects)
cp -r skills/using-dodot ~/.claude/skills/
```

Each skill is self-contained: a `SKILL.md` plus its reference files.
