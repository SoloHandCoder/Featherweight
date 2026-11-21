//! In-memory `System` used by action/journal/engine tests.

use super::{SysError, System};
use crate::model::{Hive, RegData, StartType};
use std::collections::{HashMap, HashSet};

pub const DEFAULT_SCHEME: &str = "381b4222-f694-41f0-9685-ff5bb260df2e";

type RegKey = (Hive, String, String);

pub struct FakeSystem {
    regs: HashMap<RegKey, RegData>,
    services: HashMap<String, StartType>,
    tasks: HashMap<String, bool>,
    /// Package names this "build" knows about (others report `None`).
    pub known_appx: HashSet<String>,
    installed_appx: HashSet<String>,
    schemes: HashSet<String>,
    active_scheme: String,
    /// Makes the next MUTATING call fail, then clears itself.
    pub fail_next: Option<SysError>,
    /// Number of mutating calls that succeed before `fail_next` fires.
    pub fail_skip: usize,
    /// Scripted PowerShell output keyed by exact script text.
    pub ps_responses: HashMap<String, String>,
    /// Every `(package, all_users)` passed to a successful `remove_appx`.
    pub removed_appx: Vec<(String, bool)>,
    /// Every script passed to `run_powershell`.
    pub ps_calls: Vec<String>,
    /// Services (lowercase names) stopped via `set_service_start(.., Disabled)`.
    pub stopped_services: HashSet<String>,
    /// Services (lowercase names) currently running.
    pub running_services: HashSet<String>,
    /// Log of `stop_service` / `start_service` calls: `"stop:name"`, `"start:name"`.
    pub service_calls: Vec<String>,
    /// Makes `stop_service` fail (every time) with this message.
    pub fail_stop: Option<String>,
    /// `stop_service` really stops the service, then reports an error
    /// (models a timeout after the stop was requested).
    pub stop_then_fail: bool,
}

fn rk(hive: Hive, path: &str, name: &str) -> RegKey {
    (hive, path.to_lowercase(), name.to_lowercase())
}

impl Default for FakeSystem {
    fn default() -> Self {
        Self::new()
    }
}

impl FakeSystem {
    pub fn new() -> Self {
        FakeSystem {
            regs: HashMap::new(),
            services: HashMap::new(),
            tasks: HashMap::new(),
            known_appx: HashSet::new(),
            installed_appx: HashSet::new(),
            schemes: HashSet::from([DEFAULT_SCHEME.to_string()]),
            active_scheme: DEFAULT_SCHEME.to_string(),
            fail_next: None,
            fail_skip: 0,
            ps_responses: HashMap::new(),
            removed_appx: Vec::new(),
            ps_calls: Vec::new(),
            stopped_services: HashSet::new(),
            running_services: HashSet::new(),
            service_calls: Vec::new(),
            fail_stop: None,
            stop_then_fail: false,
        }
    }

    pub fn with_reg(mut self, hive: Hive, path: &str, name: &str, value: RegData) -> Self {
        self.regs.insert(rk(hive, path, name), value);
        self
    }

    pub fn with_service(mut self, name: &str, start: StartType) -> Self {
        self.services.insert(name.to_lowercase(), start);
        self
    }

    /// Registers a service (Manual start) that is currently running.
    pub fn with_running_service(mut self, name: &str) -> Self {
        self.services.insert(name.to_lowercase(), StartType::Manual);
        self.running_services.insert(name.to_lowercase());
        self
    }

    pub fn with_task(mut self, path: &str, enabled: bool) -> Self {
        self.tasks.insert(path.to_lowercase(), enabled);
        self
    }

    /// Registers a package as known; `installed` sets whether it is present.
    pub fn with_appx(mut self, package: &str, installed: bool) -> Self {
        self.known_appx.insert(package.to_string());
        if installed {
            self.installed_appx.insert(package.to_string());
        } else {
            self.installed_appx.remove(package);
        }
        self
    }

    pub fn with_scheme(mut self, guid: &str) -> Self {
        self.schemes.insert(guid.to_lowercase());
        self
    }

    pub fn with_active_scheme(mut self, guid: &str) -> Self {
        let g = guid.to_lowercase();
        self.schemes.insert(g.clone());
        self.active_scheme = g;
        self
    }

    pub fn with_ps(mut self, script: &str, output: &str) -> Self {
        self.ps_responses.insert(script.to_string(), output.to_string());
        self
    }

    pub fn has_scheme(&self, guid: &str) -> bool {
        self.schemes.contains(&guid.to_lowercase())
    }

    fn check_fail(&mut self) -> Result<(), SysError> {
        if self.fail_next.is_some() && self.fail_skip > 0 {
            self.fail_skip -= 1;
            return Ok(());
        }
        match self.fail_next.take() {
            Some(e) => Err(e),
            None => Ok(()),
        }
    }
}

impl System for FakeSystem {
    fn reg_get(&self, hive: Hive, path: &str, name: &str) -> Result<Option<RegData>, SysError> {
        Ok(self.regs.get(&rk(hive, path, name)).cloned())
    }

    fn reg_set(
        &mut self,
        hive: Hive,
        path: &str,
        name: &str,
        value: Option<&RegData>,
    ) -> Result<(), SysError> {
        self.check_fail()?;
        let key = rk(hive, path, name);
        match value {
            Some(v) => {
                self.regs.insert(key, v.clone());
            }
            None => {
                self.regs.remove(&key);
            }
        }
        Ok(())
    }

    fn service_start(&self, name: &str) -> Result<Option<StartType>, SysError> {
        Ok(self.services.get(&name.to_lowercase()).copied())
    }

    fn set_service_start(&mut self, name: &str, start: StartType) -> Result<(), SysError> {
        self.check_fail()?;
        let key = name.to_lowercase();
        match self.services.get_mut(&key) {
            None => Err(SysError::NotFound(format!("service {name}"))),
            Some(s) => {
                *s = start;
                if start == StartType::Disabled {
                    self.stopped_services.insert(key);
                }
                Ok(())
            }
        }
    }

    fn stop_service(&mut self, name: &str) -> Result<bool, SysError> {
        self.check_fail()?;
        self.service_calls.push(format!("stop:{name}"));
        let key = name.to_lowercase();
        if !self.services.contains_key(&key) {
            return Err(SysError::NotFound(format!("service {name}")));
        }
        if let Some(m) = &self.fail_stop {
            return Err(SysError::Other(m.clone()));
        }
        let was = self.running_services.remove(&key);
        if self.stop_then_fail {
            return Err(SysError::stop_sent(&"timed out waiting for stop"));
        }
        Ok(was)
    }

    fn start_service(&mut self, name: &str) -> Result<(), SysError> {
        self.check_fail()?;
        self.service_calls.push(format!("start:{name}"));
        let key = name.to_lowercase();
        if !self.services.contains_key(&key) {
            return Err(SysError::NotFound(format!("service {name}")));
        }
        self.running_services.insert(key);
        Ok(())
    }

    fn running_service_count(&self) -> Result<u32, SysError> {
        Ok(self.running_services.len() as u32)
    }

    fn task_enabled(&self, path: &str) -> Result<Option<bool>, SysError> {
        Ok(self.tasks.get(&path.to_lowercase()).copied())
    }

    fn set_task_enabled(&mut self, path: &str, enabled: bool) -> Result<(), SysError> {
        self.check_fail()?;
        match self.tasks.get_mut(&path.to_lowercase()) {
            None => Err(SysError::NotFound(format!("task {path}"))),
            Some(t) => {
                *t = enabled;
                Ok(())
            }
        }
    }

    fn appx_installed(&self, package: &str) -> Result<Option<bool>, SysError> {
        if !self.known_appx.contains(package) {
            return Ok(None);
        }
        Ok(Some(self.installed_appx.contains(package)))
    }

    fn remove_appx(&mut self, package: &str, all_users: bool) -> Result<(), SysError> {
        self.check_fail()?;
        if !self.known_appx.contains(package) {
            return Err(SysError::NotFound(format!("package {package}")));
        }
        self.installed_appx.remove(package);
        self.removed_appx.push((package.to_string(), all_users));
        Ok(())
    }

    fn active_power_scheme(&self) -> Result<String, SysError> {
        Ok(self.active_scheme.clone())
    }

    fn ensure_power_scheme(
        &mut self,
        guid: &str,
        duplicate_from: Option<&str>,
    ) -> Result<(), SysError> {
        self.check_fail()?;
        let guid = guid.to_lowercase();
        if self.schemes.contains(&guid) {
            return Ok(());
        }
        match duplicate_from {
            Some(src) if self.schemes.contains(&src.to_lowercase()) => {
                self.schemes.insert(guid);
                Ok(())
            }
            Some(src) => Err(SysError::NotFound(format!("power scheme {src}"))),
            None => Err(SysError::Other(format!(
                "power scheme {guid} missing and no source to duplicate"
            ))),
        }
    }

    fn set_power_scheme(&mut self, guid: &str) -> Result<(), SysError> {
        self.check_fail()?;
        let guid = guid.to_lowercase();
        if !self.schemes.contains(&guid) {
            return Err(SysError::NotFound(format!("power scheme {guid}")));
        }
        self.active_scheme = guid;
        Ok(())
    }

    fn run_powershell(&mut self, script: &str) -> Result<String, SysError> {
        self.check_fail()?;
        self.ps_calls.push(script.to_string());
        self.ps_responses
            .get(script)
            .cloned()
            .ok_or_else(|| SysError::Other("no scripted response".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reg_roundtrip_and_delete() {
        let mut s = FakeSystem::new();
        let v = RegData::Dword(1);
        assert_eq!(s.reg_get(Hive::Hklm, r"SOFTWARE\X", "A").unwrap(), None);
        s.reg_set(Hive::Hklm, r"SOFTWARE\X", "A", Some(&v)).unwrap();
        // case-insensitive path and name
        assert_eq!(s.reg_get(Hive::Hklm, r"software\x", "a").unwrap(), Some(v));
        // hives are distinct
        assert_eq!(s.reg_get(Hive::Hkcu, r"SOFTWARE\X", "A").unwrap(), None);
        s.reg_set(Hive::Hklm, r"SOFTWARE\X", "A", None).unwrap();
        assert_eq!(s.reg_get(Hive::Hklm, r"SOFTWARE\X", "A").unwrap(), None);
    }

    #[test]
    fn service_absent_is_none() {
        let mut s = FakeSystem::new().with_service("DiagTrack", StartType::Automatic);
        assert_eq!(s.service_start("nope").unwrap(), None);
        assert_eq!(s.service_start("diagtrack").unwrap(), Some(StartType::Automatic));
        assert!(matches!(
            s.set_service_start("nope", StartType::Disabled),
            Err(SysError::NotFound(_))
        ));
        s.set_service_start("DiagTrack", StartType::Disabled).unwrap();
        assert_eq!(s.service_start("DiagTrack").unwrap(), Some(StartType::Disabled));
    }

    #[test]
    fn fail_next_fails_once() {
        let mut s = FakeSystem::new().with_service("S", StartType::Manual);
        s.fail_next = Some(SysError::AccessDenied("x".into()));
        // reads do not consume it
        assert_eq!(s.service_start("S").unwrap(), Some(StartType::Manual));
        assert!(matches!(
            s.set_service_start("S", StartType::Disabled),
            Err(SysError::AccessDenied(_))
        ));
        assert_eq!(s.service_start("S").unwrap(), Some(StartType::Manual));
        s.set_service_start("S", StartType::Disabled).unwrap();
        assert_eq!(s.service_start("S").unwrap(), Some(StartType::Disabled));
    }

    #[test]
    fn tasks_and_appx() {
        let mut s = FakeSystem::new()
            .with_task(r"\Microsoft\Windows\A\B", true)
            .with_appx("Contoso.App", true);
        assert_eq!(s.task_enabled(r"\microsoft\windows\a\b").unwrap(), Some(true));
        assert_eq!(s.task_enabled("missing").unwrap(), None);
        s.set_task_enabled(r"\Microsoft\Windows\A\B", false).unwrap();
        assert_eq!(s.task_enabled(r"\Microsoft\Windows\A\B").unwrap(), Some(false));
        assert!(matches!(s.set_task_enabled("missing", true), Err(SysError::NotFound(_))));

        assert_eq!(s.appx_installed("Unknown").unwrap(), None);
        assert_eq!(s.appx_installed("Contoso.App").unwrap(), Some(true));
        s.remove_appx("Contoso.App", true).unwrap();
        assert_eq!(s.appx_installed("Contoso.App").unwrap(), Some(false));
        assert_eq!(s.removed_appx, vec![("Contoso.App".to_string(), true)]);
    }

    #[test]
    fn power_schemes() {
        let mut s = FakeSystem::new();
        assert_eq!(s.active_power_scheme().unwrap(), DEFAULT_SCHEME);
        s.ensure_power_scheme("AAAA", Some(DEFAULT_SCHEME)).unwrap();
        s.set_power_scheme("aaaa").unwrap();
        assert_eq!(s.active_power_scheme().unwrap(), "aaaa");
        assert!(matches!(s.set_power_scheme("zzzz"), Err(SysError::NotFound(_))));
        assert!(matches!(
            s.ensure_power_scheme("bbbb", Some("zzzz")),
            Err(SysError::NotFound(_))
        ));
    }

    #[test]
    fn powershell_scripted() {
        let mut s = FakeSystem::new().with_ps("Get-X", "out");
        assert_eq!(s.run_powershell("Get-X").unwrap(), "out");
        assert!(
            matches!(s.run_powershell("other"), Err(SysError::Other(m)) if m == "no scripted response")
        );
        assert_eq!(s.ps_calls, vec!["Get-X", "other"]);
    }
}
