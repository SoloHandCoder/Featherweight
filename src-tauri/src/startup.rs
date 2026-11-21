//! Startup apps manager: lists Run-key and Startup-folder entries and toggles
//! them through the `StartupApproved` registry values (what Task Manager uses).
#![allow(dead_code)]

use std::path::PathBuf;

use crate::engine::Engine;
use crate::model::{Hive, RegData};
use crate::system::{SysError, System};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StartupLocation {
    HkcuRun,
    HklmRun,
    HklmRun32,
    UserFolder,
    CommonFolder,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StartupEntry {
    pub id: String,
    pub name: String,
    pub command: String,
    pub location: StartupLocation,
    pub enabled: bool,
    pub per_user: bool,
}

pub struct StartupFolders {
    pub user: PathBuf,
    pub common: PathBuf,
}

const RUN: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const RUN32: &str = r"Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Run";
const APPROVED: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved";

impl StartupLocation {
    fn slug(self) -> &'static str {
        match self {
            Self::HkcuRun => "hkcu_run",
            Self::HklmRun => "hklm_run",
            Self::HklmRun32 => "hklm_run32",
            Self::UserFolder => "user_folder",
            Self::CommonFolder => "common_folder",
        }
    }

    fn per_user(self) -> bool {
        matches!(self, Self::HkcuRun | Self::UserFolder)
    }

    /// Hive and key of the `StartupApproved` values for this location.
    fn approved(self) -> (Hive, String) {
        match self {
            Self::HkcuRun => (Hive::Hkcu, format!(r"{APPROVED}\Run")),
            Self::HklmRun => (Hive::Hklm, format!(r"{APPROVED}\Run")),
            Self::HklmRun32 => (Hive::Hklm, format!(r"{APPROVED}\Run32")),
            Self::UserFolder => (Hive::Hkcu, format!(r"{APPROVED}\StartupFolder")),
            Self::CommonFolder => (Hive::Hklm, format!(r"{APPROVED}\StartupFolder")),
        }
    }
}

fn is_disabled(flag: Option<RegData>) -> bool {
    matches!(flag, Some(RegData::Binary(b)) if b.first() == Some(&3))
}

fn entry(location: StartupLocation, name: String, command: String, enabled: bool) -> StartupEntry {
    StartupEntry {
        id: format!("{}:{}", location.slug(), name),
        name,
        command,
        location,
        enabled,
        per_user: location.per_user(),
    }
}

pub fn list<S: System>(sys: &S, folders: &StartupFolders) -> Result<Vec<StartupEntry>, SysError> {
    let mut out = Vec::new();
    let run_keys = [
        (StartupLocation::HkcuRun, Hive::Hkcu, RUN),
        (StartupLocation::HklmRun, Hive::Hklm, RUN),
        (StartupLocation::HklmRun32, Hive::Hklm, RUN32),
    ];
    for (loc, hive, path) in run_keys {
        let (ahive, apath) = loc.approved();
        for (name, data) in sys.reg_values(hive, path)? {
            let (RegData::String(command) | RegData::ExpandString(command)) = data else {
                continue;
            };
            // One unreadable flag must not hide every other entry.
            let Ok(flag) = sys.reg_get(ahive, &apath, &name) else { continue };
            out.push(entry(loc, name, command, !is_disabled(flag)));
        }
    }
    let dirs = [
        (StartupLocation::UserFolder, &folders.user),
        (StartupLocation::CommonFolder, &folders.common),
    ];
    for (loc, dir) in dirs {
        let rd = match std::fs::read_dir(dir) {
            Ok(rd) => rd,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
            Err(e) => return Err(SysError::Other(format!("read {}: {e}", dir.display()))),
        };
        let (ahive, apath) = loc.approved();
        let mut files = Vec::new();
        for item in rd {
            let item = item.map_err(|e| SysError::Other(format!("read {}: {e}", dir.display())))?;
            if !item.file_type().is_ok_and(|t| t.is_file()) {
                continue;
            }
            let name = item.file_name().to_string_lossy().into_owned();
            if name.eq_ignore_ascii_case("desktop.ini") {
                continue;
            }
            files.push((name, item.path()));
        }
        files.sort();
        for (name, path) in files {
            let Ok(flag) = sys.reg_get(ahive, &apath, &name) else { continue };
            out.push(entry(loc, name, path.to_string_lossy().into_owned(), !is_disabled(flag)));
        }
    }
    Ok(out)
}

/// FILETIME of now: 100 ns ticks since 1601-01-01 UTC.
fn filetime_now() -> u64 {
    const UNIX_TO_FILETIME_SECS: u64 = 11_644_473_600;
    let d = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    (d.as_secs() + UNIX_TO_FILETIME_SECS) * 10_000_000 + u64::from(d.subsec_nanos() / 100)
}

fn approved_value(enabled: bool) -> RegData {
    let mut b = if enabled { vec![2, 0, 0, 0] } else { vec![3, 0, 0, 0] };
    let ft = if enabled { 0 } else { filetime_now() };
    b.extend_from_slice(&ft.to_le_bytes());
    RegData::Binary(b)
}

pub fn set_enabled<S: System>(
    engine: &mut Engine<S>,
    entry: &StartupEntry,
    enabled: bool,
) -> Result<(), SysError> {
    let (hive, path) = entry.location.approved();
    engine.set_registry_journaled(
        &format!("startup:{}", entry.id),
        hive,
        &path,
        &entry.name,
        Some(approved_value(enabled)),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::Catalog;
    use crate::engine::Env;
    use crate::journal::Journal;
    use crate::system::fake::FakeSystem;

    const RUN: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
    const RUN32: &str = r"Software\WOW6432Node\Microsoft\Windows\CurrentVersion\Run";
    const APPROVED: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved";

    fn folders(d: &tempfile::TempDir) -> StartupFolders {
        let user = d.path().join("user");
        let common = d.path().join("common");
        std::fs::create_dir_all(&user).unwrap();
        std::fs::create_dir_all(&common).unwrap();
        StartupFolders { user, common }
    }

    fn engine(sys: FakeSystem, d: &tempfile::TempDir) -> Engine<FakeSystem> {
        let journal = Journal::load(d.path().join("journal.json")).unwrap();
        let env = Env { build: 19045, is_laptop: false, per_user_allowed: true };
        Engine::new(sys, Catalog::from_strs(&[]).unwrap(), journal, env)
    }

    fn find<'a>(v: &'a [StartupEntry], id: &str) -> &'a StartupEntry {
        v.iter().find(|e| e.id == id).unwrap_or_else(|| panic!("no entry {id}"))
    }

    fn disabled_flag() -> RegData {
        let mut b = vec![3, 0, 0, 0];
        b.extend_from_slice(&[0u8; 8]);
        RegData::Binary(b)
    }

    #[test]
    fn lists_run_and_folder_entries() {
        let d = tempfile::tempdir().unwrap();
        let f = folders(&d);
        std::fs::write(f.user.join("Spotify.lnk"), b"x").unwrap();
        std::fs::write(f.user.join("desktop.ini"), b"x").unwrap();
        std::fs::write(f.common.join("Tool.lnk"), b"x").unwrap();
        let sys = FakeSystem::new()
            .with_reg(Hive::Hkcu, RUN, "OneDrive", RegData::String("od.exe /bg".into()))
            .with_reg(Hive::Hklm, RUN, "Sec", RegData::String("sec.exe".into()))
            .with_reg(Hive::Hklm, RUN32, "Old", RegData::String("old.exe".into()))
            .with_reg(Hive::Hkcu, RUN, "NotString", RegData::Dword(1));
        let v = list(&sys, &f).unwrap();
        assert_eq!(v.len(), 5);
        let od = find(&v, "hkcu_run:OneDrive");
        assert_eq!(od.name, "OneDrive");
        assert_eq!(od.command, "od.exe /bg");
        assert_eq!(od.location, StartupLocation::HkcuRun);
        assert!(od.enabled && od.per_user);
        assert_eq!(find(&v, "hklm_run:Sec").location, StartupLocation::HklmRun);
        assert_eq!(find(&v, "hklm_run32:Old").location, StartupLocation::HklmRun32);
        let sp = find(&v, "user_folder:Spotify.lnk");
        assert_eq!(sp.command, f.user.join("Spotify.lnk").to_string_lossy());
        assert!(sp.per_user);
        assert_eq!(find(&v, "common_folder:Tool.lnk").location, StartupLocation::CommonFolder);
    }

    #[test]
    fn expand_sz_is_listed_and_a_bad_entry_does_not_abort() {
        let d = tempfile::tempdir().unwrap();
        let f = folders(&d);
        std::fs::write(f.user.join("Bad.lnk"), b"x").unwrap();
        std::fs::write(f.user.join("Good.lnk"), b"x").unwrap();
        let mut sys = FakeSystem::new()
            .with_reg(Hive::Hkcu, RUN, "Exp", RegData::ExpandString(r"%APPDATA%\x.exe".into()))
            .with_reg(Hive::Hkcu, RUN, "Broken", RegData::String("b".into()))
            .with_reg(Hive::Hkcu, RUN, "Fine", RegData::String("f".into()));
        sys.fail_reads = vec!["Broken".into(), "Bad.lnk".into()];
        let v = list(&sys, &f).unwrap();
        assert_eq!(find(&v, "hkcu_run:Exp").command, r"%APPDATA%\x.exe");
        assert!(v.iter().any(|e| e.id == "hkcu_run:Fine"));
        assert!(v.iter().any(|e| e.id == "user_folder:Good.lnk"));
        assert!(!v.iter().any(|e| e.id == "hkcu_run:Broken" || e.id == "user_folder:Bad.lnk"));
    }

    #[test]
    fn missing_folders_are_empty_not_errors() {
        let d = tempfile::tempdir().unwrap();
        let f = StartupFolders { user: d.path().join("nope"), common: d.path().join("nope2") };
        assert!(list(&FakeSystem::new(), &f).unwrap().is_empty());
    }

    #[test]
    fn approved_03_means_disabled() {
        let d = tempfile::tempdir().unwrap();
        let f = folders(&d);
        std::fs::write(f.user.join("Spotify.lnk"), b"x").unwrap();
        let on = RegData::Binary(vec![2, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
        let sys = FakeSystem::new()
            .with_reg(Hive::Hkcu, RUN, "A", RegData::String("a".into()))
            .with_reg(Hive::Hkcu, RUN, "B", RegData::String("b".into()))
            .with_reg(Hive::Hklm, RUN32, "C", RegData::String("c".into()))
            .with_reg(Hive::Hkcu, &format!(r"{APPROVED}\Run"), "A", disabled_flag())
            .with_reg(Hive::Hkcu, &format!(r"{APPROVED}\Run"), "B", on)
            .with_reg(Hive::Hklm, &format!(r"{APPROVED}\Run32"), "C", disabled_flag())
            .with_reg(
                Hive::Hkcu,
                &format!(r"{APPROVED}\StartupFolder"),
                "Spotify.lnk",
                disabled_flag(),
            );
        let v = list(&sys, &f).unwrap();
        assert!(!find(&v, "hkcu_run:A").enabled);
        assert!(find(&v, "hkcu_run:B").enabled);
        assert!(!find(&v, "hklm_run32:C").enabled);
        assert!(!find(&v, "user_folder:Spotify.lnk").enabled);
    }

    #[test]
    fn hklm_entry_is_not_per_user() {
        let d = tempfile::tempdir().unwrap();
        let f = folders(&d);
        std::fs::write(f.common.join("T.lnk"), b"x").unwrap();
        let sys = FakeSystem::new()
            .with_reg(Hive::Hklm, RUN, "Sec", RegData::String("s".into()))
            .with_reg(Hive::Hklm, RUN32, "Old", RegData::String("o".into()));
        let v = list(&sys, &f).unwrap();
        assert!(v.iter().all(|e| !e.per_user));
        assert_eq!(v.len(), 3);
    }

    #[test]
    fn disable_then_undo_all_reenables() {
        let d = tempfile::tempdir().unwrap();
        let f = folders(&d);
        let sys =
            FakeSystem::new().with_reg(Hive::Hkcu, RUN, "OneDrive", RegData::String("od".into()));
        let mut e = engine(sys, &d);
        let entry = find(&list(&e.sys, &f).unwrap(), "hkcu_run:OneDrive").clone();
        set_enabled(&mut e, &entry, false).unwrap();
        let raw = e.sys.reg_get(Hive::Hkcu, &format!(r"{APPROVED}\Run"), "OneDrive").unwrap();
        let Some(RegData::Binary(b)) = raw else { panic!("expected binary") };
        assert_eq!(b.len(), 12);
        assert_eq!(&b[..4], &[3, 0, 0, 0]);
        assert_ne!(&b[4..], &[0u8; 8]);
        assert!(!find(&list(&e.sys, &f).unwrap(), "hkcu_run:OneDrive").enabled);
        e.undo_all(false);
        assert!(find(&list(&e.sys, &f).unwrap(), "hkcu_run:OneDrive").enabled);
    }

    #[test]
    fn enable_writes_02_and_folder_uses_startup_folder_key() {
        let d = tempfile::tempdir().unwrap();
        let f = folders(&d);
        std::fs::write(f.common.join("T.lnk"), b"x").unwrap();
        let sys = FakeSystem::new().with_reg(
            Hive::Hklm,
            &format!(r"{APPROVED}\StartupFolder"),
            "T.lnk",
            disabled_flag(),
        );
        let mut e = engine(sys, &d);
        let entry = find(&list(&e.sys, &f).unwrap(), "common_folder:T.lnk").clone();
        assert!(!entry.enabled);
        set_enabled(&mut e, &entry, true).unwrap();
        let raw =
            e.sys.reg_get(Hive::Hklm, &format!(r"{APPROVED}\StartupFolder"), "T.lnk").unwrap();
        let mut want = vec![2, 0, 0, 0];
        want.extend_from_slice(&[0u8; 8]);
        assert_eq!(raw, Some(RegData::Binary(want)));
        assert!(find(&list(&e.sys, &f).unwrap(), "common_folder:T.lnk").enabled);
    }
}
