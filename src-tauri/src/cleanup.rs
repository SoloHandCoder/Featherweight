//! Junk-file scan and clean. Never follows links out of a target.
#![allow(dead_code)]

use crate::system::System;
use serde::Serialize;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanupTarget {
    pub id: String,
    pub label: String,
    pub paths: Vec<PathBuf>,
    pub bytes: u64,
    pub needs_service_stop: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CleanupReport {
    pub freed_bytes: u64,
    pub skipped_files: u32,
}

fn env_path(var: &str) -> Option<PathBuf> {
    std::env::var_os(var).filter(|v| !v.is_empty()).map(PathBuf::from)
}

fn system_root() -> PathBuf {
    env_path("SystemRoot").unwrap_or_else(|| PathBuf::from(r"C:\Windows"))
}

pub fn default_targets() -> Vec<CleanupTarget> {
    let t = |id: &str, label: &str, paths: Vec<PathBuf>, svc: Option<&str>| CleanupTarget {
        id: id.into(),
        label: label.into(),
        paths,
        bytes: 0,
        needs_service_stop: svc.map(String::from),
    };
    let root = system_root();
    vec![
        t("user-temp", "Your temporary files", env_path("TEMP").into_iter().collect(), None),
        t("system-temp", "Windows temporary files", vec![root.join("Temp")], None),
        t(
            "wu-cache",
            "Old Windows Update downloads",
            vec![root.join(r"SoftwareDistribution\Download")],
            Some("wuauserv"),
        ),
        t(
            "thumbs",
            "Thumbnail cache",
            env_path("LOCALAPPDATA")
                .map(|l| l.join(r"Microsoft\Windows\Explorer\thumbcache_*.db"))
                .into_iter()
                .collect(),
            None,
        ),
        t(
            "delivery-opt",
            "Delivery Optimization cache",
            vec![root.join(
                r"ServiceProfiles\NetworkService\AppData\Local\Microsoft\Windows\DeliveryOptimization\Cache",
            )],
            None,
        ),
    ]
}

/// True for symlinks and junctions (any reparse point).
fn is_link(m: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x400;
        m.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0 || m.file_type().is_symlink()
    }
    #[cfg(not(windows))]
    {
        m.file_type().is_symlink()
    }
}

/// Directory-flavoured link (junction or dir symlink) needs `remove_dir`.
fn is_dir_link(m: &fs::Metadata) -> bool {
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_DIRECTORY: u32 = 0x10;
        m.file_attributes() & FILE_ATTRIBUTE_DIRECTORY != 0
    }
    #[cfg(not(windows))]
    {
        m.is_dir()
    }
}

/// `thumbcache_*.db`-style: a single `*` in the final component.
fn wildcard_match(pattern: &str, name: &str) -> bool {
    let (p, n) = (pattern.to_lowercase(), name.to_lowercase());
    match p.split_once('*') {
        Some((pre, suf)) => {
            n.len() >= pre.len() + suf.len() && n.starts_with(pre) && n.ends_with(suf)
        }
        None => p == n,
    }
}

/// The entries a target path stands for: the folder's contents, or the
/// non-recursive file matches for a wildcard path. Never the root itself.
fn entries_of(path: &Path) -> Vec<PathBuf> {
    // Refuse drive roots and empty paths outright.
    let Some(parent) = path.parent() else { return Vec::new() };
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    if name.contains('*') {
        let Ok(rd) = fs::read_dir(parent) else { return Vec::new() };
        return rd
            .flatten()
            .filter(|e| {
                wildcard_match(&name, &e.file_name().to_string_lossy())
                    && e.metadata().map(|m| m.is_file()).unwrap_or(false)
            })
            .map(|e| e.path())
            .collect();
    }
    match fs::read_dir(path) {
        Ok(rd) => rd.flatten().map(|e| e.path()).collect(),
        Err(_) => Vec::new(),
    }
}

/// (bytes, file count) under `p`, never descending into links.
fn measure(p: &Path) -> (u64, u32) {
    let Ok(m) = fs::symlink_metadata(p) else { return (0, 0) };
    if is_link(&m) {
        return (0, 0);
    }
    if m.is_dir() {
        let Ok(rd) = fs::read_dir(p) else { return (0, 0) };
        return rd.flatten().fold((0, 0), |(b, n), e| {
            let (b2, n2) = measure(&e.path());
            (b + b2, n + n2)
        });
    }
    (m.len(), 1)
}

pub fn scan(targets: &mut [CleanupTarget]) {
    for t in targets {
        t.bytes = t.paths.iter().flat_map(|p| entries_of(p)).map(|e| measure(&e).0).sum();
    }
}

fn remove_file(p: &Path) -> bool {
    if fs::remove_file(p).is_ok() {
        return true;
    }
    // Read-only files: clear the flag and retry once.
    if let Ok(m) = fs::metadata(p) {
        let mut perm = m.permissions();
        if perm.readonly() {
            #[allow(clippy::permissions_set_readonly_false)]
            perm.set_readonly(false);
            if fs::set_permissions(p, perm).is_ok() {
                return fs::remove_file(p).is_ok();
            }
        }
    }
    false
}

fn delete(p: &Path, rep: &mut CleanupReport) {
    let Ok(m) = fs::symlink_metadata(p) else { return };
    if is_link(&m) {
        // Remove only the link itself, never what it points at.
        let _ = if is_dir_link(&m) { fs::remove_dir(p) } else { fs::remove_file(p) };
        return;
    }
    if m.is_dir() {
        if let Ok(rd) = fs::read_dir(p) {
            for e in rd.flatten() {
                delete(&e.path(), rep);
            }
        }
        // Fails (harmlessly) if something inside was skipped.
        let _ = fs::remove_dir(p);
        return;
    }
    let size = m.len();
    if remove_file(p) {
        rep.freed_bytes += size;
    } else {
        rep.skipped_files += 1;
    }
}

/// Restarts the service it stopped when dropped, whatever happens.
struct Restart<'a, S: System> {
    sys: &'a mut S,
    name: &'a str,
}

impl<S: System> Drop for Restart<'_, S> {
    fn drop(&mut self) {
        let _ = self.sys.start_service(self.name);
    }
}

pub fn clean<S: System>(sys: &mut S, t: &CleanupTarget) -> CleanupReport {
    let mut rep = CleanupReport::default();
    let entries: Vec<PathBuf> = t.paths.iter().flat_map(|p| entries_of(p)).collect();

    let mut _guard = None;
    if let Some(name) = t.needs_service_stop.as_deref() {
        match sys.stop_service(name) {
            Ok(true) => _guard = Some(Restart { sys, name }),
            Ok(false) => {}
            Err(_) => {
                // Can't safely delete: report everything as skipped.
                rep.skipped_files = entries.iter().map(|e| measure(e).1).sum();
                return rep;
            }
        }
    }
    for e in &entries {
        delete(e, &mut rep);
    }
    rep
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::system::fake::FakeSystem;
    use std::fs;
    use std::path::Path;

    fn target(paths: Vec<PathBuf>, svc: Option<&str>) -> CleanupTarget {
        CleanupTarget {
            id: "t".into(),
            label: "T".into(),
            paths,
            bytes: 0,
            needs_service_stop: svc.map(String::from),
        }
    }

    fn write(p: &Path, n: usize) {
        fs::create_dir_all(p.parent().unwrap()).unwrap();
        fs::write(p, vec![b'x'; n]).unwrap();
    }

    #[test]
    fn scan_sums_sizes() {
        let d = tempfile::tempdir().unwrap();
        write(&d.path().join("a.bin"), 100);
        write(&d.path().join("sub/b.bin"), 50);
        write(&d.path().join("sub/deep/c.bin"), 25);
        let mut ts = vec![target(vec![d.path().to_path_buf(), d.path().join("missing")], None)];
        scan(&mut ts);
        assert_eq!(ts[0].bytes, 175);
    }

    #[test]
    fn scan_thumbcache_matches_only_thumbcache_files() {
        let d = tempfile::tempdir().unwrap();
        write(&d.path().join("thumbcache_32.db"), 10);
        write(&d.path().join("thumbcache_256.db"), 20);
        write(&d.path().join("iconcache_32.db"), 1000);
        write(&d.path().join("sub/thumbcache_x.db"), 1000);
        let mut ts = vec![target(vec![d.path().join("thumbcache_*.db")], None)];
        scan(&mut ts);
        assert_eq!(ts[0].bytes, 30);
        let mut sys = FakeSystem::new();
        let r = clean(&mut sys, &ts[0]);
        assert_eq!(r.freed_bytes, 30);
        assert!(d.path().join("iconcache_32.db").exists());
        assert!(d.path().join("sub/thumbcache_x.db").exists());
        assert!(!d.path().join("thumbcache_32.db").exists());
    }

    #[test]
    fn clean_deletes_contents_but_keeps_root() {
        let d = tempfile::tempdir().unwrap();
        write(&d.path().join("a.bin"), 100);
        write(&d.path().join("sub/b.bin"), 50);
        let t = target(vec![d.path().to_path_buf()], None);
        let mut sys = FakeSystem::new();
        let r = clean(&mut sys, &t);
        assert_eq!(r, CleanupReport { freed_bytes: 150, skipped_files: 0 });
        assert!(d.path().exists());
        assert_eq!(fs::read_dir(d.path()).unwrap().count(), 0);
    }

    #[cfg(windows)]
    #[test]
    fn clean_skips_locked_file_and_reports_only_freed() {
        use std::os::windows::fs::OpenOptionsExt;
        let d = tempfile::tempdir().unwrap();
        write(&d.path().join("free.bin"), 100);
        write(&d.path().join("locked.bin"), 40);
        let _held = fs::OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(d.path().join("locked.bin"))
            .unwrap();
        let t = target(vec![d.path().to_path_buf()], None);
        let mut sys = FakeSystem::new();
        let r = clean(&mut sys, &t);
        assert_eq!(r, CleanupReport { freed_bytes: 100, skipped_files: 1 });
        assert!(d.path().join("locked.bin").exists());
        assert!(!d.path().join("free.bin").exists());
    }

    #[cfg(windows)]
    #[test]
    fn clean_does_not_follow_junction() {
        let d = tempfile::tempdir().unwrap();
        let outside = d.path().join("outside");
        write(&outside.join("keep.txt"), 77);
        write(&outside.join("nested/keep2.txt"), 5);
        let root = d.path().join("root");
        fs::create_dir_all(&root).unwrap();
        write(&root.join("junk.bin"), 10);
        let link = root.join("link");
        let st = std::process::Command::new("cmd")
            .args(["/c", "mklink", "/J"])
            .arg(&link)
            .arg(&outside)
            .output()
            .unwrap();
        assert!(st.status.success(), "mklink failed");
        assert!(link.join("keep.txt").exists());

        let mut ts = vec![target(vec![root.clone()], None)];
        scan(&mut ts);
        assert_eq!(ts[0].bytes, 10, "scan must not count junction contents");
        let mut sys = FakeSystem::new();
        let r = clean(&mut sys, &ts[0]);
        assert_eq!(r.freed_bytes, 10);
        assert_eq!(r.skipped_files, 0);
        assert!(outside.join("keep.txt").exists());
        assert!(outside.join("nested/keep2.txt").exists());
        assert!(root.exists());
        assert!(!link.exists(), "the link itself is removed");
    }

    #[test]
    fn clean_restarts_stopped_service_after_clean() {
        let d = tempfile::tempdir().unwrap();
        write(&d.path().join("a.bin"), 10);
        let t = target(vec![d.path().to_path_buf()], Some("wuauserv"));
        let mut sys = FakeSystem::new().with_running_service("wuauserv");
        let r = clean(&mut sys, &t);
        assert_eq!(r.freed_bytes, 10);
        assert_eq!(sys.service_calls, vec!["stop:wuauserv", "start:wuauserv"]);
        assert!(sys.running_services.contains("wuauserv"));
    }

    #[cfg(windows)]
    #[test]
    fn clean_restarts_stopped_service_on_error() {
        use std::os::windows::fs::OpenOptionsExt;
        let d = tempfile::tempdir().unwrap();
        write(&d.path().join("locked.bin"), 40);
        let _held = fs::OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(d.path().join("locked.bin"))
            .unwrap();
        let t = target(vec![d.path().to_path_buf()], Some("wuauserv"));
        let mut sys = FakeSystem::new().with_running_service("wuauserv");
        let r = clean(&mut sys, &t);
        assert_eq!(r.skipped_files, 1);
        assert!(sys.running_services.contains("wuauserv"), "service must be restarted");
    }

    #[test]
    fn stop_failure_skips_deletion_and_counts_files() {
        let d = tempfile::tempdir().unwrap();
        write(&d.path().join("a.bin"), 10);
        write(&d.path().join("b.bin"), 10);
        let t = target(vec![d.path().to_path_buf()], Some("wuauserv"));
        let mut sys = FakeSystem::new().with_running_service("wuauserv");
        sys.fail_stop = Some("boom".into());
        let r = clean(&mut sys, &t);
        assert_eq!(r, CleanupReport { freed_bytes: 0, skipped_files: 2 });
        assert!(d.path().join("a.bin").exists());
        assert_eq!(sys.service_calls, vec!["stop:wuauserv"]);
    }

    #[test]
    fn already_stopped_service_is_not_started() {
        let d = tempfile::tempdir().unwrap();
        write(&d.path().join("a.bin"), 10);
        let t = target(vec![d.path().to_path_buf()], Some("wuauserv"));
        let mut sys = FakeSystem::new().with_service("wuauserv", crate::model::StartType::Manual);
        let r = clean(&mut sys, &t);
        assert_eq!(r.freed_bytes, 10);
        assert_eq!(sys.service_calls, vec!["stop:wuauserv"]);
    }

    #[test]
    fn default_targets_paths_from_env() {
        let ts = default_targets();
        let ids: Vec<_> = ts.iter().map(|t| t.id.as_str()).collect();
        assert_eq!(ids, ["user-temp", "system-temp", "wu-cache", "thumbs", "delivery-opt"]);
        let wu = &ts[2];
        assert_eq!(wu.needs_service_stop.as_deref(), Some("wuauserv"));
        assert!(wu.paths[0].ends_with(r"SoftwareDistribution\Download"));
        assert!(ts[3].paths[0].to_string_lossy().ends_with("thumbcache_*.db"));
    }
}
