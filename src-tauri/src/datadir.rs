//! The protected data folder (`%ProgramData%\Featherweight`) that holds the
//! undo journal. The app runs elevated and trusts the journal to decide what
//! to write back on undo, so a standard user must not be able to plant or
//! tamper with it: the folder is owned by Administrators and its DACL grants
//! access only to Administrators and SYSTEM, without inheritance.

use std::path::{Path, PathBuf};

use windows::Win32::Foundation::{
    CloseHandle, ERROR_ALREADY_EXISTS, ERROR_SUCCESS, HANDLE, HLOCAL, LocalFree,
};
use windows::Win32::Security::Authorization::{
    ConvertStringSecurityDescriptorToSecurityDescriptorW, GetSecurityInfo, SDDL_REVISION_1,
    SE_FILE_OBJECT, SetSecurityInfo,
};
use windows::Win32::Security::{
    ACL, DACL_SECURITY_INFORMATION, GetSecurityDescriptorDacl, GetSecurityDescriptorOwner,
    IsWellKnownSid, OWNER_SECURITY_INFORMATION, PROTECTED_DACL_SECURITY_INFORMATION,
    PSECURITY_DESCRIPTOR, PSID, SECURITY_ATTRIBUTES, WinBuiltinAdministratorsSid,
    WinLocalSystemSid,
};
use windows::Win32::Storage::FileSystem::{
    BY_HANDLE_FILE_INFORMATION, CreateDirectoryW, CreateFileW, FILE_FLAG_BACKUP_SEMANTICS,
    FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_DELETE, FILE_SHARE_READ, FILE_SHARE_WRITE,
    GetFileInformationByHandle, OPEN_EXISTING, READ_CONTROL, WRITE_DAC, WRITE_OWNER,
};
use windows::core::HSTRING;

/// Folder: owner Administrators, full control for Administrators and SYSTEM
/// (inherited by children), protected from inheriting anything else.
const DIR_SDDL: &str = "O:BAD:P(A;OICI;FA;;;BA)(A;OICI;FA;;;SY)";
/// Journal file: same principals, no inheritance flags.
const FILE_SDDL: &str = "O:BAD:P(A;;FA;;;BA)(A;;FA;;;SY)";

const ATTR_DIR: u32 = 0x10;
const ATTR_REPARSE: u32 = 0x400;

pub const JOURNAL_FILE: &str = "journal.json";

/// Creates or re-secures `%ProgramData%\Featherweight` and returns it. Fails
/// (and the journal must not be loaded) if the folder or the journal file is
/// a link, or is owned by anyone but Administrators or SYSTEM.
pub fn secure_data_dir() -> Result<PathBuf, String> {
    let base = crate::cleanup::known::program_data()
        .ok_or_else(|| "could not locate the ProgramData folder".to_string())?;
    let dir = base.join("Featherweight");
    secure_dir_at(&dir)?;
    Ok(dir)
}

/// Owned security descriptor from `ConvertStringSecurityDescriptorToSecurityDescriptorW`.
struct Sd(PSECURITY_DESCRIPTOR);

impl Sd {
    fn from_sddl(sddl: &str) -> Result<Sd, String> {
        let mut sd = PSECURITY_DESCRIPTOR::default();
        // SAFETY: valid NUL-terminated string and out-pointer; freed in Drop.
        unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                &HSTRING::from(sddl),
                SDDL_REVISION_1,
                &mut sd,
                None,
            )
        }
        .map_err(|e| format!("could not build the data folder's permissions: {e}"))?;
        Ok(Sd(sd))
    }

    fn owner(&self) -> Result<PSID, String> {
        let mut owner = PSID::default();
        let mut defaulted = Default::default();
        // SAFETY: `self.0` is a valid self-relative descriptor.
        unsafe { GetSecurityDescriptorOwner(self.0, &mut owner, &mut defaulted) }
            .map_err(|e| format!("bad security descriptor: {e}"))?;
        Ok(owner)
    }

    fn dacl(&self) -> Result<*mut ACL, String> {
        let mut present = Default::default();
        let mut dacl: *mut ACL = std::ptr::null_mut();
        let mut defaulted = Default::default();
        // SAFETY: `self.0` is a valid self-relative descriptor.
        unsafe { GetSecurityDescriptorDacl(self.0, &mut present, &mut dacl, &mut defaulted) }
            .map_err(|e| format!("bad security descriptor: {e}"))?;
        if dacl.is_null() {
            return Err("bad security descriptor: no DACL".into());
        }
        Ok(dacl)
    }
}

impl Drop for Sd {
    fn drop(&mut self) {
        // SAFETY: allocated by the SDDL conversion with LocalAlloc; freed once.
        unsafe {
            LocalFree(Some(HLOCAL(self.0.0)));
        }
    }
}

struct Handle(HANDLE);

impl Drop for Handle {
    fn drop(&mut self) {
        // SAFETY: handle came from CreateFileW and is closed once.
        unsafe {
            let _ = CloseHandle(self.0);
        }
    }
}

/// Opens `path` itself (never a link target) for reading and changing its
/// owner and DACL. `Ok(None)` if it does not exist.
fn open_for_acl(path: &Path) -> Result<Option<Handle>, String> {
    // SAFETY: valid path string; the handle is owned by `Handle`.
    let r = unsafe {
        CreateFileW(
            &HSTRING::from(path.as_os_str()),
            (READ_CONTROL | WRITE_DAC | WRITE_OWNER).0,
            FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE,
            None,
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
            None,
        )
    };
    match r {
        Ok(h) => Ok(Some(Handle(h))),
        Err(e) if matches!(e.code().0 as u32 & 0xffff, 2 | 3) => Ok(None), // file/path not found
        Err(e) => Err(format!("could not open {}: {e}", path.display())),
    }
}

fn attributes(h: &Handle, path: &Path) -> Result<BY_HANDLE_FILE_INFORMATION, String> {
    let mut info = BY_HANDLE_FILE_INFORMATION::default();
    // SAFETY: valid handle and out-pointer.
    unsafe { GetFileInformationByHandle(h.0, &mut info) }
        .map_err(|e| format!("could not inspect {}: {e}", path.display()))?;
    Ok(info)
}

/// Whether `path`'s owner is Administrators or SYSTEM.
fn owner_is_trusted(h: &Handle, path: &Path) -> Result<bool, String> {
    let mut owner = PSID::default();
    let mut sd = PSECURITY_DESCRIPTOR::default();
    // SAFETY: valid handle; `sd` receives a LocalAlloc'd descriptor that owns
    // `owner` and is freed below.
    let rc = unsafe {
        GetSecurityInfo(
            h.0,
            SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION,
            Some(&mut owner),
            None,
            None,
            None,
            Some(&mut sd),
        )
    };
    if rc != ERROR_SUCCESS {
        return Err(format!("could not read the owner of {}: error {}", path.display(), rc.0));
    }
    // SAFETY: `owner` points into `sd`, which is still alive.
    let trusted = unsafe {
        IsWellKnownSid(owner, WinBuiltinAdministratorsSid).as_bool()
            || IsWellKnownSid(owner, WinLocalSystemSid).as_bool()
    };
    // SAFETY: allocated by GetSecurityInfo; freed once.
    unsafe {
        LocalFree(Some(HLOCAL(sd.0)));
    }
    Ok(trusted)
}

/// Sets owner Administrators and the protected DACL from `sd` on `h`.
fn apply(h: &Handle, sd: &Sd, path: &Path) -> Result<(), String> {
    let owner = sd.owner()?;
    let dacl = sd.dacl()?;
    // SAFETY: valid handle; `owner` and `dacl` point into `sd`.
    let rc = unsafe {
        SetSecurityInfo(
            h.0,
            SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION | PROTECTED_DACL_SECURITY_INFORMATION,
            Some(owner),
            None,
            Some(dacl),
            None,
        )
    };
    if rc != ERROR_SUCCESS {
        return Err(format!("could not secure {}: error {}", path.display(), rc.0));
    }
    Ok(())
}

fn untrusted(path: &Path, why: &str) -> String {
    format!(
        "{} {why}, so the undo journal can't be trusted. Undo and new changes are turned off until an administrator deletes that folder.",
        path.display()
    )
}

/// Creates `dir` with the protected DACL, or verifies and re-secures an
/// existing one together with its journal file.
pub fn secure_dir_at(dir: &Path) -> Result<(), String> {
    let dir_sd = Sd::from_sddl(DIR_SDDL)?;
    let sa = SECURITY_ATTRIBUTES {
        nLength: std::mem::size_of::<SECURITY_ATTRIBUTES>() as u32,
        lpSecurityDescriptor: dir_sd.0.0,
        bInheritHandle: false.into(),
    };
    // SAFETY: valid path and security attributes that outlive the call.
    let created = match unsafe { CreateDirectoryW(&HSTRING::from(dir.as_os_str()), Some(&sa)) } {
        Ok(()) => true,
        Err(e) if e.code() == ERROR_ALREADY_EXISTS.to_hresult() => false,
        Err(e) => return Err(format!("could not create {}: {e}", dir.display())),
    };

    let h = open_for_acl(dir)?.ok_or_else(|| format!("{} disappeared", dir.display()))?;
    let info = attributes(&h, dir)?;
    if info.dwFileAttributes & ATTR_REPARSE != 0 {
        return Err(untrusted(dir, "is a link"));
    }
    if info.dwFileAttributes & ATTR_DIR == 0 {
        return Err(untrusted(dir, "is not a folder"));
    }
    if !created {
        if !owner_is_trusted(&h, dir)? {
            return Err(untrusted(dir, "is not owned by Administrators or SYSTEM"));
        }
        // Drop any write access someone else might have had.
        apply(&h, &dir_sd, dir)?;
    }

    let journal = dir.join(JOURNAL_FILE);
    if let Some(fh) = open_for_acl(&journal)? {
        let info = attributes(&fh, &journal)?;
        if info.dwFileAttributes & (ATTR_REPARSE | ATTR_DIR) != 0 {
            return Err(untrusted(&journal, "is not a regular file"));
        }
        if info.nNumberOfLinks != 1 {
            return Err(untrusted(&journal, "has extra hard links"));
        }
        if !owner_is_trusted(&fh, &journal)? {
            return Err(untrusted(&journal, "is not owned by Administrators or SYSTEM"));
        }
        apply(&fh, &Sd::from_sddl(FILE_SDDL)?, &journal)?;
    }

    // A leftover temp file could have been planted before the folder was
    // secured; the journal always writes a fresh one.
    let mut tmp = journal.into_os_string();
    tmp.push(".tmp");
    match std::fs::remove_file(&tmp) {
        Ok(()) => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(format!("could not remove {}: {e}", PathBuf::from(tmp).display())),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sddl_parses_and_has_owner_and_dacl() {
        for s in [DIR_SDDL, FILE_SDDL] {
            let sd = Sd::from_sddl(s).unwrap();
            let owner = sd.owner().unwrap();
            assert!(unsafe { IsWellKnownSid(owner, WinBuiltinAdministratorsSid) }.as_bool());
            sd.dacl().unwrap();
        }
    }

    #[test]
    fn refuses_existing_folder_owned_by_someone_else() {
        // A folder this (normally unelevated) test process creates is owned
        // by the current user, which must be refused. If the tests run
        // elevated, the owner is Administrators and the folder is accepted.
        let d = tempfile::tempdir().unwrap();
        let dir = d.path().join("fw");
        std::fs::create_dir(&dir).unwrap();
        let h = open_for_acl(&dir).unwrap().unwrap();
        let trusted = owner_is_trusted(&h, &dir).unwrap();
        drop(h);
        let r = secure_dir_at(&dir);
        if trusted {
            assert!(r.is_ok(), "{r:?}");
        } else {
            let e = r.unwrap_err();
            assert!(e.contains("not owned by Administrators or SYSTEM"), "{e}");
        }
    }

    #[test]
    fn refuses_a_file_where_the_folder_should_be() {
        let d = tempfile::tempdir().unwrap();
        let p = d.path().join("fw");
        std::fs::write(&p, b"x").unwrap();
        let e = secure_dir_at(&p).unwrap_err();
        assert!(e.contains("not a folder"), "{e}");
    }
}
