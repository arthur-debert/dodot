//! `dodot template clean --path <path>` — git clean filter for
//! template sources.
//!
//! This is what makes `git status` / `git diff` / `git log -p` show
//! the truth between commits, even when the user has only edited the
//! deployed file. Git invokes us when reading a working-tree file
//! whose mtime suggests it might have changed (refresh — R5 — is the
//! thing that nudges those mtimes); we look up the cached baseline,
//! compare the deployed bytes to the baseline's rendered hash, and:
//!
//! 1. **Fast path**: if the deployed file matches the baseline
//!    exactly, echo stdin unchanged. No reverse-merge work, no
//!    burgertocow call, no provider invocations. Microseconds. The
//!    common case — most git operations don't follow a deployed-side
//!    edit.
//!
//! 2. **Slow path**: if the deployed file diverges, rehydrate the
//!    cached `TrackedRender::from_tracked_string` and run
//!    `burgertocow::generate_diff_with_markers` (with our
//!    `MARKER_*` constants from R2) against the deployed bytes.
//!    Apply the resulting diff to the template via diffy and emit
//!    the patched form. Conflict blocks land inline.
//!
//! No provider calls, ever. The whole point of caching
//! `tracked_render` in R1 is so this filter never re-renders — that
//! would re-trigger any `secret(...)` provider auth on every
//! `git status`, the auth-fatigue scenario magic.lex specifically
//! rules out.
//!
//! # Failure model
//!
//! Git treats filter exit codes other than 0 as fatal (the working-
//! tree read fails, blocking `git status` etc). We refuse to fail
//! the filter for anything except a hard I/O error: missing
//! baselines, decoding hiccups, and even malformed cached bytes
//! degrade to "echo stdin" with a stderr warning. Better the user
//! sees the unmodified template through git than their entire repo
//! becomes unreadable.

use burgertocow::{generate_diff_with_markers_opts, ConflictMarkers, DiffOptions, TrackedRender};
use diffy::Patch;
use std::io::{Read, Write};
use std::ops::Range;
use std::path::Path;

use crate::fs::Fs;
use crate::paths::Pather;
use crate::preprocessing::baseline::{hex_sha256, SecretsSidecar};
use crate::preprocessing::conflict::{MARKER_END, MARKER_MID, MARKER_START};
use crate::preprocessing::divergence::find_baseline_for_source;
use crate::preprocessing::no_reverse::is_no_reverse;
use crate::Result;

/// Produce the patched template content for one filter invocation.
///
/// `template_src` is the working-tree source bytes (what git passed
/// us on stdin). `source_path` is the absolute path of that file
/// (what git passed via `%f` and the CLI surfaced via `--path`).
/// `no_reverse_patterns` are the glob patterns from
/// `[preprocessor.template] no_reverse` for the source's pack;
/// matching files skip the slow path and echo stdin (still go through
/// the fast-path hash check, since equality is cheap and never produces
/// a misleading diff). Returns the bytes the filter should write to
/// stdout.
///
/// On any non-fatal hiccup (no baseline, hash mismatch, malformed
/// tracked render, diff parse failure, diff apply failure) returns
/// `template_src` unchanged. The caller (`template_clean_passthrough`
/// in the CLI) writes a stderr warning so the user sees the issue
/// without the filter aborting.
pub fn template_clean(
    fs: &dyn Fs,
    paths: &dyn Pather,
    template_src: &str,
    source_path: &Path,
    no_reverse_patterns: &[String],
) -> Result<String> {
    let Some((_pack, _handler, _filename, baseline)) =
        find_baseline_for_source(fs, paths, source_path)?
    else {
        // Source isn't tracked by dodot's preprocessing pipeline.
        // Echo unchanged. (git registered our filter for *.tmpl, but
        // a `.tmpl` file outside any pack would still hit us.)
        return Ok(template_src.to_string());
    };

    // Build the deployed path from the cache layout. find_baseline
    // already gave us the (pack, handler, filename) triple; we
    // re-derive the path here rather than hauling it through the
    // tuple destructure for clarity.
    let (pack, handler, filename, baseline) = (_pack, _handler, _filename, baseline);
    let deployed_path = paths
        .data_dir()
        .join("packs")
        .join(&pack)
        .join(&handler)
        .join(&filename);

    if !fs.exists(&deployed_path) {
        // Deployed file gone. Nothing to compare against → echo.
        return Ok(template_src.to_string());
    }
    let deployed_bytes = fs.read_file(&deployed_path)?;

    // ── Fast path ───────────────────────────────────────────────
    if hex_sha256(&deployed_bytes) == baseline.rendered_hash {
        return Ok(template_src.to_string());
    }

    // ── no_reverse opt-out ──────────────────────────────────────
    // The user has flagged this file as one where reverse-merge
    // produces more conflict markers than usable diffs. Skip the
    // slow path and echo stdin; `dodot transform status` still
    // surfaces the divergence so the user can decide what to do.
    if is_no_reverse(source_path, no_reverse_patterns) {
        return Ok(template_src.to_string());
    }

    // ── Slow path ───────────────────────────────────────────────
    if baseline.tracked_render.is_empty() {
        // Forward-compat: a baseline written before tracked_render
        // existed (or by a non-tracking preprocessor). We can't drive
        // burgertocow without the marker stream — echo and let
        // `dodot transform check` flag this on the next run.
        return Ok(template_src.to_string());
    }

    let tracked = TrackedRender::from_tracked_string(baseline.tracked_render.clone());
    let deployed_str = String::from_utf8_lossy(&deployed_bytes);
    let start = format!("{MARKER_START}\n");
    let mid = format!("\n{MARKER_MID}\n");
    let end = format!("\n{MARKER_END}\n");
    let markers = ConflictMarkers::new(&start, &mid, &end);
    // Per-render secrets sidecar: lines whose source-of-truth is a
    // vault must not participate in the clean filter's reverse-diff.
    // Without this, a rotated `{{ secret(...) }}` value in the
    // deployed file would land as a diff that rewrites the template
    // expression to the literal new value — defeating the
    // `secret(...)` abstraction. See `secrets.lex` §3.3 +
    // burgertocow#13. Absent sidecar = empty mask = byte-identical
    // to pre-Phase-S2 behavior.
    let secret_ranges = SecretsSidecar::load(fs, paths, &pack, &handler, &filename)?
        .map(|s| s.secret_line_ranges)
        .unwrap_or_default();
    let mask: Vec<Range<usize>> = secret_ranges.iter().map(|r| r.start..r.end).collect();
    let opts = DiffOptions::new(&markers).with_mask(&mask);
    let diff = generate_diff_with_markers_opts(template_src, &tracked, &deployed_str, &opts);

    if diff.is_empty() {
        // Pure-data edit (only variable values changed) — no
        // template-space change needed.
        return Ok(template_src.to_string());
    }

    if diff.starts_with(MARKER_START) {
        // Conflict block: burgertocow couldn't safely auto-merge.
        // The block carries the marker text as plain bytes that git
        // will surface via `git diff`. We splice it AFTER the
        // original source — exact placement doesn't matter for
        // git's purposes (any change shows up as a diff), and
        // appending leaves the user's editor view of the source
        // mostly intact, with the conflict block to resolve at the
        // bottom. We add a leading newline if needed so the block
        // sits on its own lines rather than concatenating with the
        // last line of the source.
        let mut out = template_src.to_string();
        if !out.ends_with('\n') {
            out.push('\n');
        }
        out.push_str(&diff);
        return Ok(out);
    }

    // Unified diff: apply via diffy.
    let patch = match Patch::from_str(&diff) {
        Ok(p) => p,
        Err(_) => return Ok(template_src.to_string()),
    };
    match diffy::apply(template_src, &patch) {
        Ok(patched) => Ok(patched),
        Err(_) => Ok(template_src.to_string()),
    }
}

/// Drive the clean filter as a stdin/stdout passthrough — what git
/// invokes when running the registered filter. Reads `stdin` to
/// `template_src`, calls [`template_clean`], writes the result to
/// `stdout`.
///
/// Per the module's failure model: any error from the inner
/// reverse-merge (cache read failure, hash mismatch, malformed
/// baseline) is caught here and we fall back to writing the
/// original stdin bytes to stdout, with a stderr warning so the
/// user sees the issue without git aborting. `Err` is reserved
/// strictly for stdin/stdout I/O failures, which are real
/// "filter genuinely cannot run" cases.
pub fn template_clean_stdio(
    fs: &dyn Fs,
    paths: &dyn Pather,
    source_path: &Path,
    no_reverse_patterns: &[String],
    stdin: &mut dyn Read,
    stdout: &mut dyn Write,
) -> Result<()> {
    let mut buf = Vec::new();
    stdin
        .read_to_end(&mut buf)
        .map_err(|e| crate::DodotError::Other(format!("template clean: stdin read: {e}")))?;
    let src = String::from_utf8_lossy(&buf).into_owned();
    let out = match template_clean(fs, paths, &src, source_path, no_reverse_patterns) {
        Ok(o) => o,
        Err(e) => {
            // Soft-fail: log to stderr (visible when the user runs
            // `git status` interactively, captured by CI logs) and
            // echo stdin so git doesn't abort the working-tree read.
            eprintln!(
                "dodot template clean: degraded to echo for {}: {e}",
                source_path.display()
            );
            src
        }
    };
    stdout
        .write_all(out.as_bytes())
        .map_err(|e| crate::DodotError::Other(format!("template clean: stdout write: {e}")))?;
    stdout
        .flush()
        .map_err(|e| crate::DodotError::Other(format!("template clean: stdout flush: {e}")))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::preprocessing::baseline::Baseline;
    use crate::testing::TempEnvironment;
    use burgertocow::Tracker;

    /// Render a template through burgertocow the way R1's pipeline
    /// does, so we get a tracked-render string that the filter will
    /// be able to rehydrate. Mirrors the test helper in the
    /// reverse_merge module.
    fn render(src: &str, ctx: serde_json::Value) -> (String, String) {
        let mut tracker = Tracker::new();
        tracker.add_template("t", src).unwrap();
        let tracked = tracker.render("t", &ctx).unwrap();
        (tracked.output().to_string(), tracked.tracked().to_string())
    }

    /// Stage a baseline + matching pack source + matching deployed
    /// file. Returns the absolute paths so the test can edit either
    /// side. Same shape as the helper in `commands::refresh::tests`.
    fn stage(
        env: &TempEnvironment,
        pack: &str,
        template_name: &str,
        template_body: &str,
        ctx: serde_json::Value,
    ) -> (std::path::PathBuf, std::path::PathBuf, String) {
        let src = env.dotfiles_root.join(pack).join(template_name);
        env.fs.mkdir_all(src.parent().unwrap()).unwrap();
        env.fs.write_file(&src, template_body.as_bytes()).unwrap();

        let stripped = template_name.strip_suffix(".tmpl").unwrap_or(template_name);
        let deployed = env
            .paths
            .data_dir()
            .join("packs")
            .join(pack)
            .join("preprocessed")
            .join(stripped);
        env.fs.mkdir_all(deployed.parent().unwrap()).unwrap();

        let (rendered, tracked) = render(template_body, ctx);
        env.fs.write_file(&deployed, rendered.as_bytes()).unwrap();

        let baseline = Baseline::build(
            &src,
            rendered.as_bytes(),
            template_body.as_bytes(),
            Some(&tracked),
            None,
        );
        baseline
            .write(
                env.fs.as_ref(),
                env.paths.as_ref(),
                pack,
                "preprocessed",
                stripped,
            )
            .unwrap();

        (src, deployed, rendered)
    }

    #[test]
    fn fast_path_echoes_stdin_when_deployed_matches_baseline() {
        // No edit on either side → deployed bytes hash to
        // baseline.rendered_hash → fast path hits → output equals
        // input verbatim. This is the common case (most git
        // operations don't follow a deployed-side edit).
        let env = TempEnvironment::builder().build();
        let template = "name = {{ name }}\nport = 5432\n";
        let (src, _deployed, _) = stage(
            &env,
            "app",
            "cfg.tmpl",
            template,
            serde_json::json!({"name": "Alice"}),
        );

        let out = template_clean(env.fs.as_ref(), env.paths.as_ref(), template, &src, &[]).unwrap();
        assert_eq!(out, template, "fast path must echo stdin verbatim");
    }

    #[test]
    fn slow_path_patches_static_line_edit() {
        // The user edited a static line in the deployed file. Slow
        // path runs burgertocow + diffy and produces the patched
        // template (var preserved, static line propagated).
        let env = TempEnvironment::builder().build();
        let template = "name = {{ name }}\nport = 5432\n";
        let (src, deployed, _) = stage(
            &env,
            "app",
            "cfg.tmpl",
            template,
            serde_json::json!({"name": "Alice"}),
        );

        env.fs
            .write_file(&deployed, b"name = Alice\nport = 9999\n")
            .unwrap();

        let out = template_clean(env.fs.as_ref(), env.paths.as_ref(), template, &src, &[]).unwrap();
        assert!(
            out.contains("port = 9999"),
            "expected patched static line, got: {out:?}"
        );
        assert!(
            out.contains("name = {{ name }}"),
            "var must survive, got: {out:?}"
        );
    }

    #[test]
    fn no_reverse_pattern_match_skips_slow_path() {
        // Same scenario as slow_path_patches_static_line_edit, but
        // with a no_reverse pattern matching the source filename.
        // The fast path's hash check fails (deployed differs), and
        // without the opt-out we'd fall into burgertocow + diffy and
        // emit the patched template. With the opt-out, we echo stdin.
        let env = TempEnvironment::builder().build();
        let template = "name = {{ name }}\nport = 5432\n";
        let (src, deployed, _) = stage(
            &env,
            "app",
            "cfg.tmpl",
            template,
            serde_json::json!({"name": "Alice"}),
        );

        env.fs
            .write_file(&deployed, b"name = Alice\nport = 9999\n")
            .unwrap();

        let out = template_clean(
            env.fs.as_ref(),
            env.paths.as_ref(),
            template,
            &src,
            &["cfg.tmpl".to_string()],
        )
        .unwrap();
        assert_eq!(
            out, template,
            "no_reverse match must echo stdin (no patched output)"
        );
    }

    #[test]
    fn no_reverse_glob_match_skips_slow_path() {
        // Glob form: `*.gen.tmpl` matches a deployed-edit on a
        // generated template, again echoing stdin instead of going
        // through reverse-merge.
        let env = TempEnvironment::builder().build();
        let template = "name = {{ name }}\nport = 5432\n";
        let (src, deployed, _) = stage(
            &env,
            "app",
            "foo.gen.tmpl",
            template,
            serde_json::json!({"name": "Alice"}),
        );
        env.fs
            .write_file(&deployed, b"name = Alice\nport = 9999\n")
            .unwrap();

        let out = template_clean(
            env.fs.as_ref(),
            env.paths.as_ref(),
            template,
            &src,
            &["*.gen.tmpl".to_string()],
        )
        .unwrap();
        assert_eq!(out, template);
    }

    #[test]
    fn no_reverse_does_not_block_fast_path() {
        // Even with a no_reverse match, the fast-path hash check
        // still runs first. A clean state echoes stdin (same as
        // without the opt-out) — the opt-out only affects the slow
        // path's reverse-merge step.
        let env = TempEnvironment::builder().build();
        let template = "name = {{ name }}\n";
        let (src, _deployed, _) = stage(
            &env,
            "app",
            "cfg.tmpl",
            template,
            serde_json::json!({"name": "Alice"}),
        );

        let out = template_clean(
            env.fs.as_ref(),
            env.paths.as_ref(),
            template,
            &src,
            &["cfg.tmpl".to_string()],
        )
        .unwrap();
        assert_eq!(out, template);
    }

    #[test]
    fn slow_path_pure_data_edit_echoes_stdin() {
        // The user changed only a variable value in the deployed
        // file. burgertocow returns an empty diff (pure-data edit);
        // we echo stdin unchanged.
        let env = TempEnvironment::builder().build();
        let template = "name = {{ name }}\n";
        let (src, deployed, _) = stage(
            &env,
            "app",
            "cfg.tmpl",
            template,
            serde_json::json!({"name": "Alice"}),
        );

        env.fs.write_file(&deployed, b"name = Bob\n").unwrap();

        let out = template_clean(env.fs.as_ref(), env.paths.as_ref(), template, &src, &[]).unwrap();
        assert_eq!(out, template);
    }

    #[test]
    fn slow_path_conflict_appends_marker_block_to_template() {
        // Inconsistent loop edits → burgertocow returns a conflict
        // block (starting with our MARKER_START). The filter splices
        // it after the original template so `git diff` shows both
        // the unchanged template and the conflict block.
        let env = TempEnvironment::builder().build();
        let template = "{% for i in items %}- {{ i }}\n{% endfor %}";
        let (src, deployed, _) = stage(
            &env,
            "app",
            "list.tmpl",
            template,
            serde_json::json!({"items": ["a", "b", "c"]}),
        );
        env.fs.write_file(&deployed, b"* a\n+ b\n- c\n").unwrap();

        let out = template_clean(env.fs.as_ref(), env.paths.as_ref(), template, &src, &[]).unwrap();
        // Original template still present.
        assert!(
            out.contains("{% for i in items %}"),
            "original template must be retained: {out:?}"
        );
        // Conflict block appended.
        assert!(
            out.contains(MARKER_START),
            "conflict block missing: {out:?}"
        );
        assert!(
            out.contains(MARKER_END),
            "conflict block missing end: {out:?}"
        );
    }

    #[test]
    fn unknown_source_path_echoes_stdin() {
        // Path the cache has never seen → echo. (Defensive: git's
        // .gitattributes might match a .tmpl file in some sub-tree
        // that isn't a dodot-managed pack.)
        let env = TempEnvironment::builder().build();
        let stranger = env.dotfiles_root.join("not-a-pack/random.tmpl");
        env.fs.mkdir_all(stranger.parent().unwrap()).unwrap();
        let body = "hello {{ x }}\n";
        env.fs.write_file(&stranger, body.as_bytes()).unwrap();

        let out =
            template_clean(env.fs.as_ref(), env.paths.as_ref(), body, &stranger, &[]).unwrap();
        assert_eq!(out, body);
    }

    #[test]
    fn missing_deployed_echoes_stdin() {
        // Baseline exists but deployed file was deleted. Nothing to
        // compare against → echo. (`dodot transform check` will
        // surface the MissingDeployed state on its own pass.)
        let env = TempEnvironment::builder().build();
        let template = "name = {{ name }}\n";
        let (src, deployed, _) = stage(
            &env,
            "app",
            "cfg.tmpl",
            template,
            serde_json::json!({"name": "Alice"}),
        );
        env.fs.remove_file(&deployed).unwrap();

        let out = template_clean(env.fs.as_ref(), env.paths.as_ref(), template, &src, &[]).unwrap();
        assert_eq!(out, template);
    }

    #[test]
    fn empty_tracked_render_falls_back_to_echo() {
        // Forward-compat: a baseline whose tracked_render is empty
        // (v1 baseline before the field existed, or future non-
        // tracking preprocessor) — we can't drive burgertocow.
        // Echo and let `dodot transform check` surface the issue
        // when it next runs.
        let env = TempEnvironment::builder().build();
        let template = "name = {{ name }}\n";
        let src = env.dotfiles_root.join("app/cfg.tmpl");
        env.fs.mkdir_all(src.parent().unwrap()).unwrap();
        env.fs.write_file(&src, template.as_bytes()).unwrap();

        let deployed = env.paths.data_dir().join("packs/app/preprocessed/cfg");
        env.fs.mkdir_all(deployed.parent().unwrap()).unwrap();
        env.fs.write_file(&deployed, b"name = EDITED\n").unwrap();

        // Baseline with an empty tracked_render.
        let baseline = Baseline::build(&src, b"name = Alice\n", template.as_bytes(), None, None);
        baseline
            .write(
                env.fs.as_ref(),
                env.paths.as_ref(),
                "app",
                "preprocessed",
                "cfg",
            )
            .unwrap();

        let out = template_clean(env.fs.as_ref(), env.paths.as_ref(), template, &src, &[]).unwrap();
        assert_eq!(out, template);
    }

    #[test]
    fn stdio_passthrough_writes_filter_output_to_stdout() {
        // Pin the stdin/stdout wiring: same fast-path scenario as
        // the first test, but exercised through the I/O surface git
        // will use. Confirms read_to_end / write_all / flush all
        // succeed and the output matches.
        let env = TempEnvironment::builder().build();
        let template = "name = {{ name }}\n";
        let (src, _deployed, _) = stage(
            &env,
            "app",
            "cfg.tmpl",
            template,
            serde_json::json!({"name": "Alice"}),
        );

        let mut stdin = std::io::Cursor::new(template.as_bytes().to_vec());
        let mut stdout: Vec<u8> = Vec::new();
        template_clean_stdio(
            env.fs.as_ref(),
            env.paths.as_ref(),
            &src,
            &[],
            &mut stdin,
            &mut stdout,
        )
        .unwrap();
        assert_eq!(stdout, template.as_bytes());
    }

    #[test]
    fn stdio_soft_fails_when_inner_clean_errors() {
        // Pin the documented failure model: if the inner
        // template_clean returns an Err, the stdio wrapper must
        // catch it and echo stdin rather than propagate. Git
        // treats filter exit != 0 as fatal — a template-source
        // anywhere in the repo with a transient cache I/O error
        // would otherwise brick `git status`.
        //
        // We force the error path by pointing at a baseline whose
        // backing JSON is corrupt — Baseline::load returns Err on
        // parse failure, which collect_baselines surfaces, which
        // find_baseline_for_source propagates. With the soft-fail
        // wrapper, the user gets stdin echoed instead.
        let env = TempEnvironment::builder().build();
        let src = env.dotfiles_root.join("app/cfg.tmpl");
        env.fs.mkdir_all(src.parent().unwrap()).unwrap();
        let template = "name = {{ name }}\n";
        env.fs.write_file(&src, template.as_bytes()).unwrap();

        // Lay down a corrupt baseline JSON (parse failure on load).
        let cache_path = env
            .paths
            .preprocessor_baseline_path("app", "preprocessed", "cfg");
        env.fs.mkdir_all(cache_path.parent().unwrap()).unwrap();
        env.fs.write_file(&cache_path, b"{not json").unwrap();

        let mut stdin = std::io::Cursor::new(template.as_bytes().to_vec());
        let mut stdout: Vec<u8> = Vec::new();
        // Must succeed — the inner Err is swallowed.
        template_clean_stdio(
            env.fs.as_ref(),
            env.paths.as_ref(),
            &src,
            &[],
            &mut stdin,
            &mut stdout,
        )
        .expect("stdio must soft-fail to echo, not propagate Err");
        // And the echoed bytes must equal the input verbatim.
        assert_eq!(stdout, template.as_bytes());
    }

    #[test]
    fn filter_never_fails_on_baseline_disagreement() {
        // Pin the soft-fail contract: the filter must never return
        // an Err for "logically wrong" inputs (mismatched hashes,
        // malformed cache, etc) — only for I/O failures. Git treats
        // any non-zero exit as fatal and the working tree becomes
        // unreadable.
        //
        // Construct a baseline whose rendered_hash claims one thing
        // but whose tracked_render is internally inconsistent.
        // burgertocow will produce *something* (probably an empty
        // diff or a conflict); whatever it does, we must succeed.
        let env = TempEnvironment::builder().build();
        let template = "name = {{ name }}\n";
        let src = env.dotfiles_root.join("app/cfg.tmpl");
        env.fs.mkdir_all(src.parent().unwrap()).unwrap();
        env.fs.write_file(&src, template.as_bytes()).unwrap();

        let deployed = env.paths.data_dir().join("packs/app/preprocessed/cfg");
        env.fs.mkdir_all(deployed.parent().unwrap()).unwrap();
        env.fs
            .write_file(&deployed, b"unrelated content\n")
            .unwrap();

        // Baseline with a tracked_render that doesn't correspond to
        // the template — burgertocow will see them as totally
        // different.
        let baseline = Baseline::build(
            &src,
            b"different",
            template.as_bytes(),
            Some("\u{1e}wrong\u{1f}"),
            None,
        );
        baseline
            .write(
                env.fs.as_ref(),
                env.paths.as_ref(),
                "app",
                "preprocessed",
                "cfg",
            )
            .unwrap();

        // Must succeed (output may be anything; just not a panic or
        // an Err).
        let _ = template_clean(env.fs.as_ref(), env.paths.as_ref(), template, &src, &[]).unwrap();
    }
}
