//! Numbers shown on the Home screen.
#![allow(dead_code)]

use crate::system::{SysError, System};
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Stats {
    pub startup_enabled: u32,
    pub services_running: u32,
    pub junk_bytes: u64,
}

impl Stats {
    pub fn collect<S: System>(
        sys: &S,
        startup_enabled: u32,
        junk_bytes: u64,
    ) -> Result<Stats, SysError> {
        Ok(Stats { startup_enabled, services_running: sys.running_service_count()?, junk_bytes })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::system::fake::FakeSystem;

    #[test]
    fn collect_combines_inputs_with_running_services() {
        let sys = FakeSystem::new().with_running_service("a").with_running_service("b");
        let s = Stats::collect(&sys, 7, 1234).unwrap();
        assert_eq!(s, Stats { startup_enabled: 7, services_running: 2, junk_bytes: 1234 });
    }

    #[test]
    fn serializes_camel_case() {
        let s = Stats { startup_enabled: 1, services_running: 2, junk_bytes: 3 };
        let j = serde_json::to_string(&s).unwrap();
        assert_eq!(j, r#"{"startupEnabled":1,"servicesRunning":2,"junkBytes":3}"#);
    }
}
