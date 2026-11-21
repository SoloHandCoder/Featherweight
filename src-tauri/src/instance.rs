//! One Featherweight at a time: two elevated instances would race each
//! other on the same undo journal and the same system state.

use windows::Win32::Foundation::{
    CloseHandle, ERROR_ACCESS_DENIED, ERROR_ALREADY_EXISTS, GetLastError, HANDLE,
};
use windows::Win32::System::Threading::CreateMutexW;
use windows::Win32::UI::WindowsAndMessaging::{MB_ICONINFORMATION, MB_OK, MessageBoxW};
use windows::core::HSTRING;

/// Session-independent name, so a second copy started from another account
/// (fast user switching, Run as different user) is caught too.
pub const MUTEX_NAME: &str = r"Global\FeatherweightSingleInstance";

/// Holds the named mutex for as long as it lives.
pub struct InstanceGuard(Option<HANDLE>);

impl Drop for InstanceGuard {
    fn drop(&mut self) {
        if let Some(h) = self.0.take() {
            // SAFETY: the handle was created by us and is closed exactly once.
            unsafe {
                let _ = CloseHandle(h);
            }
        }
    }
}

pub enum Claim {
    /// We are the only instance; keep the guard alive until exit.
    First(InstanceGuard),
    AlreadyRunning,
}

/// Creates (or detects) the named mutex `name`.
pub fn claim(name: &str) -> Claim {
    // SAFETY: valid name; no security attributes (default DACL).
    let created = unsafe { CreateMutexW(None, true, &HSTRING::from(name)) };
    // SAFETY: read immediately after the call it describes.
    let last = unsafe { GetLastError() };
    match created {
        Ok(h) if last == ERROR_ALREADY_EXISTS => {
            // SAFETY: our handle to the existing mutex, closed once.
            unsafe {
                let _ = CloseHandle(h);
            }
            Claim::AlreadyRunning
        }
        Ok(h) => Claim::First(InstanceGuard(Some(h))),
        // Another account's instance owns it with a DACL that keeps us out.
        Err(e) if e.code() == ERROR_ACCESS_DENIED.to_hresult() => Claim::AlreadyRunning,
        // Can't tell (e.g. no right to create global objects): don't block startup.
        Err(_) => Claim::First(InstanceGuard(None)),
    }
}

/// Native message box for the second instance (the webview never starts).
pub fn show_already_running() {
    // SAFETY: plain modal message box with static strings.
    unsafe {
        MessageBoxW(
            None,
            &HSTRING::from("Featherweight is already running."),
            &HSTRING::from("Featherweight"),
            MB_OK | MB_ICONINFORMATION,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn second_claim_sees_the_first_until_it_is_dropped() {
        // A per-session, per-test name: never collides with a real instance.
        let name = format!(r"Local\FeatherweightTest-{}", std::process::id());
        let first = claim(&name);
        assert!(matches!(first, Claim::First(InstanceGuard(Some(_)))));
        assert!(matches!(claim(&name), Claim::AlreadyRunning));
        drop(first);
        assert!(matches!(claim(&name), Claim::First(_)));
    }
}
