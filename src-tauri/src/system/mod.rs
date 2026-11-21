//! Abstraction over the Windows facilities Featherweight touches, so the
//! engine can be tested against an in-memory fake.

use crate::model::{Hive, RegData, StartType};

#[cfg(test)]
pub mod fake;
#[cfg(windows)]
pub mod windows;

#[derive(thiserror::Error, Debug)]
pub enum SysError {
    #[error("access denied: {0}")]
    AccessDenied(String),
    #[error("not found: {0}")]
    NotFound(String),
    #[error("{0}")]
    Other(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn appx_cache_fetches_once_until_cleared() {
        let cache = AppxCache::default();
        let fetches = std::cell::Cell::new(0);
        let fetch = || {
            fetches.set(fetches.get() + 1);
            Ok(HashSet::from(["Contoso.App".to_string()]))
        };
        assert!(cache.installed("contoso.app", fetch).unwrap(), "case-insensitive");
        assert!(!cache.installed("Other.App", fetch).unwrap());
        assert!(cache.installed("Contoso.App", fetch).unwrap());
        assert_eq!(fetches.get(), 1);
        cache.clear();
        assert!(cache.installed("Contoso.App", fetch).unwrap());
        assert_eq!(fetches.get(), 2);
        // A failed fetch is not cached.
        cache.clear();
        assert!(cache.installed("x", || Err(SysError::Other("boom".into()))).is_err());
        assert!(cache.installed("Contoso.App", fetch).unwrap());
        assert_eq!(fetches.get(), 3);
    }
}

/// Prefix of the `SysError::Other` message of a `stop_service` failure that
/// happened AFTER the stop request was sent (the service may be stopped).
const STOP_SENT: &str = "[stop-sent] ";

impl SysError {
    pub fn stop_sent(msg: &dyn std::fmt::Display) -> SysError {
        SysError::Other(format!("{STOP_SENT}{msg}"))
    }

    /// True if this error came from a `stop_service` whose stop was sent.
    pub fn is_stop_sent(&self) -> bool {
        matches!(self, SysError::Other(m) if m.starts_with(STOP_SENT))
    }
}

/// Installed Appx package names (lowercased) of the current user, read once
/// and reused until cleared: one `Get-AppxPackage` per command instead of
/// one per package. Cleared at the start of each command and after a removal.
#[derive(Default)]
pub struct AppxCache(std::sync::Mutex<Option<std::collections::HashSet<String>>>);

impl AppxCache {
    /// Whether `package` is installed, fetching the list on first use.
    pub fn installed(
        &self,
        package: &str,
        fetch: impl FnOnce() -> Result<std::collections::HashSet<String>, SysError>,
    ) -> Result<bool, SysError> {
        let mut g = self.0.lock().unwrap_or_else(|e| e.into_inner());
        let names = match g.as_ref() {
            Some(n) => n,
            None => g.insert(fetch()?.into_iter().map(|n| n.to_lowercase()).collect()),
        };
        Ok(names.contains(&package.to_lowercase()))
    }

    pub fn clear(&self) {
        *self.0.lock().unwrap_or_else(|e| e.into_inner()) = None;
    }
}

pub trait System {
    /// Drops per-command caches (the Appx list). Commands call this before
    /// reading state, so each command sees the PC as it is now.
    fn begin_command(&mut self) {}
    /// String SID (`S-1-5-21-…`) of the account this process runs as, whose
    /// HKCU hive and Appx packages per-user actions change. `None` if unknown.
    fn current_user_sid(&self) -> Option<String>;
    fn reg_get(&self, hive: Hive, path: &str, name: &str) -> Result<Option<RegData>, SysError>;
    /// `None` deletes the value; keys are created as needed.
    fn reg_set(
        &mut self,
        hive: Hive,
        path: &str,
        name: &str,
        value: Option<&RegData>,
    ) -> Result<(), SysError>;
    /// All values directly under a key; a missing key yields an empty list and
    /// values of unsupported types are skipped.
    fn reg_values(&self, hive: Hive, path: &str) -> Result<Vec<(String, RegData)>, SysError>;
    /// `None` = service absent.
    fn service_start(&self, name: &str) -> Result<Option<StartType>, SysError>;
    /// `Disabled` also stops the service.
    fn set_service_start(&mut self, name: &str, start: StartType) -> Result<(), SysError>;
    /// Stops the service and waits up to 30 s for STOPPED. Returns whether it
    /// WAS running (already stopped -> `Ok(false)`); absent -> `NotFound`.
    fn stop_service(&mut self, name: &str) -> Result<bool, SysError>;
    fn start_service(&mut self, name: &str) -> Result<(), SysError>;
    /// Number of Win32 services currently active.
    fn running_service_count(&self) -> Result<u32, SysError>;
    /// `None` = task absent.
    fn task_enabled(&self, path: &str) -> Result<Option<bool>, SysError>;
    fn set_task_enabled(&mut self, path: &str, enabled: bool) -> Result<(), SysError>;
    /// `Some(true)` = installed for the current user. Real implementations
    /// return `Some(false)` for packages absent on this PC; `None` (unknown
    /// package) is only produced by fakes.
    fn appx_installed(&self, package: &str) -> Result<Option<bool>, SysError>;
    fn remove_appx(&mut self, package: &str, all_users: bool) -> Result<(), SysError>;
    /// Lowercase GUID.
    fn active_power_scheme(&self) -> Result<String, SysError>;
    fn ensure_power_scheme(
        &mut self,
        guid: &str,
        duplicate_from: Option<&str>,
    ) -> Result<(), SysError>;
    fn set_power_scheme(&mut self, guid: &str) -> Result<(), SysError>;
    /// Trimmed stdout.
    fn run_powershell(&mut self, script: &str) -> Result<String, SysError>;
}
