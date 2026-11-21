//! Integration tests against the real Windows. Ignored by default; run with
//! `cargo test --test windows_integration -- --ignored`. They only touch
//! `HKCU\Software\FeatherweightTest` and perform read-only queries elsewhere.
#![cfg(windows)]

use featherweight_lib::model::{Hive, RegData, StartType};
use featherweight_lib::system::windows::WindowsSystem;
use featherweight_lib::system::{SysError, System};

const KEY: &str = r"Software\FeatherweightTest";

/// Deletes the test key tree on drop, even when an assertion fails.
struct Cleanup;
impl Drop for Cleanup {
    fn drop(&mut self) {
        let hkcu = winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER);
        let _ = hkcu.delete_subkey_all(KEY);
    }
}

#[test]
#[ignore]
fn reg_roundtrip_dword_string_delete() {
    let _c = Cleanup;
    let mut sys = WindowsSystem;
    assert_eq!(sys.reg_get(Hive::Hkcu, KEY, "D").unwrap(), None);
    let cases = [
        ("D", RegData::Dword(7)),
        ("Q", RegData::Qword(1 << 40)),
        ("S", RegData::String("hello".into())),
        ("B", RegData::Binary(vec![1, 2, 3])),
    ];
    for (name, v) in &cases {
        sys.reg_set(Hive::Hkcu, KEY, name, Some(v)).unwrap();
        assert_eq!(sys.reg_get(Hive::Hkcu, KEY, name).unwrap().as_ref(), Some(v));
    }
    for (name, _) in &cases {
        sys.reg_set(Hive::Hkcu, KEY, name, None).unwrap();
        assert_eq!(sys.reg_get(Hive::Hkcu, KEY, name).unwrap(), None);
    }
    // deleting an already-missing value is fine
    sys.reg_set(Hive::Hkcu, KEY, "D", None).unwrap();
}

#[test]
#[ignore]
fn reads_start_type_of_a_known_service() {
    let sys = WindowsSystem;
    assert_eq!(sys.service_start("EventLog").unwrap(), Some(StartType::Automatic));
}

#[test]
#[ignore]
fn missing_service_is_none() {
    assert_eq!(WindowsSystem.service_start("FwNoSuchService").unwrap(), None);
}

#[test]
#[ignore]
fn missing_task_is_none() {
    assert_eq!(WindowsSystem.task_enabled(r"\FwNoSuchTask\Nope").unwrap(), None);
}

#[test]
#[ignore]
fn reads_task_state_without_error() {
    // Read-only; the task may be absent or unreadable without elevation.
    match WindowsSystem.task_enabled(r"\Microsoft\Windows\Defrag\ScheduledDefrag") {
        Ok(_) | Err(SysError::AccessDenied(_)) => {}
        Err(e) => panic!("{e}"),
    }
}

#[test]
#[ignore]
fn appx_unknown_package_is_none() {
    match WindowsSystem.appx_installed("Fw.NoSuchPackage") {
        Ok(v) => assert_eq!(v, None),
        Err(SysError::AccessDenied(_)) => {} // -AllUsers may need elevation
        Err(e) => panic!("{e}"),
    }
}

#[test]
#[ignore]
fn appx_rejects_unsafe_name() {
    assert!(matches!(
        WindowsSystem.appx_installed(r"x'; Remove-Item C:\ #"),
        Err(SysError::Other(_))
    ));
}

#[test]
#[ignore]
fn active_power_scheme_is_guid() {
    let g = WindowsSystem.active_power_scheme().unwrap();
    assert_eq!(g.len(), 36, "{g}");
    assert_eq!(g, g.to_lowercase());
    let parts: Vec<usize> = g.split('-').map(str::len).collect();
    assert_eq!(parts, vec![8, 4, 4, 4, 12]);
}

#[test]
#[ignore]
fn powershell_echo() {
    assert_eq!(WindowsSystem.run_powershell("Write-Output 1").unwrap(), "1");
}

#[test]
#[ignore]
fn reg_values_lists_run_key_read_only() {
    let sys = WindowsSystem;
    let vals = sys
        .reg_values(Hive::Hkcu, r"Software\Microsoft\Windows\CurrentVersion\Run")
        .unwrap();
    println!("{} values", vals.len());
    // A missing key is an empty list, not an error.
    assert!(sys.reg_values(Hive::Hkcu, r"Software\FeatherweightNoSuchKey").unwrap().is_empty());
}
