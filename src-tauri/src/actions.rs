//! Per-action observe / target / apply / restore / describe.
#![allow(dead_code)]

use crate::catalog::{ActionDef, AllUsers};
use crate::model::{Hive, RegData, Snapshot, StartType, ValueKey};
use crate::system::{SysError, System};

#[derive(Clone, PartialEq, Debug)]
pub enum Observed {
    Present(Snapshot),
    NotApplicable,
}

#[derive(Clone, PartialEq, Debug)]
pub enum RestoreResult {
    Done,
    ManualReinstall(Option<String>),
}

/// Reads the current state an action would change. Takes `&mut S` because the
/// PowerShell check runs a script (`System::run_powershell` is `&mut self`).
pub fn observe<S: System>(a: &ActionDef, sys: &mut S) -> Result<Observed, SysError> {
    Ok(match a {
        ActionDef::Registry { hive, path, name, .. } => {
            Observed::Present(Snapshot::Reg(sys.reg_get(*hive, path, name)?))
        }
        ActionDef::Service { name, .. } => match sys.service_start(name)? {
            Some(s) => Observed::Present(Snapshot::Service(s)),
            None => Observed::NotApplicable,
        },
        ActionDef::ScheduledTask { path, .. } => match sys.task_enabled(path)? {
            Some(e) => Observed::Present(Snapshot::Task(e)),
            None => Observed::NotApplicable,
        },
        ActionDef::Appx { package, .. } => match sys.appx_installed(package)? {
            Some(i) => Observed::Present(Snapshot::Appx(i)),
            None => Observed::NotApplicable,
        },
        ActionDef::PowerPlan { .. } => {
            Observed::Present(Snapshot::Power(sys.active_power_scheme()?.to_lowercase()))
        }
        ActionDef::PowerShell { check, .. } => {
            Observed::Present(Snapshot::Script(sys.run_powershell(check)?))
        }
    })
}

/// The state that means "this action is applied".
pub fn target(a: &ActionDef) -> Snapshot {
    match a {
        ActionDef::Registry { value, .. } => Snapshot::Reg(value.clone()),
        ActionDef::Service { start, .. } => Snapshot::Service(*start),
        ActionDef::ScheduledTask { enabled, .. } => Snapshot::Task(*enabled),
        ActionDef::Appx { .. } => Snapshot::Appx(false),
        ActionDef::PowerPlan { scheme_guid, .. } => Snapshot::Power(scheme_guid.to_lowercase()),
        ActionDef::PowerShell { .. } => Snapshot::Script("1".into()),
    }
}

pub fn apply<S: System>(a: &ActionDef, sys: &mut S, all_users: bool) -> Result<(), SysError> {
    match a {
        ActionDef::Registry { hive, path, name, value } => {
            sys.reg_set(*hive, path, name, value.as_ref())
        }
        ActionDef::Service { name, start } => sys.set_service_start(name, *start),
        ActionDef::ScheduledTask { path, enabled } => sys.set_task_enabled(path, *enabled),
        ActionDef::Appx { package, all_users: au, .. } => {
            let flag = match au {
                AllUsers::Always => true,
                AllUsers::Optional => all_users,
            };
            sys.remove_appx(package, flag)
        }
        ActionDef::PowerPlan { scheme_guid, duplicate_from } => {
            sys.ensure_power_scheme(scheme_guid, duplicate_from.as_deref())?;
            sys.set_power_scheme(scheme_guid)
        }
        ActionDef::PowerShell { apply, .. } => sys.run_powershell(apply).map(|_| ()),
    }
}

/// Restores from the journal key alone; `catalog_hint` supplies appx reinstall
/// hints and PowerShell undo scripts when a catalog action exists.
pub fn restore_key<S: System>(
    key: &ValueKey,
    sys: &mut S,
    before: &Snapshot,
    catalog_hint: Option<&ActionDef>,
) -> Result<RestoreResult, SysError> {
    match (key, before) {
        (ValueKey::Registry { hive, path, name }, Snapshot::Reg(v)) => {
            sys.reg_set(*hive, path, name, v.as_ref())?;
            Ok(RestoreResult::Done)
        }
        (ValueKey::Service { name }, Snapshot::Service(s)) => {
            sys.set_service_start(name, *s)?;
            Ok(RestoreResult::Done)
        }
        (ValueKey::Task { path }, Snapshot::Task(b)) => {
            sys.set_task_enabled(path, *b)?;
            Ok(RestoreResult::Done)
        }
        (ValueKey::PowerScheme, Snapshot::Power(guid)) => {
            sys.set_power_scheme(guid)?;
            Ok(RestoreResult::Done)
        }
        (ValueKey::Appx { .. }, Snapshot::Appx(_)) => {
            let hint = match catalog_hint {
                Some(ActionDef::Appx { reinstall, .. }) => reinstall.clone(),
                _ => None,
            };
            Ok(RestoreResult::ManualReinstall(hint))
        }
        (ValueKey::Script { .. }, Snapshot::Script(_)) => match catalog_hint {
            Some(ActionDef::PowerShell { undo, .. }) => {
                sys.run_powershell(undo)?;
                Ok(RestoreResult::Done)
            }
            _ => Err(SysError::Other("no undo script for script key".into())),
        },
        _ => Err(SysError::Other(format!(
            "snapshot {before:?} does not match key {key:?}"
        ))),
    }
}

pub fn restore<S: System>(
    a: &ActionDef,
    sys: &mut S,
    before: &Snapshot,
) -> Result<RestoreResult, SysError> {
    restore_key(&a.key("", 0), sys, before, Some(a))
}

fn hive_name(h: Hive) -> &'static str {
    match h {
        Hive::Hkcu => "HKCU",
        Hive::Hklm => "HKLM",
    }
}

pub fn describe(a: &ActionDef) -> String {
    match a {
        ActionDef::Registry { hive, path, name, value } => {
            let loc = format!("{}\\{}\\{}", hive_name(*hive), path, name);
            match value {
                None => format!("{loc} → deleted"),
                Some(RegData::Dword(n)) => format!("{loc} = {n} (DWORD)"),
                Some(RegData::Qword(n)) => format!("{loc} = {n} (QWORD)"),
                Some(RegData::String(s)) => format!("{loc} = {s} (String)"),
                Some(RegData::Binary(b)) => {
                    let hex: Vec<String> = b.iter().map(|x| format!("{x:02x}")).collect();
                    format!("{loc} = {} (Binary)", hex.join(" "))
                }
            }
        }
        ActionDef::Service { name, start } => {
            let s = match start {
                StartType::Disabled => "Disabled",
                StartType::Manual => "Manual",
                StartType::Automatic => "Automatic",
            };
            format!("Service \"{name}\" → {s}")
        }
        ActionDef::ScheduledTask { path, enabled } => format!(
            "Scheduled task \"{path}\" → {}",
            if *enabled { "Enabled" } else { "Disabled" }
        ),
        ActionDef::Appx { package, all_users, .. } => match all_users {
            AllUsers::Always => format!("Remove app \"{package}\" (all users)"),
            AllUsers::Optional => format!("Remove app \"{package}\""),
        },
        ActionDef::PowerPlan { .. } => "Power plan → High Performance".to_string(),
        ActionDef::PowerShell { apply, .. } => format!("PowerShell: {apply}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::system::fake::FakeSystem;

    const P: &str = r"Software\Microsoft\Windows\CurrentVersion\ContentDeliveryManager";

    fn reg(value: Option<RegData>) -> ActionDef {
        ActionDef::Registry {
            hive: Hive::Hkcu,
            path: P.into(),
            name: "SystemPaneSuggestionsEnabled".into(),
            value,
        }
    }

    fn appx(au: AllUsers) -> ActionDef {
        ActionDef::Appx {
            package: "king.com.CandyCrushSaga".into(),
            all_users: au,
            reinstall: Some("9NBLGGH4...".into()),
        }
    }

    #[test]
    fn registry_target_matches_after_apply() {
        let a = reg(Some(RegData::Dword(0)));
        let mut sys = FakeSystem::new();
        apply(&a, &mut sys, false).unwrap();
        assert_eq!(observe(&a, &mut sys).unwrap(), Observed::Present(target(&a)));
    }

    #[test]
    fn registry_restore_absent_deletes_value() {
        let a = reg(Some(RegData::Dword(0)));
        let mut sys = FakeSystem::new();
        apply(&a, &mut sys, false).unwrap();
        let r = restore(&a, &mut sys, &Snapshot::Reg(None)).unwrap();
        assert_eq!(r, RestoreResult::Done);
        assert_eq!(
            sys.reg_get(Hive::Hkcu, P, "SystemPaneSuggestionsEnabled").unwrap(),
            None
        );
    }

    #[test]
    fn missing_service_is_not_applicable() {
        let a = ActionDef::Service { name: "DiagTrack".into(), start: StartType::Disabled };
        assert_eq!(observe(&a, &mut FakeSystem::new()).unwrap(), Observed::NotApplicable);
    }

    #[test]
    fn appx_restore_returns_manual_reinstall_hint() {
        let a = appx(AllUsers::Always);
        let mut sys = FakeSystem::new().with_appx("king.com.CandyCrushSaga", true);
        let r = restore(&a, &mut sys, &Snapshot::Appx(true)).unwrap();
        assert_eq!(r, RestoreResult::ManualReinstall(Some("9NBLGGH4...".into())));
    }

    #[test]
    fn appx_optional_respects_all_users_flag() {
        let a = appx(AllUsers::Optional);
        let mut sys = FakeSystem::new().with_appx("king.com.CandyCrushSaga", true);
        apply(&a, &mut sys, false).unwrap();
        let mut sys2 = FakeSystem::new().with_appx("king.com.CandyCrushSaga", true);
        apply(&a, &mut sys2, true).unwrap();
        assert_eq!(sys.removed_appx, vec![("king.com.CandyCrushSaga".to_string(), false)]);
        assert_eq!(sys2.removed_appx, vec![("king.com.CandyCrushSaga".to_string(), true)]);
        let always = appx(AllUsers::Always);
        let mut sys3 = FakeSystem::new().with_appx("king.com.CandyCrushSaga", true);
        apply(&always, &mut sys3, false).unwrap();
        assert!(sys3.removed_appx[0].1);
    }

    #[test]
    fn describe_registry_dword() {
        assert_eq!(
            describe(&reg(Some(RegData::Dword(0)))),
            r"HKCU\Software\Microsoft\Windows\CurrentVersion\ContentDeliveryManager\SystemPaneSuggestionsEnabled = 0 (DWORD)"
        );
        assert_eq!(
            describe(&reg(None)),
            format!(r"HKCU\{P}\SystemPaneSuggestionsEnabled → deleted")
        );
    }

    #[test]
    fn describe_others() {
        assert_eq!(
            describe(&ActionDef::Service { name: "DiagTrack".into(), start: StartType::Disabled }),
            "Service \"DiagTrack\" → Disabled"
        );
        assert_eq!(
            describe(&appx(AllUsers::Always)),
            "Remove app \"king.com.CandyCrushSaga\" (all users)"
        );
    }

    #[test]
    fn power_apply_duplicates_and_observe_returns_active() {
        let a = ActionDef::PowerPlan {
            scheme_guid: "AAAAAAAA-0000-0000-0000-000000000000".into(),
            duplicate_from: Some(crate::system::fake::DEFAULT_SCHEME.into()),
        };
        let mut sys = FakeSystem::new();
        apply(&a, &mut sys, false).unwrap();
        assert_eq!(observe(&a, &mut sys).unwrap(), Observed::Present(target(&a)));
    }

    #[test]
    fn powershell_observe_and_restore_runs_undo() {
        let a = ActionDef::PowerShell {
            check: "chk".into(),
            apply: "app".into(),
            undo: "und".into(),
        };
        let mut sys = FakeSystem::new().with_ps("chk", "1").with_ps("und", "");
        assert_eq!(
            observe(&a, &mut sys).unwrap(),
            Observed::Present(Snapshot::Script("1".into()))
        );
        assert_eq!(
            restore(&a, &mut sys, &Snapshot::Script("0".into())).unwrap(),
            RestoreResult::Done
        );
        assert_eq!(sys.ps_calls.last().unwrap(), "und");
    }

    #[test]
    fn restore_key_registry_without_hint() {
        let key = ValueKey::Registry { hive: Hive::Hkcu, path: P.to_lowercase(), name: "x".into() };
        let mut sys = FakeSystem::new();
        let r = restore_key(&key, &mut sys, &Snapshot::Reg(Some(RegData::Dword(7))), None).unwrap();
        assert_eq!(r, RestoreResult::Done);
        assert_eq!(sys.reg_get(Hive::Hkcu, P, "X").unwrap(), Some(RegData::Dword(7)));
    }

    #[test]
    fn restore_key_script_without_hint_errors() {
        let key = ValueKey::Script { tweak_id: "t".into(), index: 0 };
        let mut sys = FakeSystem::new();
        let e = restore_key(&key, &mut sys, &Snapshot::Script("0".into()), None).unwrap_err();
        assert!(e.to_string().contains("no undo script"));
    }

    #[test]
    fn restore_key_mismatched_snapshot_errors() {
        let key = ValueKey::Service { name: "x".into() };
        let mut sys = FakeSystem::new();
        assert!(restore_key(&key, &mut sys, &Snapshot::Task(true), None).is_err());
    }
}
