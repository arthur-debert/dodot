//! Is the manager *well enough* to use? — the fitness probe, which
//! spawns, and is therefore `up`-only.
//!
//! [`availability`](crate::provisioners::availability) asks whether a
//! manager exists, and answers with `stat` and mode bits. This module
//! asks the questions only the manager itself can answer, which means
//! spawning it. The two are separate modules rather than two arms of
//! one function precisely so the boundary is visible: `dodot status`
//! carries a real `ShellCommandRunner` in production and is kept
//! passive by its call graph rather than by a disabled runner, it is
//! pinned to leave the datastore byte-identical, and it reaches the
//! presence probe on every provisioning row. Nothing here may be
//! reachable from it.
//!
//! The one caller is [`commands::up`](crate::commands::up), behind
//! `!ctx.dry_run`. A dry run reports what it *would* do, and a
//! command that spawns the user's package manager to find out is no
//! longer reporting.
//!
//! # The first question: Homebrew's version floor
//!
//! A `Brewfile` may declare `go`, `cargo`, `uv`, `npm`, and `krew`
//! entries alongside `brew` and `cask`, and dodot's position on the
//! language-bound package managers rests on it: they are Homebrew's
//! job, not dodot's. Those entry types arrived across four releases,
//! the last of them — `npm` and `krew` — in Homebrew 5.1.2. Below a
//! given entry type's release, the line the user was entitled to
//! write fails as a parse error out of `brew bundle`, which names
//! neither the version nor the remedy.
//!
//! Brew's own auto-update would carry a stale installation over the
//! floor without dodot doing anything. dodot sets
//! `HOMEBREW_NO_AUTO_UPDATE` for provisioning commands, so on a
//! dodot-managed run it does not: an old brew stays old. Having taken
//! that away, dodot owes the user the check.
//!
//! # Checked whenever the manifest is there, not when it needs it
//!
//! Only a `Brewfile` that actually uses one of the newer entry types
//! is affected, and checking only those would be friendlier. dodot
//! does not, for a reason that outranks friendliness: knowing would
//! mean reading and parsing the user's `Brewfile`, and a run-once
//! manifest is opaque to dodot by design — its content is hashed and
//! handed to the manager, never interpreted. A version floor is a
//! fact about the machine, and the machine is what this module is
//! allowed to look at.
//!
//! # It reports; it does not refuse
//!
//! Because the check is not conditioned on content, a below-floor
//! brew is not proof that anything will fail: a `Brewfile` of plain
//! `brew` lines runs perfectly on a brew from 2024. Refusing the run
//! would turn a warning dodot is not sure about into a deployment
//! this machine cannot do. So the condition is named, with its
//! remedy, *before* the file runs — and then the file runs. If a
//! `go` line does fail afterwards, the explanation is already on
//! screen.
//!
//! # Ephemeral, like every other probe answer
//!
//! A [`Fitness`] is never written to the datastore and never becomes
//! part of a receipt. A receipt asserts *this exact file content ran
//! successfully*; the manager's version at that moment is not part of
//! that claim, and recording it would mean a `brew update` invalidated
//! receipts it has nothing to do with.
//!
//! See `docs/adr/0010-a-fitness-probe-spawns-so-it-is-up-only.md`.

use crate::datastore::{CommandRunner, CommandSpec};
use crate::provisioners::descriptor_for;

/// The argument every supported manager answers its version to.
///
/// Homebrew prints `Homebrew 5.1.2`, Nix prints `nix (Nix) 2.35.2`:
/// different words, same shape, and [`reported_version`] reads the
/// number out of either. A manager that needed a different argument
/// would state it on its descriptor; none does.
const VERSION_ARGUMENT: &str = "--version";

/// A manager's minimum usable version, and what to say about a host
/// below it.
///
/// Compile-time data on the handler's
/// [`ProvisionerDescriptor`](crate::provisioners::ProvisionerDescriptor),
/// next to the candidate paths and the environment — never
/// configuration. The same rule that keeps a pack from choosing which
/// executable runs keeps it from lowering the bar that executable has
/// to clear. See
/// `docs/adr/0004-configuration-selects-files-never-executables.md`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VersionFloor {
    /// The lowest version that is fit, as the manager spells it.
    pub minimum: &'static str,
    /// What stops working below the floor, as a clause: it is placed
    /// after "older than <minimum>: ".
    pub below: &'static str,
    /// The command that raises the version — the whole reason to
    /// report the condition rather than let it surface as a parse
    /// error.
    pub remedy: &'static str,
}

/// Homebrew's floor: 5.1.2, where `npm` and `krew` `Brewfile` entries
/// arrived (2026-03-30), and the last of the five entry types that
/// make the language-bound package managers Homebrew's problem rather
/// than dodot's.
///
/// Support is staggered: `go` landed in 4.6.17, `cargo` in 5.0.7,
/// `uv` in 5.0.16, and `npm`/`krew` in 5.1.2. dodot declares the
/// highest of those as one floor because it does not read the
/// `Brewfile` to find out which entry types a user wrote — which is
/// also why [`Fitness::warning`] says *one of them may* fail rather
/// than naming a line it has not seen. A brew at 5.0.16 runs a
/// `Brewfile` of `go` and `cargo` entries perfectly, and still gets
/// the warning.
pub const HOMEBREW_VERSION_FLOOR: VersionFloor = VersionFloor {
    minimum: "5.1.2",
    below: "not all of the `Brewfile` entry types go, cargo, uv, npm, \
            and krew are supported, and one of them may fail to parse",
    remedy: "brew update",
};

/// Whether a manager is well enough to be used.
///
/// Three outcomes, for the same reason
/// [`Availability`](crate::provisioners::availability::Availability)
/// has three: "brew is too old" and "brew would not tell me how old
/// it is" are different machines with different remedies, and a probe
/// that reported the second as the first would tell a user to run
/// `brew update` against a brew whose version nobody has read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fitness {
    /// The manager answered, and its answer clears the floor — or it
    /// declares no floor, and there was nothing to ask.
    Fit,
    /// The manager answered with a version below its floor.
    BelowFloor {
        /// The version as the manager reported it.
        found: String,
        /// The floor it did not clear.
        floor: VersionFloor,
    },
    /// The manager could not be asked, or did not answer with a
    /// version dodot can compare against the floor. Never folded into
    /// [`Fitness::Fit`]: a silent pass would claim dodot checked
    /// something it did not.
    ProbeFailed {
        /// What went wrong, in the manager's own words where there
        /// are any.
        detail: String,
    },
}

impl Fitness {
    /// What to tell the user, or `None` when there is nothing to say.
    ///
    /// A fit manager produces no output at all. The two other
    /// outcomes both end in the same place — this run continues —
    /// because the probe is about the machine and the file is about
    /// its content, and dodot has not read the file. See the module
    /// docs.
    ///
    /// `handler` names the manager the way every other provisioning
    /// row does (`homebrew`, `nix`), and `executable` is the path the
    /// run will spawn, so a host with two brews shows which one
    /// answered.
    pub fn warning(&self, handler: &str, executable: &str) -> Option<String> {
        match self {
            Fitness::Fit => None,
            Fitness::BelowFloor { found, floor } => Some(format!(
                "{handler} at {executable} is {found}, older than {}: {}. \
                 Run `{}` to update it. dodot runs this file anyway — \
                 everything that does not need the newer {handler} still works.",
                floor.minimum, floor.below, floor.remedy
            )),
            Fitness::ProbeFailed { detail } => {
                let floor = floor_for(handler);
                let consequence = floor
                    .map(|f| {
                        format!(
                            " dodot cannot tell whether it is at {}, below which {}.",
                            f.minimum, f.below
                        )
                    })
                    .unwrap_or_default();
                Some(format!(
                    "{handler} at {executable} did not report its version: {detail}.\
                     {consequence} This run continues."
                ))
            }
        }
    }
}

/// The version floor `handler` declares, if it declares one.
pub fn floor_for(handler: &str) -> Option<VersionFloor> {
    descriptor_for(handler).and_then(|d| d.version_floor)
}

/// Ask `executable` for its version and compare it against
/// `handler`'s floor.
///
/// Spawns exactly one process, and only for a handler that declares a
/// floor: `install` and `nix` return [`Fitness::Fit`] without the
/// runner being touched.
///
/// `executable` is the program the run is about to spawn — the
/// absolute path the presence probe found, carried on the intent —
/// not the handler's name. Asking a `brew` on `PATH` how old it is
/// and then running a different one would answer a question nobody
/// asked. `environment` is that same intent's rows, for the same
/// reason and one more: `HOMEBREW_NO_AUTO_UPDATE` belongs on this
/// spawn too, or the act of reading brew's version would trigger the
/// auto-update this project turned off, and the answer would describe
/// a brew that did not exist a moment ago.
pub fn probe(
    runner: &dyn CommandRunner,
    handler: &str,
    executable: &str,
    environment: &[(String, String)],
) -> Fitness {
    let Some(floor) = floor_for(handler) else {
        return Fitness::Fit;
    };
    let arguments = [VERSION_ARGUMENT.to_string()];
    let output = match runner.run(CommandSpec::with_environment(
        executable,
        &arguments,
        environment,
    )) {
        Ok(output) => output,
        Err(e) => {
            return Fitness::ProbeFailed {
                detail: e.to_string(),
            }
        }
    };
    if output.exit_code != 0 {
        let said = first_line(&output.stderr)
            .or_else(|| first_line(&output.stdout))
            .unwrap_or("no output");
        return Fitness::ProbeFailed {
            detail: format!(
                "`{executable} {VERSION_ARGUMENT}` exited {} ({said})",
                output.exit_code
            ),
        };
    }

    match reported_version(&output.stdout) {
        Reported::Exact { text, version } => {
            let minimum = Version::parse(floor.minimum).expect(
                "every declared floor is a dotted numeric version; \
                 pinned by tests::every_declared_floor_parses",
            );
            if version < minimum {
                Fitness::BelowFloor { found: text, floor }
            } else {
                Fitness::Fit
            }
        }
        // `Homebrew >=4.1.0 (shallow or no git repository)` — brew's
        // own answer when it cannot read its git history. It is a
        // lower bound, so it can neither clear the floor nor fail it,
        // and reading it as `4.1.0` would tell a user on a current
        // brew to update.
        Reported::Inexact { text } => Fitness::ProbeFailed {
            detail: format!("reported `{text}`, which is a lower bound rather than a version"),
        },
        Reported::Unreadable => Fitness::ProbeFailed {
            detail: match first_line(&output.stdout) {
                Some(line) => format!("reported `{line}`, which names no version"),
                None => "printed nothing".to_string(),
            },
        },
    }
}

/// What a manager's `--version` output turned out to be.
#[derive(Debug, PartialEq, Eq)]
enum Reported {
    /// A version dodot can compare, with the text it was read from.
    Exact { text: String, version: Version },
    /// A lower bound rather than a version.
    Inexact { text: String },
    /// No version in there at all.
    Unreadable,
}

/// Read the version out of a manager's `--version` output.
///
/// The first line only, and the first token in it that begins with a
/// digit: `Homebrew 5.1.2` and `nix (Nix) 2.35.2` are the same shape
/// wearing different words. Trailing detail on the token — brew's
/// `4.3.0-77-g1234567` on a git checkout — stops the number and is
/// discarded, because a commit count is not a version component.
fn reported_version(stdout: &str) -> Reported {
    let Some(line) = first_line(stdout) else {
        return Reported::Unreadable;
    };
    for token in line.split_whitespace() {
        if let Some(rest) = token.strip_prefix(">=") {
            if rest.starts_with(|c: char| c.is_ascii_digit()) {
                return Reported::Inexact {
                    text: token.to_string(),
                };
            }
            continue;
        }
        if !token.starts_with(|c: char| c.is_ascii_digit()) {
            continue;
        }
        let number: String = token
            .chars()
            .take_while(|c| c.is_ascii_digit() || *c == '.')
            .collect();
        if let Some(version) = Version::parse(&number) {
            return Reported::Exact {
                text: number,
                version,
            };
        }
    }
    Reported::Unreadable
}

/// The first non-empty line of some output, trimmed.
fn first_line(text: &str) -> Option<&str> {
    text.lines().map(str::trim).find(|line| !line.is_empty())
}

/// A dotted numeric version, ordered component by component.
///
/// Trailing zero components are dropped so `5.1` and `5.1.0` are the
/// same version rather than the first being older than the second —
/// which is what a plain lexicographic comparison of the component
/// lists would say.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Version(Vec<u64>);

impl Version {
    /// Parse `5.1.2`. `None` for anything that is not at least one
    /// number, or that carries a component too large to be one.
    fn parse(text: &str) -> Option<Self> {
        let mut components = Vec::new();
        for part in text.split('.') {
            if part.is_empty() {
                continue;
            }
            components.push(part.parse::<u64>().ok()?);
        }
        while components.last() == Some(&0) {
            components.pop();
        }
        if components.is_empty() && !text.contains(|c: char| c.is_ascii_digit()) {
            return None;
        }
        Some(Self(components))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::datastore::CommandOutput;
    use crate::handlers::{HANDLER_HOMEBREW, HANDLER_INSTALL, HANDLER_NIX};
    use crate::provisioners::PROVISIONERS;
    use std::sync::Mutex;

    /// Answers one staged `--version` reply and records every spawn.
    ///
    /// Recording is half the point: several tests here are about a
    /// spawn that must *not* happen, and a runner that only returned
    /// canned output could not witness one.
    struct StagedVersion {
        reply: Result<CommandOutput, String>,
        spawns: Mutex<Vec<Spawn>>,
    }

    /// One spawn, as the runner saw it.
    struct Spawn {
        executable: String,
        arguments: Vec<String>,
        environment: Vec<(String, String)>,
    }

    impl StagedVersion {
        fn saying(stdout: &str) -> Self {
            Self::replying(Ok(CommandOutput {
                exit_code: 0,
                stdout: stdout.to_string(),
                stderr: String::new(),
            }))
        }

        fn replying(reply: Result<CommandOutput, String>) -> Self {
            Self {
                reply,
                spawns: Mutex::new(Vec::new()),
            }
        }

        fn spawn_count(&self) -> usize {
            self.spawns.lock().unwrap().len()
        }
    }

    impl CommandRunner for StagedVersion {
        fn run(&self, command: CommandSpec<'_>) -> crate::Result<CommandOutput> {
            self.spawns.lock().unwrap().push(Spawn {
                executable: command.executable.to_string(),
                arguments: command.arguments.to_vec(),
                environment: command.environment.to_vec(),
            });
            self.reply.clone().map_err(crate::DodotError::Other)
        }
    }

    fn probe_brew(runner: &StagedVersion) -> Fitness {
        probe(runner, HANDLER_HOMEBREW, "/opt/homebrew/bin/brew", &[])
    }

    // ── The floor ───────────────────────────────────────────────

    #[test]
    fn a_brew_below_the_floor_is_reported_with_its_remedy() {
        let runner = StagedVersion::saying("Homebrew 5.0.16\n");
        let fitness = probe_brew(&runner);

        assert_eq!(
            fitness,
            Fitness::BelowFloor {
                found: "5.0.16".to_string(),
                floor: HOMEBREW_VERSION_FLOOR,
            }
        );
        let warning = fitness
            .warning(HANDLER_HOMEBREW, "/opt/homebrew/bin/brew")
            .expect("a below-floor brew has something to say");
        assert!(warning.contains("5.0.16"), "{warning}");
        assert!(warning.contains("5.1.2"), "{warning}");
        assert!(
            warning.contains("brew update"),
            "the remedy is the reason the warning exists: {warning}"
        );
    }

    #[test]
    fn a_brew_at_the_floor_is_fit_and_says_nothing() {
        let runner = StagedVersion::saying("Homebrew 5.1.2\n");
        let fitness = probe_brew(&runner);

        assert_eq!(fitness, Fitness::Fit);
        assert_eq!(
            fitness.warning(HANDLER_HOMEBREW, "/opt/homebrew/bin/brew"),
            None,
            "the floor is a minimum, and a machine that meets it is not news"
        );
    }

    #[test]
    fn a_brew_above_the_floor_is_fit() {
        let runner = StagedVersion::saying(
            "Homebrew 6.0.18\nHomebrew/homebrew-core (git revision abc; last commit 2026-08-20)\n",
        );
        assert_eq!(probe_brew(&runner), Fitness::Fit);
    }

    #[test]
    fn a_development_build_is_read_by_its_release_number() {
        // `brew --version` on a git checkout appends the commits since
        // the tag. Those are not a fourth version component.
        let runner = StagedVersion::saying("Homebrew 6.0.18-77-g1234567\n");
        assert_eq!(probe_brew(&runner), Fitness::Fit);

        let old = StagedVersion::saying("Homebrew 5.0.7-3-gabcdefg\n");
        assert!(matches!(probe_brew(&old), Fitness::BelowFloor { .. }));
    }

    // ── When brew will not answer ────────────────────────────────

    #[test]
    fn a_failing_version_command_is_a_probe_failure_not_an_absent_manager() {
        let runner = StagedVersion::replying(Ok(CommandOutput {
            exit_code: 1,
            stdout: String::new(),
            stderr: "dyld: library not loaded\n".to_string(),
        }));
        let fitness = probe_brew(&runner);

        match &fitness {
            Fitness::ProbeFailed { detail } => {
                assert!(detail.contains("exited 1"), "{detail}");
                assert!(detail.contains("dyld"), "the manager's own words: {detail}");
            }
            other => panic!("expected a probe failure, got {other:?}"),
        }
        let warning = fitness
            .warning(HANDLER_HOMEBREW, "/opt/homebrew/bin/brew")
            .unwrap();
        assert!(
            !warning.contains("not installed"),
            "a brew that will not answer is not a brew that is missing: {warning}"
        );
    }

    #[test]
    fn a_runner_error_is_a_probe_failure() {
        let runner = StagedVersion::replying(Err("permission denied".to_string()));
        match probe_brew(&runner) {
            Fitness::ProbeFailed { detail } => assert!(detail.contains("permission denied")),
            other => panic!("expected a probe failure, got {other:?}"),
        }
    }

    #[test]
    fn output_with_no_version_in_it_is_a_probe_failure() {
        for stdout in ["", "Homebrew\n", "error: no such command\n"] {
            let runner = StagedVersion::saying(stdout);
            assert!(
                matches!(probe_brew(&runner), Fitness::ProbeFailed { .. }),
                "unreadable output must not pass as fit: {stdout:?}"
            );
        }
    }

    #[test]
    fn a_lower_bound_is_neither_fit_nor_below_the_floor() {
        // Brew's answer from a shallow clone. Reading `>=4.1.0` as
        // `4.1.0` would tell a user on a current brew to update.
        let runner = StagedVersion::saying("Homebrew >=4.1.0 (shallow or no git repository)\n");
        match probe_brew(&runner) {
            Fitness::ProbeFailed { detail } => assert!(detail.contains(">=4.1.0"), "{detail}"),
            other => panic!("expected a probe failure, got {other:?}"),
        }
    }

    // ── What it spawns, and what it does not ─────────────────────

    #[test]
    fn a_handler_with_no_floor_is_fit_without_spawning_anything() {
        for handler in [HANDLER_INSTALL, HANDLER_NIX] {
            let runner = StagedVersion::saying("irrelevant 0.1\n");
            assert_eq!(
                probe(&runner, handler, "/usr/bin/whatever", &[]),
                Fitness::Fit
            );
            assert_eq!(
                runner.spawn_count(),
                0,
                "{handler} declares no floor, so there is no question to ask it"
            );
        }
    }

    #[test]
    fn the_probe_asks_the_executable_the_run_will_spawn_under_its_own_environment() {
        let runner = StagedVersion::saying("Homebrew 6.0.18\n");
        let environment = [("HOMEBREW_NO_AUTO_UPDATE".to_string(), "1".to_string())];
        probe(
            &runner,
            HANDLER_HOMEBREW,
            "/tmp/fake-prefix/bin/brew",
            &environment,
        );

        let spawns = runner.spawns.lock().unwrap();
        assert_eq!(spawns.len(), 1, "one question, one spawn");
        assert_eq!(spawns[0].executable, "/tmp/fake-prefix/bin/brew");
        assert_eq!(spawns[0].arguments, vec!["--version".to_string()]);
        assert_eq!(
            spawns[0].environment, environment,
            "without HOMEBREW_NO_AUTO_UPDATE, asking brew its version would update \
             brew — and the answer would describe a brew that did not exist a moment ago"
        );
    }

    // ── The registry keeps its side of the bargain ───────────────

    #[test]
    fn every_declared_floor_parses() {
        for descriptor in PROVISIONERS {
            let Some(floor) = descriptor.version_floor else {
                continue;
            };
            assert!(
                Version::parse(floor.minimum).is_some(),
                "{}'s floor `{}` is not a version dodot can compare",
                descriptor.handler,
                floor.minimum
            );
        }
    }

    #[test]
    fn homebrew_is_the_only_handler_with_a_floor() {
        // Not a style rule: every floor costs a subprocess on every
        // `up` that would run that handler's file, so a new one is a
        // decision, not a default.
        let with_floors: Vec<&str> = PROVISIONERS
            .iter()
            .filter(|d| d.version_floor.is_some())
            .map(|d| d.handler)
            .collect();
        assert_eq!(with_floors, vec![HANDLER_HOMEBREW]);
    }

    // ── Version ordering ─────────────────────────────────────────

    #[test]
    fn trailing_zero_components_do_not_make_a_version_older() {
        assert_eq!(Version::parse("5.1"), Version::parse("5.1.0"));
        assert!(Version::parse("5.1.2") > Version::parse("5.1"));
        assert!(Version::parse("6.0") > Version::parse("5.1.2"));
        assert!(Version::parse("10.0.0") > Version::parse("9.9.9"));
    }
}
