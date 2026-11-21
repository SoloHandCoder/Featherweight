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

/// Well-known folders from the OS APIs (not environment variables, which a
/// less-privileged process could influence).
#[cfg(windows)]
mod known {
    use std::path::PathBuf;
    use windows::Win32::System::Com::CoTaskMemFree;
    use windows::Win32::Storage::FileSystem::{GetTempPath2W, GetTempPathW};
    use windows::Win32::System::SystemInformation::GetWindowsDirectoryW;
    use windows::Win32::UI::Shell::{
        FOLDERID_LocalAppData, FOLDERID_Profile, FOLDERID_ProgramFiles, FOLDERID_ProgramFilesX86,
        KF_FLAG_DEFAULT, SHGetKnownFolderPath,
    };
    use windows::core::GUID;

    fn from_buf(buf: &[u16], n: u32) -> Option<PathBuf> {
        let n = n as usize;
        if n == 0 || n > buf.len() {
            return None;
        }
        Some(PathBuf::from(String::from_utf16_lossy(&buf[..n])))
    }

    pub fn temp_dir() -> Option<PathBuf> {
        let mut buf = [0u16; 1024];
        // SAFETY: buffer is valid for its length.
        let n = unsafe { GetTempPath2W(Some(&mut buf)) };
        let n = if n == 0 { unsafe { GetTempPathW(Some(&mut buf)) } } else { n };
        let p = from_buf(&buf, n)?;
        // The API returns a trailing separator; drop it.
        Some(PathBuf::from(p.to_string_lossy().trim_end_matches('\\').to_string()))
    }

    pub fn windows_dir() -> Option<PathBuf> {
        let mut buf = [0u16; 512];
        // SAFETY: buffer is valid for its length.
        let n = unsafe { GetWindowsDirectoryW(Some(&mut buf)) };
        from_buf(&buf, n)
    }

    fn folder(id: &GUID) -> Option<PathBuf> {
        // SAFETY: on success the returned PWSTR is CoTaskMem-allocated; we copy
        // it and free it exactly once.
        unsafe {
            let p = SHGetKnownFolderPath(id, KF_FLAG_DEFAULT, None).ok()?;
            let s = p.to_string().ok();
            CoTaskMemFree(Some(p.0 as *const _));
            s.map(PathBuf::from)
        }
    }

    pub fn local_app_data() -> Option<PathBuf> {
        folder(&FOLDERID_LocalAppData)
    }
    pub fn profile() -> Option<PathBuf> {
        folder(&FOLDERID_Profile)
    }
    pub fn program_files() -> Option<PathBuf> {
        folder(&FOLDERID_ProgramFiles)
    }
    pub fn program_files_x86() -> Option<PathBuf> {
        folder(&FOLDERID_ProgramFilesX86)
    }
}

#[cfg(not(windows))]
mod known {
    use std::path::PathBuf;
    fn env(v: &str) -> Option<PathBuf> {
        std::env::var_os(v).filter(|x| !x.is_empty()).map(PathBuf::from)
    }
    pub fn temp_dir() -> Option<PathBuf> {
        env("TEMP")
    }
    pub fn windows_dir() -> Option<PathBuf> {
        env("SystemRoot")
    }
    pub fn local_app_data() -> Option<PathBuf> {
        env("LOCALAPPDATA")
    }
    pub fn profile() -> Option<PathBuf> {
        None
    }
    pub fn program_files() -> Option<PathBuf> {
        None
    }
    pub fn program_files_x86() -> Option<PathBuf> {
        None
    }
}

fn system_root() -> PathBuf {
    known::windows_dir().unwrap_or_else(|| PathBuf::from(r"C:\Windows"))
}

/// Folders a target must never equal or contain.
fn protected_paths() -> Vec<PathBuf> {
    let win = system_root();
    let mut v = vec![win.clone()];
    // %SystemDrive%\
    if let Some(prefix) = win.components().next() {
        v.push(PathBuf::from(format!("{}\\", prefix.as_os_str().to_string_lossy())));
    }
    v.extend(known::profile());
    v.extend(known::program_files());
    v.extend(known::program_files_x86());
    v
}

fn norm_components(p: &Path) -> Vec<String> {
    p.components().map(|c| c.as_os_str().to_string_lossy().to_lowercase()).collect()
}

/// A path is safe to clean only if it is absolute, free of `.`/`..`, not a
/// drive root, and neither equals nor is an ancestor of a protected folder.
fn validate_target(path: &Path, protected: &[PathBuf]) -> bool {
    use std::path::Component;
    if !path.is_absolute() || path.parent().is_none() {
        return false;
    }
    if path.components().any(|c| matches!(c, Component::ParentDir | Component::CurDir)) {
        return false;
    }
    let mine = norm_components(path);
    !protected.iter().any(|p| norm_components(p).starts_with(&mine))
}

/// The folder whose contents are cleaned: the path itself, or the parent of a
/// wildcard path.
fn root_of(path: &Path) -> &Path {
    let wild = path.file_name().is_some_and(|n| n.to_string_lossy().contains('*'));
    if wild { path.parent().unwrap_or(path) } else { path }
}

/// `final_path` (as returned for an open handle) lies strictly inside `root`.
fn path_within(final_path: &str, root: &str) -> bool {
    let root = root.trim_end_matches('\\').to_lowercase();
    final_path.to_lowercase().starts_with(&format!("{root}\\"))
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
        t("user-temp", "Your temporary files", known::temp_dir().into_iter().collect(), None),
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
            known::local_app_data()
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
        t.bytes = valid_paths(t).into_iter().flat_map(|p| entries_of(p)).map(|e| measure(&e).0).sum();
    }
}

#[cfg(not(windows))]
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

/// Portable fallback (not used on Windows, where `winfs` does handle-based
/// deletion).
#[cfg(not(windows))]
fn delete(p: &Path, _root: &str, rep: &mut CleanupReport) {
    let Ok(m) = fs::symlink_metadata(p) else { return };
    if is_link(&m) {
        // Remove only the link itself, never what it points at.
        let _ = if is_dir_link(&m) { fs::remove_dir(p) } else { fs::remove_file(p) };
        return;
    }
    if m.is_dir() {
        if let Ok(rd) = fs::read_dir(p) {
            for e in rd.flatten() {
                delete(&e.path(), _root, rep);
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
        if let Err(e) = self.sys.start_service(self.name) {
            eprintln!("featherweight: could not restart service {}: {e}", self.name);
        }
    }
}

/// Target paths that pass validation (others are ignored entirely).
fn valid_paths(t: &CleanupTarget) -> Vec<&PathBuf> {
    let protected = protected_paths();
    t.paths.iter().filter(|p| validate_target(root_of(p), &protected)).collect()
}

/// Handle-based deletion: every object is opened once, inspected through that
/// handle, and deleted through the same handle, so a folder swapped for a
/// junction mid-clean can never redirect the delete outside the root.
#[cfg(windows)]
mod winfs {
    use super::{CleanupReport, measure, path_within};
    use std::os::windows::ffi::OsStrExt;
    use std::path::Path;
    use windows::Win32::Foundation::{CloseHandle, HANDLE};
    use windows::Win32::Storage::FileSystem::{
        BY_HANDLE_FILE_INFORMATION, CreateFileW, DELETE, FILE_ACCESS_RIGHTS,
        FILE_DISPOSITION_FLAG_DELETE, FILE_DISPOSITION_FLAG_IGNORE_READONLY_ATTRIBUTE,
        FILE_DISPOSITION_FLAG_POSIX_SEMANTICS, FILE_DISPOSITION_INFO, FILE_DISPOSITION_INFO_EX,
        FILE_DISPOSITION_INFO_EX_FLAGS, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_FLAGS_AND_ATTRIBUTES,
        FILE_READ_ATTRIBUTES, FILE_SHARE_MODE, FILE_WRITE_ATTRIBUTES, FileDispositionInfo,
        FileDispositionInfoEx, GETFINALPATHNAMEBYHANDLE_FLAGS, GetFileInformationByHandle,
        GetFinalPathNameByHandleW, OPEN_EXISTING, SetFileInformationByHandle,
    };
    use windows::core::PCWSTR;

    const ATTR_DIR: u32 = 0x10;
    const ATTR_REPARSE: u32 = 0x400;
    /// Others may read/write but not delete/rename while we hold the handle.
    const SHARE_RW: FILE_SHARE_MODE = FILE_SHARE_MODE(3);

    pub struct H(HANDLE);
    impl Drop for H {
        fn drop(&mut self) {
            // SAFETY: handle opened by us, closed once.
            unsafe {
                let _ = CloseHandle(self.0);
            }
        }
    }

    fn open(path: &Path, access: FILE_ACCESS_RIGHTS) -> std::io::Result<H> {
        let w: Vec<u16> = path.as_os_str().encode_wide().chain(Some(0)).collect();
        // SAFETY: `w` is NUL-terminated and outlives the call.
        unsafe {
            CreateFileW(
                PCWSTR(w.as_ptr()),
                access.0,
                SHARE_RW,
                None,
                OPEN_EXISTING,
                FILE_FLAGS_AND_ATTRIBUTES(
                    FILE_FLAG_BACKUP_SEMANTICS.0 | FILE_FLAG_OPEN_REPARSE_POINT.0,
                ),
                None,
            )
        }
        .map(H)
        .map_err(|e| std::io::Error::from_raw_os_error((e.code().0 & 0xFFFF) as i32))
    }

    fn info(h: &H) -> Option<BY_HANDLE_FILE_INFORMATION> {
        let mut i = BY_HANDLE_FILE_INFORMATION::default();
        // SAFETY: valid handle and out-pointer.
        unsafe { GetFileInformationByHandle(h.0, &mut i) }.ok()?;
        Some(i)
    }

    fn final_path(h: &H) -> Option<String> {
        let mut buf = vec![0u16; 512];
        loop {
            // SAFETY: valid handle; buffer valid for its length.
            let n = unsafe {
                GetFinalPathNameByHandleW(h.0, &mut buf, GETFINALPATHNAMEBYHANDLE_FLAGS(0))
            } as usize;
            if n == 0 {
                return None;
            }
            if n < buf.len() {
                return Some(String::from_utf16_lossy(&buf[..n]));
            }
            buf = vec![0u16; n + 1];
        }
    }

    /// Canonical (final) path of a cleanable root, or `None` if the root is
    /// missing, not a directory, or itself a reparse point.
    pub fn canonical_root(path: &Path) -> Option<String> {
        let h = open(path, FILE_READ_ATTRIBUTES).ok()?;
        let i = info(&h)?;
        if i.dwFileAttributes & ATTR_REPARSE != 0 || i.dwFileAttributes & ATTR_DIR == 0 {
            return None;
        }
        final_path(&h)
    }

    /// Marks the object behind `h` for deletion (never follows anything).
    fn set_delete(h: &H) -> bool {
        let ex = FILE_DISPOSITION_INFO_EX {
            Flags: FILE_DISPOSITION_INFO_EX_FLAGS(
                FILE_DISPOSITION_FLAG_DELETE.0
                    | FILE_DISPOSITION_FLAG_POSIX_SEMANTICS.0
                    | FILE_DISPOSITION_FLAG_IGNORE_READONLY_ATTRIBUTE.0,
            ),
        };
        // SAFETY: pointer/size describe a live FILE_DISPOSITION_INFO_EX.
        let r = unsafe {
            SetFileInformationByHandle(
                h.0,
                FileDispositionInfoEx,
                (&raw const ex).cast(),
                size_of::<FILE_DISPOSITION_INFO_EX>() as u32,
            )
        };
        if r.is_ok() {
            return true;
        }
        let old = FILE_DISPOSITION_INFO { DeleteFile: true };
        // SAFETY: as above.
        unsafe {
            SetFileInformationByHandle(
                h.0,
                FileDispositionInfo,
                (&raw const old).cast(),
                size_of::<FILE_DISPOSITION_INFO>() as u32,
            )
        }
        .is_ok()
    }

    pub fn delete(path: &Path, root: &str, rep: &mut CleanupReport) {
        let full = FILE_ACCESS_RIGHTS(DELETE.0 | FILE_READ_ATTRIBUTES.0 | FILE_WRITE_ATTRIBUTES.0);
        let plain = FILE_ACCESS_RIGHTS(DELETE.0 | FILE_READ_ATTRIBUTES.0);
        let h = match open(path, full).or_else(|_| open(path, plain)) {
            Ok(h) => h,
            Err(e) => {
                // 2 / 3: already gone.
                if !matches!(e.raw_os_error(), Some(2 | 3)) {
                    rep.skipped_files += measure(path).1.max(1);
                }
                return;
            }
        };
        let Some(i) = info(&h) else {
            rep.skipped_files += 1;
            return;
        };
        if i.dwFileAttributes & ATTR_REPARSE != 0 {
            // A link: delete the link itself through this handle, nothing else.
            if !set_delete(&h) {
                rep.skipped_files += 1;
            }
            return;
        }
        // The object we hold must really live inside the canonical root.
        match final_path(&h) {
            Some(fp) if path_within(&fp, root) => {}
            _ => {
                rep.skipped_files += 1;
                return;
            }
        }
        if i.dwFileAttributes & ATTR_DIR != 0 {
            // `h` pins the directory (no share-delete), so it cannot be swapped
            // while we enumerate it.
            if let Ok(rd) = std::fs::read_dir(path) {
                for e in rd.flatten() {
                    delete(&e.path(), root, rep);
                }
            }
            let _ = set_delete(&h); // fails harmlessly if something was skipped
            return;
        }
        let size = (u64::from(i.nFileSizeHigh) << 32) | u64::from(i.nFileSizeLow);
        if set_delete(&h) {
            rep.freed_bytes += size;
        } else {
            rep.skipped_files += 1;
        }
    }
}

/// Canonical root for deletion; `None` refuses the path.
fn canonical_root(root: &Path) -> Option<String> {
    #[cfg(windows)]
    {
        winfs::canonical_root(root)
    }
    #[cfg(not(windows))]
    {
        let m = fs::symlink_metadata(root).ok()?;
        (m.is_dir() && !is_link(&m)).then(|| root.to_string_lossy().into_owned())
    }
}

fn delete_entry(p: &Path, root: &str, rep: &mut CleanupReport) {
    #[cfg(windows)]
    winfs::delete(p, root, rep);
    #[cfg(not(windows))]
    delete(p, root, rep);
}

pub fn clean<S: System>(sys: &mut S, t: &CleanupTarget) -> CleanupReport {
    let mut rep = CleanupReport::default();
    // (canonical root, entries) per valid path; invalid paths are refused.
    let jobs: Vec<(String, Vec<PathBuf>)> = valid_paths(t)
        .into_iter()
        .filter_map(|p| Some((canonical_root(root_of(p))?, entries_of(p))))
        .collect();
    if jobs.is_empty() {
        return rep;
    }

    let mut _guard = None;
    if let Some(name) = t.needs_service_stop.as_deref() {
        match sys.stop_service(name) {
            Ok(true) => _guard = Some(Restart { sys, name }),
            Ok(false) => {}
            Err(e) => {
                // A stop may have been requested before the error (timeout):
                // make sure the service is running again. Nothing was asked
                // of the service if it was absent or access was denied.
                if !matches!(
                    e,
                    crate::system::SysError::NotFound(_) | crate::system::SysError::AccessDenied(_)
                ) {
                    let _ = Restart { sys, name };
                }
                // Can't safely delete: report everything as skipped.
                rep.skipped_files =
                    jobs.iter().flat_map(|(_, es)| es).map(|e| measure(e).1).sum();
                return rep;
            }
        }
    }
    for (root, entries) in &jobs {
        for e in entries {
            delete_entry(e, root, &mut rep);
        }
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
        // A failed stop may have half-happened, so a start is always tried.
        assert_eq!(sys.service_calls, vec!["stop:wuauserv", "start:wuauserv"]);
    }

    #[test]
    fn late_stop_failure_still_restarts_service() {
        let d = tempfile::tempdir().unwrap();
        write(&d.path().join("a.bin"), 10);
        let t = target(vec![d.path().to_path_buf()], Some("wuauserv"));
        let mut sys = FakeSystem::new().with_running_service("wuauserv");
        sys.stop_then_fail = true; // service really stops, but stop reports Err
        let r = clean(&mut sys, &t);
        assert_eq!(r.freed_bytes, 0);
        assert!(d.path().join("a.bin").exists());
        assert!(sys.running_services.contains("wuauserv"), "must be running again");
        assert_eq!(sys.service_calls, vec!["stop:wuauserv", "start:wuauserv"]);
    }

    #[test]
    fn absent_service_is_not_started_on_stop_error() {
        let d = tempfile::tempdir().unwrap();
        write(&d.path().join("a.bin"), 10);
        let t = target(vec![d.path().to_path_buf()], Some("nosuch"));
        let mut sys = FakeSystem::new();
        let r = clean(&mut sys, &t);
        assert_eq!(r.skipped_files, 1);
        assert_eq!(sys.service_calls, vec!["stop:nosuch"]);
    }

    #[test]
    fn path_within_refuses_outside_root() {
        let root = r"\\?\C:\Users\bob\AppData\Local\Temp";
        assert!(path_within(r"\\?\C:\Users\bob\AppData\Local\Temp\a\b.tmp", root));
        assert!(path_within(r"\\?\c:\users\BOB\appdata\local\temp\x", root));
        assert!(!path_within(r"\\?\C:\Users\bob\AppData\Local\Temp", root), "root itself");
        assert!(!path_within(r"\\?\C:\Users\bob\AppData\Local\Temp2\x", root), "sibling prefix");
        assert!(!path_within(r"\\?\C:\Windows\System32\x.dll", root));
    }

    #[cfg(windows)]
    #[test]
    fn validate_target_rules() {
        let prot: Vec<PathBuf> = [r"C:\Windows", r"C:\", r"C:\Users\bob", r"C:\Program Files"]
            .iter()
            .map(PathBuf::from)
            .collect();
        let ok = |p: &str| validate_target(Path::new(p), &prot);
        assert!(!ok("."));
        assert!(!ok("foo"));
        assert!(!ok("C:foo"));
        assert!(!ok(r"C:\Windows\.."));
        assert!(!ok(r"C:\"));
        assert!(!ok(r"C:\Windows"));
        assert!(!ok(r"C:\Users"), "ancestor of the profile");
        assert!(!ok(r"c:\users\BOB"));
        assert!(!ok(r"C:\Program Files"));
        assert!(ok(r"C:\Users\bob\AppData\Local\Temp"));
        assert!(ok(r"C:\Windows\SoftwareDistribution\Download"));
    }

    #[cfg(windows)]
    #[test]
    fn clean_refuses_invalid_target_path() {
        let mut sys = FakeSystem::new().with_running_service("wuauserv");
        let t = target(vec![PathBuf::from("relative"), PathBuf::from(r"C:\")], Some("wuauserv"));
        let r = clean(&mut sys, &t);
        assert_eq!(r, CleanupReport::default());
        assert!(sys.service_calls.is_empty(), "service untouched");
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
