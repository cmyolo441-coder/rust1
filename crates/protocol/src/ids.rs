//! Identifiers used across the event-sourced core.
//!
//! Every id is a ULID (48-bit millisecond timestamp + 80 bits of entropy) rendered in
//! Crockford base32, wrapped in a distinct newtype so that mixing two id families is a
//! *compile* error instead of a log-reading exercise at 3am.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

const CROCKFORD: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";

static COUNTER: AtomicU64 = AtomicU64::new(0);

/// SplitMix64 finalizer — cheap, well-distributed avalanche for the random 80 bits.
fn splitmix64(mut x: u64) -> u64 {
    x = x.wrapping_add(0x9E37_79B9_7F4A_7C15);
    let mut z = x;
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

fn entropy80() -> u128 {
    let n = COUNTER.fetch_add(1, Ordering::SeqCst);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.subsec_nanos() as u64)
        .unwrap_or(0);
    let pid = std::process::id() as u64;
    let a = splitmix64(n ^ nanos.rotate_left(17) ^ pid.rotate_left(41));
    let b = splitmix64(a ^ 0xA5A5_5A5A_1234_5678);
    ((a as u128) << 48) | ((b as u128) & 0xFFFF_FFFF_FFFF)
}

/// Monotonic, lexicographically sortable 128-bit id (26 base32 chars).
pub fn ulid_now() -> String {
    let ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    let bits: u128 = ((ms as u128) << 80) | (entropy80() & ((1u128 << 80) - 1));
    let mut out = [0u8; 26];
    let mut v = bits;
    for i in (0..26).rev() {
        out[i] = CROCKFORD[(v & 0x1f) as usize];
        v >>= 5;
    }
    String::from_utf8(out.to_vec()).expect("crockford alphabet is ASCII")
}

/// Wall-clock milliseconds since the unix epoch — the timestamp format used on the wire.
pub fn now_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

macro_rules! define_id {
    ($(#[$meta:meta])* $name:ident, $prefix:literal) => {
        $(#[$meta])*
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            /// Fresh sortable id, e.g. `trn_01JBQ...`.
            pub fn new() -> Self {
                Self(concat!($prefix, "{}").replacen("{}", &ulid_now(), 1))
            }
            /// Wrap an id that came off the wire / out of a rollout file.
            pub fn from_raw(raw: impl Into<String>) -> Self {
                Self(raw.into())
            }
            pub fn as_str(&self) -> &str {
                &self.0
            }
            pub fn into_inner(self) -> String {
                self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }

        impl FromStr for $name {
            type Err = std::convert::Infallible;
            fn from_str(s: &str) -> Result<Self, Self::Err> {
                Ok(Self(s.to_string()))
            }
        }

        impl Default for $name {
            fn default() -> Self {
                Self::new()
            }
        }

        impl AsRef<str> for $name {
            fn as_ref(&self) -> &str {
                &self.0
            }
        }
    };
}

define_id!(
    /// A conversation (one JSONL rollout file, one SQLite row).
    ThreadId,
    "thr_"
);
define_id!(
    /// One user prompt → one agent turn.
    TurnId,
    "trn_"
);
define_id!(
    /// One model-issued tool call.
    CallId,
    "cal_"
);
define_id!(
    /// One persisted session (a thread across process restarts).
    SessionId,
    "ses_"
);
define_id!(
    /// One parallel worker in orchestration mode.
    WorkerId,
    "wrk_"
);
define_id!(
    /// One event envelope on the bus / in the rollout.
    EventId,
    "evt_"
);
define_id!(
    /// Client → agent operation, idempotent by this id.
    OpId,
    "op_"
);
define_id!(
    /// One orchestration run (a DAG execution).
    RunId,
    "run_"
);
define_id!(
    /// One approval request; single use.
    RequestId,
    "req_"
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ulid_is_26_chars_and_sortable() {
        let a = ulid_now();
        std::thread::sleep(std::time::Duration::from_millis(2));
        let b = ulid_now();
        assert_eq!(a.len(), 26, "ulid length");
        assert_eq!(b.len(), 26);
        assert!(a < b, "later ulid must sort after earlier: {a} vs {b}");
        assert!(a.chars().all(|c| CROCKFORD.contains(&(c as u8))));
    }

    #[test]
    fn ids_are_distinct_and_prefixed() {
        let a = TurnId::new();
        let b = TurnId::new();
        assert_ne!(a, b);
        assert!(a.as_str().starts_with("trn_"));
        assert_eq!(a.as_str().len(), 4 + 26);
    }

    #[test]
    fn ids_roundtrip_through_json() {
        let id = ThreadId::new();
        let json = serde_json::to_string(&id).unwrap();
        assert_eq!(json, format!("\"{id}\""));
        let back: ThreadId = serde_json::from_str(&json).unwrap();
        assert_eq!(id, back);
    }
}
