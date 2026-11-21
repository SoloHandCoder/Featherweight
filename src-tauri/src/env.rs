//! Environment detection and preset resolution.
#![allow(dead_code)]

use crate::catalog::TweakDef;
use crate::engine::{Engine, TweakStatus};
pub use crate::engine::Env;
use crate::model::{Level, Preset};
use crate::system::System;

/// The level a tweak has on this PC: `laptop_level` wins on laptops.
pub fn effective_level(t: &TweakDef, env: &Env) -> Level {
    if env.is_laptop { t.laptop_level.unwrap_or(t.level) } else { t.level }
}

/// Ids (catalog order) of tweaks in preset `p` that are Safe on this PC and
/// applicable here. Startup apps are not catalog tweaks, so never included.
pub fn resolve_preset<S: System>(engine: &mut Engine<S>, p: Preset) -> Vec<String> {
    let candidates: Vec<String> = engine
        .catalog
        .tweaks
        .iter()
        .filter(|t| t.presets.contains(&p) && effective_level(t, &engine.env) == Level::Safe)
        .map(|t| t.id.clone())
        .collect();
    candidates
        .into_iter()
        .filter(|id| !matches!(engine.status(id), TweakStatus::NotApplicable(_)))
        .collect()
}

pub fn account_message(env: &Env) -> Option<String> {
    (!env.per_user_allowed).then(|| {
        "You're running Featherweight with a different administrator account than the one signed in. Settings that only affect your account are turned off. Sign in as an administrator to use them."
            .to_string()
    })
}

/// Reads the build, laptop-ness and signed-in-account match of this PC.
#[cfg(windows)]
pub fn detect_env() -> Env {
    Env {
        build: win::build_number(),
        is_laptop: win::is_laptop(),
        // If the explorer.exe owner can't be determined (e.g. no explorer is
        // running) assume the common case: the admin at the keyboard.
        per_user_allowed: win::same_user_as_explorer().unwrap_or(true),
    }
}

#[cfg(windows)]
mod win {
    use std::mem::size_of;

    use windows::Wdk::System::SystemServices::RtlGetVersion;
    use windows::Win32::Foundation::{CloseHandle, HANDLE};
    use windows::Win32::Security::{
        EqualSid, GetTokenInformation, PSID, TOKEN_QUERY, TOKEN_USER, TokenUser,
    };
    use windows::Win32::System::Diagnostics::ToolHelp::{
        CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW,
        TH32CS_SNAPPROCESS,
    };
    use windows::Win32::System::Power::{GetSystemPowerStatus, SYSTEM_POWER_STATUS};
    use windows::Win32::System::RemoteDesktop::{ProcessIdToSessionId, WTSGetActiveConsoleSessionId};
    use windows::Win32::System::SystemInformation::OSVERSIONINFOW;
    use windows::Win32::System::Threading::{
        GetCurrentProcess, OpenProcess, OpenProcessToken, PROCESS_QUERY_LIMITED_INFORMATION,
    };

    pub fn build_number() -> u32 {
        let mut info = OSVERSIONINFOW {
            dwOSVersionInfoSize: size_of::<OSVERSIONINFOW>() as u32,
            ..Default::default()
        };
        // SAFETY: `info` is a valid, correctly sized OSVERSIONINFOW.
        let _ = unsafe { RtlGetVersion(&mut info) };
        info.dwBuildNumber
    }

    /// A battery (flag not 128 = "no battery", not 255 = unknown) means laptop.
    pub fn is_laptop() -> bool {
        let mut st = SYSTEM_POWER_STATUS::default();
        // SAFETY: `st` is a valid out-pointer.
        if unsafe { GetSystemPowerStatus(&mut st) }.is_err() {
            return false;
        }
        st.BatteryFlag != 128 && st.BatteryFlag != 255
    }

    /// Closes the wrapped handle on drop.
    struct Owned(HANDLE);
    impl Drop for Owned {
        fn drop(&mut self) {
            // SAFETY: the handle was opened by us and is closed exactly once.
            unsafe { let _ = CloseHandle(self.0); }
        }
    }

    /// TOKEN_USER blob (the SID points inside the buffer) for a process handle.
    fn token_user(process: HANDLE) -> Option<Vec<u8>> {
        let mut token = HANDLE::default();
        // SAFETY: valid process handle and out-pointer.
        unsafe { OpenProcessToken(process, TOKEN_QUERY, &mut token) }.ok()?;
        let token = Owned(token);
        let mut len = 0u32;
        // SAFETY: first call only asks for the required size.
        let _ = unsafe { GetTokenInformation(token.0, TokenUser, None, 0, &mut len) };
        if len == 0 {
            return None;
        }
        let mut buf = vec![0u8; len as usize];
        // SAFETY: `buf` is `len` bytes long.
        unsafe {
            GetTokenInformation(token.0, TokenUser, Some(buf.as_mut_ptr().cast()), len, &mut len)
        }
        .ok()?;
        Some(buf)
    }

    fn sid_of(blob: &[u8]) -> PSID {
        // SAFETY: `blob` was filled by GetTokenInformation(TokenUser).
        unsafe { (*blob.as_ptr().cast::<TOKEN_USER>()).User.Sid }
    }

    /// pid of explorer.exe running in the active console session.
    fn console_explorer_pid() -> Option<u32> {
        // SAFETY: plain Win32 calls with valid arguments.
        unsafe {
            let console = WTSGetActiveConsoleSessionId();
            let snap = Owned(CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0).ok()?);
            let mut e = PROCESSENTRY32W {
                dwSize: size_of::<PROCESSENTRY32W>() as u32,
                ..Default::default()
            };
            let mut more = Process32FirstW(snap.0, &mut e).is_ok();
            while more {
                let len = e.szExeFile.iter().position(|&c| c == 0).unwrap_or(e.szExeFile.len());
                let name = String::from_utf16_lossy(&e.szExeFile[..len]);
                if name.eq_ignore_ascii_case("explorer.exe") {
                    let mut session = 0u32;
                    if ProcessIdToSessionId(e.th32ProcessID, &mut session).is_ok()
                        && session == console
                    {
                        return Some(e.th32ProcessID);
                    }
                }
                more = Process32NextW(snap.0, &mut e).is_ok();
            }
            None
        }
    }

    /// `None` when explorer's owner can't be determined.
    pub fn same_user_as_explorer() -> Option<bool> {
        let pid = console_explorer_pid()?;
        // SAFETY: valid pid; handle is closed by `Owned`.
        let explorer = Owned(unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }.ok()?);
        let theirs = token_user(explorer.0)?;
        // SAFETY: pseudo-handle, needs no closing.
        let ours = token_user(unsafe { GetCurrentProcess() })?;
        // SAFETY: both SIDs live inside the live buffers above.
        Some(unsafe { EqualSid(sid_of(&ours), sid_of(&theirs)) }.is_ok())
    }
}

#[cfg(all(test, windows))]
mod smoke {
    /// Read-only; run manually: `cargo test detect_env_smoke -- --ignored --nocapture`.
    #[test]
    #[ignore]
    fn detect_env_smoke() {
        println!("{:?}", super::detect_env());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::Catalog;
    use crate::journal::Journal;
    use crate::model::{Hive, RegData};
    use crate::system::fake::FakeSystem;

    fn tweak(id: &str, section: &str, extra: &str, svc: Option<&str>) -> String {
        let action = match svc {
            Some(s) => format!("[[actions]]\ntype = \"service\"\nname = \"{s}\"\nstart = \"disabled\"\n"),
            None => format!(
                "[[actions]]\ntype = \"registry\"\nhive = \"hklm\"\npath = 'Software\\{id}'\nname = \"V\"\nvalue = {{ dword = 1 }}\n"
            ),
        };
        format!(
            "id = \"{id}\"\ntitle = \"T\"\ndescription = \"D\"\nsection = \"{section}\"\n{extra}\n{action}"
        )
    }

    fn env(laptop: bool) -> Env {
        Env { build: 19045, is_laptop: laptop, per_user_allowed: true }
    }

    fn engine(srcs: &[String], laptop: bool, d: &tempfile::TempDir) -> Engine<FakeSystem> {
        let refs: Vec<&str> = srcs.iter().map(String::as_str).collect();
        let catalog = Catalog::from_strs(&refs).unwrap();
        let journal = Journal::load(d.path().join("j.json")).unwrap();
        let sys = FakeSystem::new().with_reg(Hive::Hklm, r"Software\x", "V", RegData::Dword(0));
        Engine::new(sys, catalog, journal, env(laptop))
    }

    #[test]
    fn laptop_promotes_high_perf_to_advanced() {
        let srcs = vec![
            tweak("plan", "gaming", "level = \"safe\"\nlaptop_level = \"advanced\"\npresets = [\"gaming\"]", None),
            tweak("plain", "gaming", "level = \"safe\"\npresets = [\"gaming\"]", None),
        ];
        let d = tempfile::tempdir().unwrap();
        let mut desktop = engine(&srcs, false, &d);
        assert_eq!(resolve_preset(&mut desktop, Preset::Gaming), vec!["plan", "plain"]);
        let mut laptop = engine(&srcs, true, &d);
        assert_eq!(resolve_preset(&mut laptop, Preset::Gaming), vec!["plain"]);
        assert_eq!(effective_level(&laptop.catalog.tweaks[0], &laptop.env), Level::Advanced);
        assert_eq!(effective_level(&desktop.catalog.tweaks[0], &desktop.env), Level::Safe);
    }

    #[test]
    fn everyday_excludes_advanced_and_gaming_section() {
        let srcs = vec![
            tweak("e-safe", "background", "level = \"safe\"\npresets = [\"everyday\", \"gaming\"]", None),
            tweak("e-adv", "background", "level = \"advanced\"\npresets = [\"everyday\"]", None),
            tweak("g-only", "gaming", "level = \"safe\"\npresets = [\"gaming\"]", None),
            tweak("none", "cleanup", "level = \"safe\"", None),
        ];
        let d = tempfile::tempdir().unwrap();
        let mut e = engine(&srcs, false, &d);
        assert_eq!(resolve_preset(&mut e, Preset::Everyday), vec!["e-safe"]);
    }

    #[test]
    fn gaming_includes_everyday() {
        let srcs = vec![
            tweak("e-safe", "background", "level = \"safe\"\npresets = [\"everyday\", \"gaming\"]", None),
            tweak("g-only", "gaming", "level = \"safe\"\npresets = [\"gaming\"]", None),
        ];
        let d = tempfile::tempdir().unwrap();
        let mut e = engine(&srcs, false, &d);
        assert_eq!(resolve_preset(&mut e, Preset::Gaming), vec!["e-safe", "g-only"]);
    }

    #[test]
    fn preset_excludes_not_applicable() {
        let srcs = vec![
            tweak("ok", "background", "level = \"safe\"\npresets = [\"everyday\"]", None),
            tweak("newer", "background", "level = \"safe\"\npresets = [\"everyday\"]\nmin_build = 22000", None),
            tweak("nosvc", "background", "level = \"safe\"\npresets = [\"everyday\"]", Some("Missing")),
        ];
        let d = tempfile::tempdir().unwrap();
        let mut e = engine(&srcs, false, &d);
        assert_eq!(resolve_preset(&mut e, Preset::Everyday), vec!["ok"]);
    }

    #[test]
    fn account_message_only_when_mismatch() {
        assert_eq!(account_message(&env(false)), None);
        let e = Env { build: 1, is_laptop: false, per_user_allowed: false };
        assert_eq!(
            account_message(&e).unwrap(),
            "You're running Featherweight with a different administrator account than the one signed in. Settings that only affect your account are turned off. Sign in as an administrator to use them."
        );
    }
}
