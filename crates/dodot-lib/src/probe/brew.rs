//! Homebrew-cask probe — advisory lookup of cask metadata.
//!
//! Implements Phase M6 of `docs/proposals/macos-paths.lex` §8.2. The
//! cardinal rule from §8 holds: probes are *advisory*, never
//! authoritative. The symlink resolver in §5 never consults this
//! module, and a probe failure (no `brew` on PATH, malformed JSON,
//! cache miss) never alters routing — it just means the user sees a
//! less-rich suggestion or warning.
//!
//! ## What the probe surfaces
//!
//! Two shells over `brew`:
//!
//! - [`list_installed_casks`] → `brew list --cask --versions`. Cheap.
//!   Used to short-circuit `brew info` when a token isn't installed.
//! - [`info_cask`] → `brew info --json=v2 --cask <token>`. Expensive
//!   the first time (network), so we cache it on disk.
//!
//! Plus zap-stanza parsing from the JSON: app-folder candidates,
//! Application Support entries, and Preferences plists for
//! sibling-adoption suggestions in `dodot adopt`.
//!
//! ## Cache layout
//!
//! `<cache_dir>/probes/brew/<token>.json` carries:
//!
//! ```json
//! { "fetched_at": <unix_ts>, "info": { ...brew info JSON... } }
//! ```
//!
//! Entries older than [`CACHE_TTL_SECS`] are treated as stale and
//! re-fetched. `dodot probe app --refresh` blows the cache for that
//! pack's tokens.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::datastore::CommandRunner;
use crate::fs::Fs;
use crate::Result;

/// Cache lifetime in seconds. 24 hours: brew zap data changes rarely
/// enough that a daily refresh is plenty, and the cost of a
/// `brew info` call is high enough to want the cache hit.
pub const CACHE_TTL_SECS: u64 = 24 * 60 * 60;

/// Minimal subset of `brew info --json=v2 --cask <token>` we read.
///
/// The full JSON shape is large and brew owns it; we deserialise only
/// the fields the proposal calls out and tolerate everything else via
/// `#[serde(default)]` so a brew schema bump doesn't break the probe.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CaskInfo {
    /// Cask token (e.g. `"visual-studio-code"`).
    #[serde(default)]
    pub token: String,
    /// Human-readable display name (`"Visual Studio Code"`).
    #[serde(default)]
    pub name: Vec<String>,
    /// Bundle filenames declared by the cask's `app` artifact (e.g.
    /// `["Visual Studio Code.app"]`).
    #[serde(default)]
    pub artifacts: Vec<serde_json::Value>,
    /// Whether the cask is currently installed locally. The brew JSON
    /// reports this via the `installed` field on each cask entry.
    #[serde(default)]
    pub installed: Option<String>,
}

impl CaskInfo {
    /// Extract leaf names of `~/Library/Application Support/<X>` paths
    /// declared in the cask's zap stanza. Each is a candidate
    /// app-support folder name for matching against an `_app/<X>/`
    /// pack entry.
    pub fn app_support_candidates(&self) -> Vec<String> {
        zap_paths(&self.artifacts)
            .filter_map(|p| {
                let needle = "Library/Application Support/";
                let idx = p.find(needle)?;
                let rest = &p[idx + needle.len()..];
                let leaf = rest.split('/').next()?.trim();
                if leaf.is_empty() {
                    None
                } else {
                    Some(leaf.to_string())
                }
            })
            .collect()
    }

    /// Preferences plist paths declared in the zap stanza. Used by
    /// `dodot adopt` to suggest sibling adoptions
    /// (`~/Library/Preferences/<bundle-id>.plist`).
    pub fn preferences_plists(&self) -> Vec<String> {
        zap_paths(&self.artifacts)
            .filter(|p| p.contains("Library/Preferences/"))
            .collect()
    }

    /// `.app` bundle leaf name from the cask's `app` artifact, e.g.
    /// `"Visual Studio Code.app"`. Used to drive `mdls` lookups.
    pub fn app_bundle_name(&self) -> Option<String> {
        for artifact in &self.artifacts {
            if let Some(arr) = artifact.get("app").and_then(|v| v.as_array()) {
                if let Some(first) = arr.first().and_then(|v| v.as_str()) {
                    return Some(first.to_string());
                }
            }
        }
        None
    }
}

/// Iterate every string path declared anywhere in any cask `zap`
/// stanza. Brew's JSON nests these under `artifacts[].zap[].trash`
/// and similar arrays; we walk the JSON tree generically rather than
/// pinning the exact path so a schema tweak doesn't bite us.
fn zap_paths(artifacts: &[serde_json::Value]) -> impl Iterator<Item = String> + '_ {
    artifacts.iter().flat_map(|art| {
        let mut out: Vec<String> = Vec::new();
        if let Some(zap) = art.get("zap") {
            walk_strings(zap, &mut out);
        }
        out.into_iter()
    })
}

fn walk_strings(v: &serde_json::Value, out: &mut Vec<String>) {
    match v {
        serde_json::Value::String(s) => out.push(s.clone()),
        serde_json::Value::Array(a) => {
            for child in a {
                walk_strings(child, out);
            }
        }
        serde_json::Value::Object(map) => {
            for child in map.values() {
                walk_strings(child, out);
            }
        }
        _ => {}
    }
}

/// On-disk cache wrapper around [`CaskInfo`].
#[derive(Debug, Clone, Serialize, Deserialize)]
struct CacheEntry {
    fetched_at: u64,
    info: CaskInfo,
}

/// Run `brew list --cask --versions` and return the set of installed
/// cask tokens. Empty on non-macOS or when `brew` isn't on PATH.
///
/// This is intentionally lossy: any error path returns an empty set,
/// not a `Result::Err`. The probe is advisory — a failure to enumerate
/// installed casks must never block adopt or up.
pub fn list_installed_casks(runner: &dyn CommandRunner) -> Vec<String> {
    if !cfg!(target_os = "macos") {
        return Vec::new();
    }
    let output = match runner.run(
        "brew",
        &["list".into(), "--cask".into(), "--versions".into()],
    ) {
        Ok(o) if o.exit_code == 0 => o,
        _ => return Vec::new(),
    };
    output
        .stdout
        .lines()
        .filter_map(|line| line.split_whitespace().next().map(str::to_string))
        .collect()
}

/// Look up `brew info --json=v2 --cask <token>` with on-disk caching.
///
/// `now_secs` is the wall-clock unix timestamp the caller considers
/// "now" for TTL evaluation; passing it in keeps the cache testable
/// without a clock dependency. Production callers pass
/// `SystemTime::now()` via [`now_secs_unix`].
///
/// Returns `Ok(None)` when the cask is unknown to brew, or when we're
/// running on a host without `brew`. Returns `Ok(Some(_))` for a
/// fresh cache hit, a stale-entry refresh, or a successful first
/// fetch.
///
/// Cache writes are best-effort — a non-writable cache dir downgrades
/// the probe to "no caching" but doesn't propagate the error.
pub fn info_cask(
    token: &str,
    cache_dir: &Path,
    now_secs: u64,
    fs: &dyn Fs,
    runner: &dyn CommandRunner,
) -> Result<Option<CaskInfo>> {
    if !cfg!(target_os = "macos") {
        return Ok(None);
    }
    let cache_path = cache_path_for(cache_dir, token);
    if let Some(entry) = read_cache(&cache_path, fs) {
        if now_secs.saturating_sub(entry.fetched_at) < CACHE_TTL_SECS {
            return Ok(Some(entry.info));
        }
    }

    let info = match fetch_from_brew(token, runner) {
        Some(i) => i,
        None => return Ok(None),
    };

    let entry = CacheEntry {
        fetched_at: now_secs,
        info: info.clone(),
    };
    let _ = write_cache(&cache_path, &entry, fs);
    Ok(Some(info))
}

/// Force a refresh by deleting any cached entry for `token`. Errors
/// are swallowed — the cache is best-effort.
pub fn invalidate_cache(token: &str, cache_dir: &Path, fs: &dyn Fs) {
    let path = cache_path_for(cache_dir, token);
    if fs.exists(&path) {
        let _ = fs.remove_file(&path);
    }
}

fn cache_path_for(cache_dir: &Path, token: &str) -> PathBuf {
    // Token can contain `/` in theory (formula taps); flatten to a safe
    // filename. Brew cask tokens in practice are kebab-case ASCII, but
    // a defensive replace keeps the cache key total.
    let safe = token.replace(['/', '\\', ':', ' '], "_");
    cache_dir.join(format!("{safe}.json"))
}

fn read_cache(path: &Path, fs: &dyn Fs) -> Option<CacheEntry> {
    if !fs.exists(path) {
        return None;
    }
    let bytes = fs.read_to_string(path).ok()?;
    serde_json::from_str(&bytes).ok()
}

fn write_cache(path: &Path, entry: &CacheEntry, fs: &dyn Fs) -> Result<()> {
    if let Some(parent) = path.parent() {
        if !fs.exists(parent) {
            fs.mkdir_all(parent)?;
        }
    }
    let json = serde_json::to_string(entry)
        .map_err(|e| crate::DodotError::Other(format!("brew cache encode failed: {e}")))?;
    fs.write_file(path, json.as_bytes())?;
    Ok(())
}

fn fetch_from_brew(token: &str, runner: &dyn CommandRunner) -> Option<CaskInfo> {
    let output = runner
        .run(
            "brew",
            &[
                "info".into(),
                "--json=v2".into(),
                "--cask".into(),
                token.to_string(),
            ],
        )
        .ok()?;
    if output.exit_code != 0 {
        return None;
    }
    parse_info_json(&output.stdout)
}

/// Parse a `brew info --json=v2 --cask` payload, returning the first
/// cask entry (brew nests them under `casks: []`).
fn parse_info_json(stdout: &str) -> Option<CaskInfo> {
    #[derive(Deserialize)]
    struct Wrapper {
        #[serde(default)]
        casks: Vec<CaskInfo>,
    }
    let w: Wrapper = serde_json::from_str(stdout).ok()?;
    w.casks.into_iter().next()
}

/// Wall-clock unix timestamp helper used by callers that want the
/// real current time. Tests pass a fixed value so cache TTL is
/// deterministic.
pub fn now_secs_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Outcome of a folder-to-cask matching pass.
///
/// `installed_tokens` carries the result of `brew list --cask
/// --versions` so callers don't re-spawn the same subprocess for it.
/// `folder_to_token` is the actual matches: each entry pairs a
/// pack-relative folder name with the cask token that declares it in
/// its zap stanza.
#[derive(Debug, Clone, Default)]
pub struct InstalledCaskMatches {
    pub installed_tokens: Vec<String>,
    pub folder_to_token: HashMap<String, String>,
}

/// Match `app_aliases` map values + every pack-relative `_app/<X>/...`
/// folder against installed casks' Application Support candidates.
///
/// **Installed-only** — this iterates only the tokens
/// `brew list --cask --versions` reports. A cask the user hasn't
/// installed will never appear here. The name reflects that. If you
/// need broader matching, you'd have to drive it off some other
/// source (zap data isn't available for non-installed casks without
/// further `brew info` calls per known token).
///
/// `cache_only` controls whether a cache miss triggers a fresh
/// `brew info --json=v2 --cask <token>` subprocess: callers on a hot
/// path (planner hints during `up`/`status`) pass `true` so a stale
/// cache silently degrades to "no enrichment" rather than spawning
/// dozens of subprocesses; the on-demand `dodot probe app` subcommand
/// passes `false` to populate the cache fully.
pub fn match_folders_to_installed_casks(
    folders: &[String],
    runner: &dyn CommandRunner,
    cache_dir: &Path,
    now_secs: u64,
    fs: &dyn Fs,
    cache_only: bool,
) -> InstalledCaskMatches {
    let mut out = InstalledCaskMatches::default();
    if !cfg!(target_os = "macos") {
        return out;
    }
    out.installed_tokens = list_installed_casks(runner);
    for token in &out.installed_tokens {
        let info = if cache_only {
            // Cache-only mode: read the on-disk entry if fresh, never
            // spawn `brew info`. A miss leaves this token unmatched.
            read_cache(&cache_path_for(cache_dir, token), fs)
                .filter(|e| now_secs.saturating_sub(e.fetched_at) < CACHE_TTL_SECS)
                .map(|e| e.info)
        } else {
            info_cask(token, cache_dir, now_secs, fs, runner)
                .ok()
                .flatten()
        };
        if let Some(info) = info {
            for cand in info.app_support_candidates() {
                if folders.iter().any(|f| f == &cand) {
                    out.folder_to_token.insert(cand, token.clone());
                }
            }
        }
    }
    out
}

/// Best-effort wipe of the entire brew probe cache directory.
///
/// `dodot probe app --refresh` calls this so the user's "I want
/// fresh data" gesture isn't bottlenecked by per-token invalidation
/// requiring the caller to know which tokens to invalidate (which
/// they wouldn't, before matching).
pub fn invalidate_all_cache(cache_dir: &Path, fs: &dyn Fs) {
    if !fs.exists(cache_dir) {
        return;
    }
    if let Ok(entries) = fs.read_dir(cache_dir) {
        for entry in entries {
            if entry.name.ends_with(".json") {
                let _ = fs.remove_file(&entry.path);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::datastore::CommandOutput;
    use std::sync::Mutex;

    /// CommandRunner mock that returns canned outputs per command.
    struct MockRunner {
        responses: Mutex<HashMap<Vec<String>, CommandOutput>>,
        calls: Mutex<Vec<Vec<String>>>,
    }

    impl MockRunner {
        fn new() -> Self {
            Self {
                responses: Mutex::new(HashMap::new()),
                calls: Mutex::new(Vec::new()),
            }
        }
        fn respond(&self, args: &[&str], stdout: &str, exit_code: i32) {
            let key: Vec<String> = args.iter().map(|s| s.to_string()).collect();
            self.responses.lock().unwrap().insert(
                key,
                CommandOutput {
                    exit_code,
                    stdout: stdout.into(),
                    stderr: String::new(),
                },
            );
        }
        fn call_count(&self, args: &[&str]) -> usize {
            let key: Vec<String> = args.iter().map(|s| s.to_string()).collect();
            self.calls
                .lock()
                .unwrap()
                .iter()
                .filter(|c| **c == key)
                .count()
        }
    }

    impl CommandRunner for MockRunner {
        fn run(&self, exe: &str, args: &[String]) -> Result<CommandOutput> {
            let mut full = vec![exe.to_string()];
            full.extend(args.iter().cloned());
            self.calls.lock().unwrap().push(full.clone());
            // Strip the executable prefix for response lookup so test
            // fixtures stay readable.
            let key: Vec<String> = full.iter().skip(1).cloned().collect();
            self.responses
                .lock()
                .unwrap()
                .get(&key)
                .cloned()
                .ok_or_else(|| crate::DodotError::Other(format!("no mock response for {full:?}")))
        }
    }

    fn make_env() -> (crate::testing::TempEnvironment, std::path::PathBuf) {
        let env = crate::testing::TempEnvironment::builder().build();
        let cache = env.home.join("brew-probe-cache");
        env.fs.mkdir_all(&cache).unwrap();
        (env, cache)
    }

    #[test]
    fn parse_info_json_extracts_first_cask() {
        let payload = r#"{
            "casks": [
                {
                    "token": "visual-studio-code",
                    "name": ["Visual Studio Code"],
                    "installed": "1.95.0",
                    "artifacts": [
                        {"app": ["Visual Studio Code.app"]},
                        {"zap": [
                            {"trash": [
                                "~/Library/Application Support/Code",
                                "~/Library/Preferences/com.microsoft.VSCode.plist"
                            ]}
                        ]}
                    ]
                }
            ]
        }"#;
        let info = parse_info_json(payload).expect("parse");
        assert_eq!(info.token, "visual-studio-code");
        assert_eq!(info.installed.as_deref(), Some("1.95.0"));
        assert_eq!(
            info.app_bundle_name().as_deref(),
            Some("Visual Studio Code.app")
        );
        let candidates = info.app_support_candidates();
        assert!(candidates.iter().any(|c| c == "Code"), "got {candidates:?}");
        let plists = info.preferences_plists();
        assert!(
            plists
                .iter()
                .any(|p| p.contains("com.microsoft.VSCode.plist")),
            "got {plists:?}"
        );
    }

    #[test]
    fn parse_info_json_missing_casks_array_returns_none() {
        assert!(parse_info_json("{}").is_none());
        assert!(parse_info_json("not json").is_none());
    }

    #[test]
    #[cfg_attr(not(target_os = "macos"), ignore = "macOS-only behavior")]
    fn info_cask_caches_first_result_then_serves_from_cache() {
        let (env, cache) = make_env();
        let runner = MockRunner::new();
        runner.respond(
            &["info", "--json=v2", "--cask", "visual-studio-code"],
            r#"{"casks": [{"token": "visual-studio-code", "installed": "1.95.0"}]}"#,
            0,
        );

        let now = 1_000_000;
        let first = info_cask("visual-studio-code", &cache, now, env.fs.as_ref(), &runner)
            .unwrap()
            .expect("first call returns Some");
        assert_eq!(first.token, "visual-studio-code");
        assert_eq!(
            runner.call_count(&["brew", "info", "--json=v2", "--cask", "visual-studio-code"]),
            1
        );

        // Within TTL → cache hit, no second subprocess.
        let _ = info_cask(
            "visual-studio-code",
            &cache,
            now + 100,
            env.fs.as_ref(),
            &runner,
        )
        .unwrap();
        assert_eq!(
            runner.call_count(&["brew", "info", "--json=v2", "--cask", "visual-studio-code"]),
            1,
            "fresh cache must not re-fetch"
        );
    }

    #[test]
    #[cfg_attr(not(target_os = "macos"), ignore = "macOS-only behavior")]
    fn info_cask_refetches_when_ttl_expires() {
        let (env, cache) = make_env();
        let runner = MockRunner::new();
        runner.respond(
            &["info", "--json=v2", "--cask", "cursor"],
            r#"{"casks": [{"token": "cursor"}]}"#,
            0,
        );

        let now = 1_000_000;
        let _ = info_cask("cursor", &cache, now, env.fs.as_ref(), &runner).unwrap();
        // Simulate clock advance past TTL.
        let _ = info_cask(
            "cursor",
            &cache,
            now + CACHE_TTL_SECS + 1,
            env.fs.as_ref(),
            &runner,
        )
        .unwrap();
        assert_eq!(
            runner.call_count(&["brew", "info", "--json=v2", "--cask", "cursor"]),
            2,
            "stale cache should re-fetch"
        );
    }

    #[test]
    #[cfg_attr(not(target_os = "macos"), ignore = "macOS-only behavior")]
    fn invalidate_cache_forces_refetch() {
        let (env, cache) = make_env();
        let runner = MockRunner::new();
        runner.respond(
            &["info", "--json=v2", "--cask", "zed"],
            r#"{"casks": [{"token": "zed"}]}"#,
            0,
        );

        let now = 1_000_000;
        let _ = info_cask("zed", &cache, now, env.fs.as_ref(), &runner).unwrap();
        invalidate_cache("zed", &cache, env.fs.as_ref());
        let _ = info_cask("zed", &cache, now + 10, env.fs.as_ref(), &runner).unwrap();
        assert_eq!(
            runner.call_count(&["brew", "info", "--json=v2", "--cask", "zed"]),
            2
        );
    }

    #[test]
    #[cfg_attr(not(target_os = "macos"), ignore = "macOS-only behavior")]
    fn info_cask_returns_none_on_brew_failure() {
        let (env, cache) = make_env();
        let runner = MockRunner::new();
        runner.respond(
            &["info", "--json=v2", "--cask", "nonexistent"],
            "",
            1, // brew exits non-zero on unknown cask
        );
        let got = info_cask("nonexistent", &cache, 100, env.fs.as_ref(), &runner).unwrap();
        assert!(got.is_none());
    }

    #[test]
    #[cfg_attr(not(target_os = "macos"), ignore = "macOS-only behavior")]
    fn list_installed_casks_parses_first_column() {
        let runner = MockRunner::new();
        runner.respond(
            &["list", "--cask", "--versions"],
            "visual-studio-code 1.95.0\ncursor 0.42.0\nzed 0.150.0\n",
            0,
        );
        let got = list_installed_casks(&runner);
        assert_eq!(got, vec!["visual-studio-code", "cursor", "zed"]);
    }

    #[test]
    fn list_installed_casks_silent_on_non_macos() {
        // The function is gated by cfg!(target_os = "macos"). On every
        // other host it returns empty regardless of mock state.
        let runner = MockRunner::new();
        let got = list_installed_casks(&runner);
        if !cfg!(target_os = "macos") {
            assert!(got.is_empty());
        }
        // On macOS hosts we'd hit the "no mock response" path which
        // returns Err → empty Vec via the function's loss-tolerant
        // outer match. Still expect no panic either way.
    }

    #[test]
    #[cfg_attr(not(target_os = "macos"), ignore = "macOS-only behavior")]
    fn match_folders_cache_only_skips_brew_info_on_miss() {
        // With cache_only=true and an empty cache, the matcher must
        // not call `brew info` — the planner path uses this mode to
        // keep `up`/`status` fast.
        let (env, cache) = make_env();
        let runner = MockRunner::new();
        runner.respond(
            &["list", "--cask", "--versions"],
            "visual-studio-code 1.95.0\n",
            0,
        );
        // No brew info response registered: a call would fail
        // CannedRunner's lookup → propagating to info_cask returning
        // None silently. We assert the call count remains 0.

        let now = 1_000_000;
        let result = match_folders_to_installed_casks(
            &["Code".into()],
            &runner,
            &cache,
            now,
            env.fs.as_ref(),
            /*cache_only=*/ true,
        );
        // Installed list still populated (brew list was called).
        assert!(result
            .installed_tokens
            .contains(&"visual-studio-code".into()));
        // No info → no folder match.
        assert!(result.folder_to_token.is_empty());
        // And brew info was never invoked.
        assert_eq!(
            runner.call_count(&["brew", "info", "--json=v2", "--cask", "visual-studio-code"]),
            0,
            "cache_only=true must not spawn brew info"
        );
    }

    #[test]
    #[cfg_attr(not(target_os = "macos"), ignore = "macOS-only behavior")]
    fn invalidate_all_cache_clears_every_token() {
        let (env, cache) = make_env();
        let runner = MockRunner::new();
        runner.respond(
            &["info", "--json=v2", "--cask", "alpha"],
            r#"{"casks": [{"token": "alpha"}]}"#,
            0,
        );
        runner.respond(
            &["info", "--json=v2", "--cask", "beta"],
            r#"{"casks": [{"token": "beta"}]}"#,
            0,
        );
        let now = 1_000_000;
        let _ = info_cask("alpha", &cache, now, env.fs.as_ref(), &runner).unwrap();
        let _ = info_cask("beta", &cache, now, env.fs.as_ref(), &runner).unwrap();
        assert!(env.fs.exists(&cache.join("alpha.json")));
        assert!(env.fs.exists(&cache.join("beta.json")));

        invalidate_all_cache(&cache, env.fs.as_ref());
        assert!(!env.fs.exists(&cache.join("alpha.json")));
        assert!(!env.fs.exists(&cache.join("beta.json")));
    }

    #[test]
    fn cache_path_sanitizes_token() {
        // Hypothetical token containing path separators — brew tokens
        // don't actually do this today but the sanitization keeps the
        // cache key total. Path component check is what matters: no
        // entry should resolve outside the cache dir via `..`.
        use std::path::Component;
        let cache = Path::new("/tmp/brew-cache");
        let p = cache_path_for(cache, "evil/../token");
        assert!(p.starts_with(cache));
        let escapes = p.components().any(|c| matches!(c, Component::ParentDir));
        assert!(
            !escapes,
            "sanitized path must not contain a ParentDir component: {}",
            p.display()
        );
    }
}
