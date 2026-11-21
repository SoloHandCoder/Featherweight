//! Crash-safe per-value undo journal. The engine records an entry BEFORE
//! applying an action and removes it after a successful undo.

use crate::model::{Snapshot, ValueKey};
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum JournalError {
    #[error("journal I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("journal file is corrupt: {0}")]
    Corrupt(String),
    #[error("journal serialization failed: {0}")]
    Serialize(String),
    #[error("the undo journal is in use by another copy of Featherweight")]
    Locked,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct JournalEntry {
    pub key: ValueKey,
    pub tweak_id: String,
    pub action_index: usize,
    pub before: Snapshot,
    pub after: Snapshot,
    /// RFC 3339 UTC, fixed millisecond precision (string order == time order).
    pub applied_at: String,
    /// SID of the account whose state a per-user key (HKCU value, Appx
    /// package, per-user script) belongs to; `None` for system-wide keys and
    /// for entries written before this field existed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub user_sid: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct JournalFile {
    version: u32,
    entries: Vec<JournalEntry>,
}

pub struct Journal {
    path: PathBuf,
    entries: Vec<JournalEntry>,
    /// `<journal>.lock`, held open without sharing while the app runs.
    _lock: Option<fs::File>,
}

/// Opens `<journal>.lock` exclusively (no sharing at all). The journal file
/// itself is replaced by rename on every write, so it can't carry the lock.
fn take_lock(journal: &std::path::Path) -> Result<fs::File, JournalError> {
    let mut p = journal.as_os_str().to_owned();
    p.push(".lock");
    let mut o = fs::OpenOptions::new();
    o.read(true).write(true).create(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
        o.share_mode(0).custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
    }
    if let Some(dir) = journal.parent() {
        fs::create_dir_all(dir)?;
    }
    o.open(PathBuf::from(p)).map_err(|e| {
        // ERROR_SHARING_VIOLATION: another instance holds it.
        if e.raw_os_error() == Some(32) { JournalError::Locked } else { JournalError::Io(e) }
    })
}

/// Current UTC time as `YYYY-MM-DDTHH:MM:SS.mmmZ`.
pub fn now_rfc3339() -> String {
    use time::macros::format_description;
    time::OffsetDateTime::now_utc()
        .format(format_description!(
            "[year]-[month]-[day]T[hour]:[minute]:[second].[subsecond digits:3]Z"
        ))
        .unwrap_or_else(|_| "1970-01-01T00:00:00.000Z".to_string())
}

impl Journal {
    /// A journal that holds nothing and can never be written (used when the
    /// real one can't be trusted; commands refuse to change anything then).
    pub fn unavailable() -> Journal {
        Journal { path: PathBuf::new(), entries: Vec::new(), _lock: None }
    }

    /// Missing file yields an empty journal; a bad file is `Corrupt` and untouched.
    pub fn load(path: PathBuf) -> Result<Journal, JournalError> {
        let entries = match fs::read(&path) {
            Ok(bytes) => {
                let f: JournalFile = serde_json::from_slice(&bytes)
                    .map_err(|e| JournalError::Corrupt(e.to_string()))?;
                if f.version != 1 {
                    return Err(JournalError::Corrupt(format!(
                        "unsupported version {}",
                        f.version
                    )));
                }
                f.entries
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
            Err(e) => return Err(e.into()),
        };
        Ok(Journal { path, entries, _lock: None })
    }

    /// `load`, holding an exclusive lock next to the journal for as long as
    /// the returned journal lives. Fails with `Locked` if another process
    /// holds it.
    pub fn load_exclusive(path: PathBuf) -> Result<Journal, JournalError> {
        let lock = take_lock(&path)?;
        let mut j = Journal::load(path)?;
        j._lock = Some(lock);
        Ok(j)
    }

    /// Records an entry. For an existing (key, account) the original `before`
    /// is kept; the same key of another account is a separate entry.
    pub fn record(&mut self, e: JournalEntry) -> Result<(), JournalError> {
        let prev = self.entries.clone();
        match self.entries.iter_mut().find(|x| x.key == e.key && x.user_sid == e.user_sid) {
            Some(existing) => {
                let before = existing.before.clone();
                *existing = JournalEntry { before, ..e };
            }
            None => self.entries.push(e),
        }
        self.persist_or_rollback(prev)
    }

    /// Removes the entry for `key` of account `sid` (`None` = system-wide).
    pub fn remove(&mut self, key: &ValueKey, sid: Option<&str>) -> Result<(), JournalError> {
        let prev = self.entries.clone();
        self.entries.retain(|x| !(&x.key == key && x.user_sid.as_deref() == sid));
        self.persist_or_rollback(prev)
    }

    pub fn for_tweak(&self, id: &str) -> Vec<JournalEntry> {
        self.entries
            .iter()
            .filter(|e| e.tweak_id == id)
            .cloned()
            .collect()
    }

    /// The entry for `key` of account `sid`, if any.
    pub fn find(&self, key: &ValueKey, sid: Option<&str>) -> Option<JournalEntry> {
        self.entries.iter().find(|e| &e.key == key && e.user_sid.as_deref() == sid).cloned()
    }

    /// Newest first; ties keep reverse insertion order.
    pub fn all(&self) -> Vec<JournalEntry> {
        let mut v: Vec<JournalEntry> = self.entries.iter().rev().cloned().collect();
        v.sort_by(|a, b| b.applied_at.cmp(&a.applied_at)); // stable
        v
    }

    fn persist_or_rollback(&mut self, prev: Vec<JournalEntry>) -> Result<(), JournalError> {
        let r = self.persist();
        if r.is_err() {
            self.entries = prev;
        }
        r
    }

    /// Write `<file>.tmp`, flush + sync, then rename over the real file.
    fn persist(&self) -> Result<(), JournalError> {
        let file = JournalFile {
            version: 1,
            entries: self.entries.clone(),
        };
        if self.path.as_os_str().is_empty() {
            return Err(JournalError::Io(std::io::Error::other("the undo journal is unavailable")));
        }
        let json = serde_json::to_vec_pretty(&file)
            .map_err(|e| JournalError::Serialize(e.to_string()))?;
        if let Some(dir) = self.path.parent() {
            fs::create_dir_all(dir)?;
        }
        let mut tmp = self.path.clone().into_os_string();
        tmp.push(".tmp");
        let tmp = PathBuf::from(tmp);
        {
            let mut f = fs::File::create(&tmp)?;
            f.write_all(&json)?;
            f.flush()?;
            f.sync_all()?;
        }
        fs::rename(&tmp, &self.path)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{RegData, StartType};

    fn reg_key(name: &str) -> ValueKey {
        ValueKey::Registry {
            hive: crate::model::Hive::Hkcu,
            path: "Software\\X".into(),
            name: name.into(),
        }
    }

    fn entry(key: ValueKey, tweak: &str, before: u32, after: u32, at: &str) -> JournalEntry {
        JournalEntry {
            key,
            tweak_id: tweak.into(),
            action_index: 0,
            before: Snapshot::Reg(Some(RegData::Dword(before))),
            after: Snapshot::Reg(Some(RegData::Dword(after))),
            applied_at: at.into(),
            user_sid: None,
        }
    }

    #[test]
    fn load_missing_file_is_empty() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("sub").join("journal.json");
        let j = Journal::load(p.clone()).unwrap();
        assert!(j.all().is_empty());
        assert!(!p.parent().unwrap().exists(), "load must not create dirs");
    }

    #[test]
    fn record_persists_and_reloads() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("nested").join("journal.json");
        let mut j = Journal::load(p.clone()).unwrap();
        let e = entry(reg_key("A"), "t1", 1, 2, "2026-01-01T00:00:00.000Z");
        j.record(e.clone()).unwrap();
        assert!(!d.path().join("nested").join("journal.json.tmp").exists());
        let j2 = Journal::load(p).unwrap();
        assert_eq!(j2.all(), vec![e.clone()]);
        assert_eq!(j2.for_tweak("t1"), vec![e]);
        assert!(j2.for_tweak("other").is_empty());
    }

    #[test]
    fn record_same_key_keeps_original_before() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("journal.json");
        let mut j = Journal::load(p.clone()).unwrap();
        j.record(entry(reg_key("A"), "t1", 1, 2, "2026-01-01T00:00:00.000Z"))
            .unwrap();
        let mut second = entry(reg_key("A"), "t2", 2, 3, "2026-01-02T00:00:00.000Z");
        second.action_index = 4;
        j.record(second).unwrap();
        let all = Journal::load(p).unwrap().all();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].before, Snapshot::Reg(Some(RegData::Dword(1))));
        assert_eq!(all[0].after, Snapshot::Reg(Some(RegData::Dword(3))));
        assert_eq!(all[0].applied_at, "2026-01-02T00:00:00.000Z");
        assert_eq!(all[0].tweak_id, "t2");
        assert_eq!(all[0].action_index, 4);
    }

    #[test]
    fn same_key_for_different_accounts_is_kept_separately() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("journal.json");
        let mut j = Journal::load(p.clone()).unwrap();
        let mut a = entry(reg_key("A"), "t", 1, 2, "2026-01-01T00:00:00.000Z");
        a.user_sid = Some("S-1-5-21-1".into());
        let mut b = entry(reg_key("A"), "t", 5, 2, "2026-01-01T00:00:01.000Z");
        b.user_sid = Some("S-1-5-21-2".into());
        j.record(a.clone()).unwrap();
        j.record(b.clone()).unwrap();
        assert_eq!(j.all().len(), 2, "never merged across accounts");
        // Re-recording account 1 merges with account 1's entry only.
        let mut a2 = entry(reg_key("A"), "t", 2, 3, "2026-01-02T00:00:00.000Z");
        a2.user_sid = a.user_sid.clone();
        j.record(a2).unwrap();
        let all = Journal::load(p.clone()).unwrap().all();
        assert_eq!(all.len(), 2);
        let mine = all.iter().find(|e| e.user_sid == a.user_sid).unwrap();
        assert_eq!(mine.before, Snapshot::Reg(Some(RegData::Dword(1))));
        let theirs = all.iter().find(|e| e.user_sid == b.user_sid).unwrap();
        assert_eq!(theirs.before, Snapshot::Reg(Some(RegData::Dword(5))));
        j.remove(&reg_key("A"), a.user_sid.as_deref()).unwrap();
        assert_eq!(Journal::load(p).unwrap().all(), vec![b]);
    }

    #[test]
    fn old_entries_without_user_sid_still_load() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("journal.json");
        std::fs::write(
            &p,
            br#"{"version":1,"entries":[{"key":{"service":{"name":"svc"}},"tweak_id":"t","action_index":0,
            "before":{"service":"automatic"},"after":{"service":"disabled"},"applied_at":"2026-01-01T00:00:00.000Z"}]}"#,
        )
        .unwrap();
        let all = Journal::load(p).unwrap().all();
        assert_eq!(all.len(), 1);
        assert_eq!(all[0].user_sid, None);
    }

    #[test]
    fn remove_deletes_entry() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("journal.json");
        let mut j = Journal::load(p.clone()).unwrap();
        let svc = ValueKey::Service { name: "Svc".into() };
        j.record(entry(reg_key("A"), "t1", 1, 2, "2026-01-01T00:00:00.000Z"))
            .unwrap();
        j.record(JournalEntry {
            key: svc.clone(),
            tweak_id: "t1".into(),
            action_index: 1,
            before: Snapshot::Service(StartType::Automatic),
            after: Snapshot::Service(StartType::Disabled),
            applied_at: "2026-01-01T00:00:01.000Z".into(),
            user_sid: None,
        })
        .unwrap();
        j.remove(&svc, None).unwrap();
        j.remove(&svc, None).unwrap(); // idempotent
        let j2 = Journal::load(p).unwrap();
        assert_eq!(j2.all().len(), 1);
        assert_eq!(j2.all()[0].key, reg_key("A"));
    }

    #[test]
    fn all_is_newest_first() {
        let d = tempfile::tempdir().unwrap();
        let mut j = Journal::load(d.path().join("journal.json")).unwrap();
        j.record(entry(reg_key("old"), "t", 1, 2, "2026-01-01T00:00:00.000Z"))
            .unwrap();
        j.record(entry(reg_key("new"), "t", 1, 2, "2026-03-01T00:00:00.000Z"))
            .unwrap();
        j.record(entry(reg_key("mid"), "t", 1, 2, "2026-02-01T00:00:00.000Z"))
            .unwrap();
        // identical timestamps: later insertion comes first
        j.record(entry(reg_key("tie1"), "t", 1, 2, "2026-03-01T00:00:00.000Z"))
            .unwrap();
        j.record(entry(reg_key("tie2"), "t", 1, 2, "2026-03-01T00:00:00.000Z"))
            .unwrap();
        let names: Vec<String> = j
            .all()
            .into_iter()
            .map(|e| match e.key {
                ValueKey::Registry { name, .. } => name,
                _ => unreachable!(),
            })
            .collect();
        assert_eq!(names, ["tie2", "tie1", "new", "mid", "old"]);
    }

    #[test]
    fn exclusive_load_locks_out_a_second_instance() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("journal.json");
        let first = Journal::load_exclusive(p.clone()).unwrap();
        assert!(matches!(Journal::load_exclusive(p.clone()), Err(JournalError::Locked)));
        drop(first);
        let mut again = Journal::load_exclusive(p.clone()).unwrap();
        // The lock does not get in the way of writing the journal itself.
        again.record(entry(reg_key("A"), "t", 1, 2, "2026-01-01T00:00:00.000Z")).unwrap();
        assert_eq!(Journal::load(p).unwrap().all().len(), 1);
    }

    #[test]
    fn now_is_fixed_width_utc() {
        let a = now_rfc3339();
        assert_eq!(a.len(), "2026-01-01T00:00:00.000Z".len());
        assert!(a.ends_with('Z'));
    }

    #[test]
    fn corrupt_file_errors_without_overwriting() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("journal.json");
        std::fs::write(&p, b"{ not json").unwrap();
        match Journal::load(p.clone()) {
            Err(JournalError::Corrupt(_)) => {}
            other => panic!("expected Corrupt, got {:?}", other.map(|_| ())),
        }
        assert_eq!(std::fs::read(&p).unwrap(), b"{ not json");
    }
}
