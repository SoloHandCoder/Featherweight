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
    let mut sys = WindowsSystem::new();
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
    let sys = WindowsSystem::new();
    assert_eq!(sys.service_start("EventLog").unwrap(), Some(StartType::Automatic));
}

#[test]
#[ignore]
fn missing_service_is_none() {
    assert_eq!(WindowsSystem::new().service_start("FwNoSuchService").unwrap(), None);
}

#[test]
#[ignore]
fn missing_task_is_none() {
    assert_eq!(WindowsSystem::new().task_enabled(r"\FwNoSuchTask\Nope").unwrap(), None);
}

#[test]
#[ignore]
fn reads_task_state() {
    let r = WindowsSystem::new().task_enabled(r"\Microsoft\Windows\Defrag\ScheduledDefrag");
    if elevated() {
        assert!(matches!(r, Ok(Some(_))), "{r:?}");
    } else {
        match r {
            Ok(_) | Err(SysError::AccessDenied(_)) => {}
            Err(e) => panic!("{e}"),
        }
    }
}

/// `net session` succeeds only for an elevated process.
fn elevated() -> bool {
    std::process::Command::new("net")
        .arg("session")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

#[test]
#[ignore]
fn appx_missing_package_is_false() {
    assert_eq!(WindowsSystem::new().appx_installed("Fw.NoSuchPackage").unwrap(), Some(false));
}

#[test]
#[ignore]
fn appx_rejects_unsafe_name() {
    assert!(matches!(
        WindowsSystem::new().appx_installed(r"x'; Remove-Item C:\ #"),
        Err(SysError::Other(_))
    ));
}

#[test]
#[ignore]
fn active_power_scheme_is_guid() {
    let g = WindowsSystem::new().active_power_scheme().unwrap();
    assert_eq!(g.len(), 36, "{g}");
    assert_eq!(g, g.to_lowercase());
    let parts: Vec<usize> = g.split('-').map(str::len).collect();
    assert_eq!(parts, vec![8, 4, 4, 4, 12]);
}

#[test]
#[ignore]
fn powershell_echo() {
    assert_eq!(WindowsSystem::new().run_powershell("Write-Output 1").unwrap(), "1");
}

#[test]
#[ignore]
fn reg_values_lists_run_key_read_only() {
    let sys = WindowsSystem::new();
    let vals = sys
        .reg_values(Hive::Hkcu, r"Software\Microsoft\Windows\CurrentVersion\Run")
        .unwrap();
    println!("{} values", vals.len());
    // A missing key is an empty list, not an error.
    assert!(sys.reg_values(Hive::Hkcu, r"Software\FeatherweightNoSuchKey").unwrap().is_empty());
}

#[test]
#[ignore]
fn reg_roundtrip_expand_string() {
    let _c = Cleanup;
    let mut sys = WindowsSystem::new();
    let v = RegData::ExpandString(r"%APPDATA%\x.exe".into());
    sys.reg_set(Hive::Hkcu, KEY, "E", Some(&v)).unwrap();
    // Type is preserved (not downgraded to REG_SZ) and nothing is expanded.
    assert_eq!(sys.reg_get(Hive::Hkcu, KEY, "E").unwrap(), Some(v.clone()));
    assert!(sys.reg_values(Hive::Hkcu, KEY).unwrap().contains(&("E".to_string(), v)));
    sys.reg_set(Hive::Hkcu, KEY, "E", None).unwrap();
    assert_eq!(sys.reg_get(Hive::Hkcu, KEY, "E").unwrap(), None);
}

#[test]
#[ignore]
fn unsupported_registry_type_is_other_error() {
    use winreg::enums::{HKEY_CURRENT_USER, KEY_WRITE, REG_MULTI_SZ};
    let _c = Cleanup;
    let hkcu = winreg::RegKey::predef(HKEY_CURRENT_USER);
    let (k, _) = hkcu.create_subkey_with_flags(KEY, KEY_WRITE).unwrap();
    k.set_raw_value(
        "M",
        &winreg::RegValue { bytes: vec![0, 0, 0, 0].into(), vtype: REG_MULTI_SZ },
    )
    .unwrap();
    let sys = WindowsSystem::new();
    assert!(matches!(sys.reg_get(Hive::Hkcu, KEY, "M"), Err(SysError::Other(_))));
    // reg_values skips it rather than failing.
    assert!(sys.reg_values(Hive::Hkcu, KEY).unwrap().is_empty());
}
