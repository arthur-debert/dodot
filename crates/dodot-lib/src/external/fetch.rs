//! HTTP-fetcher abstraction for externals.
//!
//! The trait exists so tests don't have to spin up a real HTTP server.
//! Production uses [`UreqFetcher`] (which also handles `file://` URLs
//! so tests can point at fixture files directly).

use std::fs;
use std::io::Read;

/// Error category returned by a fetcher.
///
/// The executor distinguishes these so soft network failures don't
/// kill the whole `up` invocation — only integrity failures do.
#[derive(Debug, thiserror::Error)]
pub enum HttpFetchError {
    /// Bad URL (parse error, unsupported scheme).
    #[error("invalid URL: {0}")]
    InvalidUrl(String),
    /// Network reachable but server returned non-2xx.
    #[error("HTTP {status}: {url}")]
    Status { url: String, status: u16 },
    /// Network unreachable / DNS / timeout / I/O.
    #[error("network error fetching {url}: {source}")]
    Network {
        url: String,
        #[source]
        source: Box<dyn std::error::Error + Send + Sync>,
    },
}

impl HttpFetchError {
    /// Was this a transient failure that should soft-fail (use
    /// cached content if any) rather than abort `up`?
    pub fn is_transient(&self) -> bool {
        matches!(self, Self::Network { .. })
    }
}

pub trait HttpFetcher: Send + Sync {
    /// Fetch the entire body at `url` into memory.
    fn fetch(&self, url: &str) -> std::result::Result<Vec<u8>, HttpFetchError>;
}

/// Default fetcher: ureq for http(s), direct fs read for `file://`.
///
/// `file://` support exists so the test suite can drive the executor
/// end-to-end without standing up an HTTP server; it's also useful for
/// users who want to pull from a local mirror.
///
/// Timeouts: the underlying agent is configured with explicit
/// connect / read / overall-call deadlines so a stalled remote can't
/// hang `dodot up` indefinitely. On a plane or behind a dead
/// captive-portal these fail predictably and the executor's
/// soft-fail path takes over (cached content stays in place).
pub struct UreqFetcher {
    agent: ureq::Agent,
}

impl UreqFetcher {
    pub fn new() -> Self {
        // Picked to favour failing-fast over edge cases:
        // - 5 s to open a TCP/TLS connection (DNS + handshake).
        // - 20 s on a single read (servers that dribble bytes).
        // - 60 s total wall-clock cap on the whole call (last-resort).
        // Externals are typically small files / small clones, so
        // these aren't tight enough to bite real-world fetches but
        // they're tight enough to keep `up` snappy when the network
        // is unreachable.
        let agent = ureq::AgentBuilder::new()
            .timeout_connect(std::time::Duration::from_secs(5))
            .timeout_read(std::time::Duration::from_secs(20))
            .timeout(std::time::Duration::from_secs(60))
            .build();
        Self { agent }
    }
}

impl Default for UreqFetcher {
    fn default() -> Self {
        Self::new()
    }
}

impl HttpFetcher for UreqFetcher {
    fn fetch(&self, url: &str) -> std::result::Result<Vec<u8>, HttpFetchError> {
        if let Some(rest) = url.strip_prefix("file://") {
            // ureq doesn't speak file://. Strip and read directly.
            return fs::read(rest).map_err(|e| HttpFetchError::Network {
                url: url.to_string(),
                source: Box::new(e),
            });
        }

        if !(url.starts_with("http://") || url.starts_with("https://")) {
            return Err(HttpFetchError::InvalidUrl(format!(
                "unsupported URL scheme: {url}"
            )));
        }

        match self.agent.get(url).call() {
            Ok(resp) => {
                let mut reader = resp.into_reader();
                let mut bytes = Vec::new();
                reader
                    .read_to_end(&mut bytes)
                    .map_err(|e| HttpFetchError::Network {
                        url: url.to_string(),
                        source: Box::new(e),
                    })?;
                Ok(bytes)
            }
            Err(ureq::Error::Status(code, _)) => Err(HttpFetchError::Status {
                url: url.to_string(),
                status: code,
            }),
            Err(e) => Err(HttpFetchError::Network {
                url: url.to_string(),
                source: Box::new(e),
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn file_url_reads_local_file() {
        let mut f = NamedTempFile::new().unwrap();
        f.write_all(b"hello externals").unwrap();
        let url = format!("file://{}", f.path().display());
        let bytes = UreqFetcher::new().fetch(&url).unwrap();
        assert_eq!(bytes, b"hello externals");
    }

    #[test]
    fn missing_file_url_is_network_error() {
        let url = "file:///definitely/not/a/real/path/external.bin";
        let err = UreqFetcher::new().fetch(url).unwrap_err();
        assert!(err.is_transient(), "should be transient: {err:?}");
    }

    #[test]
    fn rejects_unknown_scheme() {
        let err = UreqFetcher::new().fetch("ftp://example.com/x").unwrap_err();
        assert!(matches!(err, HttpFetchError::InvalidUrl(_)));
    }
}
