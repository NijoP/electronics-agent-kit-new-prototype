//! Supplier provider abstraction with caching, rate limiting, retry with backoff, error
//! reporting, offline fallback and provenance (directive part 2 §7.2).
//!
//! * [`SupplierProvider`] is the trait a supplier lookup implements; the signed OpenAPI client
//!   (`lcsc_api::LcscApi`) and the legacy [`LcscProvider`] do.
//!   JLCPCB is a manufacturing-preparation adapter (`jlcpcb.rs`), not a part-lookup supplier, so it
//!   does not implement this trait; further suppliers add an impl.
//! * [`Resilient`] wraps any provider. Calls are blocking: run them off the UI thread (the desktop's
//!   `parts_*` commands already do). A supplier outage returns a typed [`ProviderError`]; it never
//!   panics, never blocks beyond `max_attempts` x timeout, and never invents data.
//! * Fresh in-memory cache hits are served as [`Served::Cache`]. If the supplier is unreachable or
//!   refuses access and an older cached answer exists, it is served as [`Served::StaleCache`] and
//!   labelled with its age and the reason; with no cached answer the error is returned as-is.
//! * Provenance stays on each [`SourcingRecord`] (provider, URL, retrieval time, raw SHA-256). A
//!   listing is never proof of electrical suitability: see `sourcing::VerificationState`.
//!
//! The cache is per process (not persisted). Code-reviewed only; tests are written, unrun.

use crate::lcsc::{Fetch, LcscError, LcscProvider};
use crate::sourcing::SourcingRecord;
use serde::Serialize;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorKind {
    BadQuery,
    /// No network path (DNS, TLS, timeout). Retried, then falls back to stale cache.
    Offline,
    /// Access refused (HTTP 401/403/429). Not retried; falls back to stale cache.
    Blocked,
    NotFound,
    Unusable,
    /// No API credentials are configured (LCSC OpenAPI). Never retried, never stale.
    NeedsCredentials,
    /// Credentials, signature or clock rejected. Never retried, never stale.
    Unauthorized,
    /// Rate limit or quota (per-minute 429, daily 430, or "busy"). Never retried; falls back
    /// to stale cache.
    RateLimited,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProviderError {
    pub provider: String,
    pub kind: ErrorKind,
    pub message: String,
}

impl std::fmt::Display for ProviderError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} ({:?}): {}", self.provider, self.kind, self.message)
    }
}

impl std::error::Error for ProviderError {}

/// How an answer was obtained.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "source", rename_all = "snake_case")]
pub enum Served {
    Live,
    /// Fresh cache hit (younger than the TTL).
    Cache {
        age_s: u64,
    },
    /// Supplier failed; this older cached answer is shown instead, never as live data.
    StaleCache {
        age_s: u64,
        because: ErrorKind,
    },
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Answer {
    pub provider: String,
    pub served: Served,
    pub records: Vec<SourcingRecord>,
}

pub trait SupplierProvider: Send + Sync {
    fn name(&self) -> &str;
    /// Exact lookup by the supplier's own part number.
    fn lookup(&self, part: &str) -> Result<SourcingRecord, ProviderError>;
    /// Keyword / MPN search.
    fn search(&self, query: &str) -> Result<Vec<SourcingRecord>, ProviderError>;
}

fn map_lcsc(e: LcscError) -> ProviderError {
    let kind = match &e {
        LcscError::BadQuery(_) => ErrorKind::BadQuery,
        LcscError::Transport(_) => ErrorKind::Offline,
        LcscError::Blocked { .. } => ErrorKind::Blocked,
        LcscError::NotFound(_) => ErrorKind::NotFound,
        LcscError::Unusable(_) => ErrorKind::Unusable,
    };
    ProviderError {
        provider: "lcsc".into(),
        kind,
        message: e.to_string(),
    }
}

impl<F: Fetch> SupplierProvider for LcscProvider<F> {
    fn name(&self) -> &str {
        "lcsc"
    }
    fn lookup(&self, part: &str) -> Result<SourcingRecord, ProviderError> {
        LcscProvider::lookup(self, part).map_err(map_lcsc)
    }
    fn search(&self, query: &str) -> Result<Vec<SourcingRecord>, ProviderError> {
        LcscProvider::search(self, query).map_err(map_lcsc)
    }
}

struct State {
    last_call: Option<Instant>,
    cache: HashMap<String, (Instant, Vec<SourcingRecord>)>,
}

/// Cache + rate limit + retry/backoff around a provider.
pub struct Resilient<P: SupplierProvider> {
    inner: P,
    /// A cached answer younger than this is served without a request.
    pub ttl: Duration,
    /// Minimum spacing between two requests to the supplier.
    pub min_interval: Duration,
    /// Total tries for an `Offline` failure (>= 1).
    pub max_attempts: u32,
    /// First backoff; doubles each retry, capped at 30 s.
    pub base_backoff: Duration,
    sleep: fn(Duration),
    state: Mutex<State>,
}

impl<P: SupplierProvider> Resilient<P> {
    pub fn new(inner: P) -> Self {
        Resilient {
            inner,
            ttl: Duration::from_secs(24 * 3600),
            min_interval: Duration::from_millis(500),
            max_attempts: 3,
            base_backoff: Duration::from_millis(500),
            sleep: std::thread::sleep,
            state: Mutex::new(State {
                last_call: None,
                cache: HashMap::new(),
            }),
        }
    }

    /// Replace the sleep function (tests use a no-op).
    pub fn with_sleep(mut self, sleep: fn(Duration)) -> Self {
        self.sleep = sleep;
        self
    }

    pub fn lookup(&self, part: &str) -> Result<Answer, ProviderError> {
        let key = format!("lookup:{}", part.trim().to_ascii_uppercase());
        self.call(&key, |p| p.lookup(part).map(|r| vec![r]))
    }

    pub fn search(&self, query: &str) -> Result<Answer, ProviderError> {
        let key = format!("search:{}", query.trim().to_lowercase());
        self.call(&key, |p| p.search(query))
    }

    fn pace(&self) {
        let wait = {
            let mut s = self.state.lock().unwrap_or_else(|e| e.into_inner());
            let now = Instant::now();
            let wait = match s.last_call {
                Some(t) => self
                    .min_interval
                    .saturating_sub(now.saturating_duration_since(t)),
                None => Duration::ZERO,
            };
            // Reserve the slot so concurrent callers queue behind each other.
            s.last_call = Some(now + wait);
            wait
        };
        if !wait.is_zero() {
            (self.sleep)(wait);
        }
    }

    fn call(
        &self,
        key: &str,
        f: impl Fn(&P) -> Result<Vec<SourcingRecord>, ProviderError>,
    ) -> Result<Answer, ProviderError> {
        let provider = self.inner.name().to_string();
        let cached = {
            let s = self.state.lock().unwrap_or_else(|e| e.into_inner());
            s.cache.get(key).map(|(t, r)| (t.elapsed(), r.clone()))
        };
        if let Some((age, records)) = &cached {
            if *age < self.ttl {
                return Ok(Answer {
                    provider,
                    served: Served::Cache {
                        age_s: age.as_secs(),
                    },
                    records: records.clone(),
                });
            }
        }
        let attempts = self.max_attempts.max(1);
        let mut backoff = self.base_backoff;
        let mut last_err = None;
        for attempt in 0..attempts {
            self.pace();
            match f(&self.inner) {
                Ok(records) => {
                    let mut s = self.state.lock().unwrap_or_else(|e| e.into_inner());
                    s.cache
                        .insert(key.to_string(), (Instant::now(), records.clone()));
                    return Ok(Answer {
                        provider,
                        served: Served::Live,
                        records,
                    });
                }
                Err(e) if e.kind == ErrorKind::Offline && attempt + 1 < attempts => {
                    (self.sleep)(backoff);
                    backoff = (backoff * 2).min(Duration::from_secs(30));
                    last_err = Some(e);
                }
                Err(e) => {
                    last_err = Some(e);
                    break;
                }
            }
        }
        let err = last_err.expect("at least one attempt ran");
        // Offline fallback: only for unreachable / refused / rate-limited, always labelled as stale.
        if matches!(
            err.kind,
            ErrorKind::Offline | ErrorKind::Blocked | ErrorKind::RateLimited
        ) {
            if let Some((age, records)) = cached {
                return Ok(Answer {
                    provider,
                    served: Served::StaleCache {
                        age_s: age.as_secs(),
                        because: err.kind,
                    },
                    records,
                });
            }
        }
        Err(err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::sync::Arc;

    fn rec(part: &str) -> SourcingRecord {
        serde_json::from_value(serde_json::json!({
            "provider": "fake", "source_url": "https://example.invalid/x",
            "retrieved_at_unix_s": 1, "raw_sha256": "00", "raw_bytes": 0,
            "lcsc_part": part, "mpn": null, "manufacturer": null, "package": null,
            "description": null, "datasheet_url": null, "specs": [], "stock": null,
            "price_breaks": [], "missing": [], "rejected": [], "datasheet": null,
            "attestation": null, "state": "search_hit"
        }))
        .expect("record json")
    }

    struct Fake {
        calls: Arc<AtomicU32>,
        /// Number of initial calls that fail with `kind`.
        fail_first: AtomicU32,
        kind: ErrorKind,
    }

    impl SupplierProvider for Fake {
        fn name(&self) -> &str {
            "fake"
        }
        fn lookup(&self, part: &str) -> Result<SourcingRecord, ProviderError> {
            let n = self.calls.fetch_add(1, Ordering::SeqCst);
            if n < self.fail_first.load(Ordering::SeqCst) {
                return Err(ProviderError {
                    provider: "fake".into(),
                    kind: self.kind,
                    message: "down".into(),
                });
            }
            Ok(rec(part))
        }
        fn search(&self, _q: &str) -> Result<Vec<SourcingRecord>, ProviderError> {
            Ok(vec![])
        }
    }

    fn make(fail_first: u32, kind: ErrorKind) -> (Resilient<Fake>, Arc<AtomicU32>) {
        let calls = Arc::new(AtomicU32::new(0));
        let r = Resilient::new(Fake {
            calls: calls.clone(),
            fail_first: AtomicU32::new(fail_first),
            kind,
        })
        .with_sleep(|_| {});
        (r, calls)
    }

    #[test]
    fn second_call_is_a_cache_hit() {
        let (r, calls) = make(0, ErrorKind::Offline);
        assert_eq!(r.lookup("C1").unwrap().served, Served::Live);
        assert!(matches!(
            r.lookup("c1").unwrap().served,
            Served::Cache { .. }
        ));
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn offline_is_retried_then_succeeds() {
        let (r, calls) = make(2, ErrorKind::Offline);
        assert_eq!(r.lookup("C1").unwrap().served, Served::Live);
        assert_eq!(calls.load(Ordering::SeqCst), 3);
    }

    #[test]
    fn offline_with_no_cache_is_an_error_not_invented_data() {
        let (r, calls) = make(99, ErrorKind::Offline);
        let e = r.lookup("C1").unwrap_err();
        assert_eq!(e.kind, ErrorKind::Offline);
        assert_eq!(calls.load(Ordering::SeqCst), 3);
    }

    #[test]
    fn blocked_is_not_retried() {
        let (r, calls) = make(99, ErrorKind::Blocked);
        assert_eq!(r.lookup("C1").unwrap_err().kind, ErrorKind::Blocked);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn stale_cache_is_served_and_labelled_when_the_supplier_fails() {
        let (mut r, calls) = make(0, ErrorKind::Offline);
        assert_eq!(r.lookup("C1").unwrap().served, Served::Live);
        // Expire the cache and make every later call fail.
        r.ttl = Duration::ZERO;
        r.inner.fail_first.store(u32::MAX, Ordering::SeqCst);
        let a = r.lookup("C1").unwrap();
        assert!(matches!(
            a.served,
            Served::StaleCache {
                because: ErrorKind::Offline,
                ..
            }
        ));
        assert_eq!(a.records.len(), 1);
        assert_eq!(calls.load(Ordering::SeqCst), 4);
    }
}
