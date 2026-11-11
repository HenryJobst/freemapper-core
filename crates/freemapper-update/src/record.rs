//! The check record: when the last check ran, by which version, and what it found.

use std::io;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::atomic::write_atomic;
use crate::version::Version;

/// A check is due again after this many seconds.
const INTERVAL_SECS: i64 = 24 * 60 * 60;

/// The outcome of the last attempt.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "result", rename_all = "snake_case")]
pub enum RecordResult {
    Current,
    Available {
        version: Version,
        notes: String,
    },
    /// A failed attempt counts as an attempt: the next try waits the interval too.
    Failed {
        reason: String,
    },
}

/// One line of memory between runs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckRecord {
    /// Unix seconds of the attempt.
    pub at: i64,
    /// The version that made the attempt.
    pub by: Version,
    pub result: RecordResult,
}

/// Reads the record; a missing, unreadable or foreign file is `None`, never an error.
pub fn load_record(path: &Path) -> Option<CheckRecord> {
    serde_json::from_slice(&std::fs::read(path).ok()?).ok()
}

/// Writes the record (the folder must exist).
pub fn save_record(path: &Path, record: &CheckRecord) -> io::Result<()> {
    let bytes = serde_json::to_vec_pretty(record).map_err(io::Error::other)?;
    write_atomic(path, &bytes)
}

/// Whether a check is due at `now` (Unix seconds): no record, older than 24 h, or from the future
/// (the clock went backwards).
pub fn due(record: Option<&CheckRecord>, now: i64) -> bool {
    record.is_none_or(|r| r.at > now || now - r.at > INTERVAL_SECS)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(text: &str) -> Version {
        text.parse().unwrap()
    }

    fn record(at: i64, result: RecordResult) -> CheckRecord {
        CheckRecord {
            at,
            by: v("0.10.0"),
            result,
        }
    }

    #[test]
    fn record_roundtrip_and_due_after_24_h() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("update-check.json");
        assert_eq!(load_record(&path), None);
        assert!(due(None, 1_000));
        let saved = record(
            1_000,
            RecordResult::Available {
                version: v("0.11.0"),
                notes: "https://github.com/x/releases/tag/v0.11.0".into(),
            },
        );
        save_record(&path, &saved).unwrap();
        assert_eq!(load_record(&path), Some(saved.clone()));
        save_record(&path, &record(1_000, RecordResult::Current)).unwrap();
        assert_eq!(
            load_record(&path),
            Some(record(1_000, RecordResult::Current))
        );
        assert!(!dir.path().join("update-check.json.part").exists());
        assert!(!due(Some(&saved), 1_000 + 23 * 3600));
        assert!(!due(Some(&saved), 1_000 + 24 * 3600));
        assert!(due(Some(&saved), 1_000 + 24 * 3600 + 1));
        assert!(due(Some(&saved), 999), "clock went backwards");
    }

    #[test]
    fn failed_attempt_counts_as_attempt() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("update-check.json");
        let failed = record(
            5_000,
            RecordResult::Failed {
                reason: "offline".into(),
            },
        );
        save_record(&path, &failed).unwrap();
        let loaded = load_record(&path).unwrap();
        assert_eq!(loaded, failed);
        assert!(!due(Some(&loaded), 5_000 + 3600));
        assert!(due(Some(&loaded), 5_000 + 24 * 3600 + 1));
    }

    #[test]
    fn unreadable_record_is_none() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("update-check.json");
        std::fs::write(&path, "not json").unwrap();
        assert_eq!(load_record(&path), None);
        assert_eq!(load_record(dir.path()), None);
    }
}
