//! One system restore point per session, before the first change.

#![allow(dead_code)] // wired up by a later task
use crate::system::{SysError, System};
use serde::Serialize;

pub const CHECKPOINT_SCRIPT: &str = r#"Checkpoint-Computer -Description "Featherweight" -RestorePointType MODIFY_SETTINGS -WarningAction Stop"#;
pub const ENABLE_SCRIPT: &str = r#"Enable-ComputerRestore -Drive "$env:SystemDrive\""#;

#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum RestorePointOutcome {
    Created,
    AlreadyMadeThisSession,
    Throttled,
    Disabled,
    Failed(String),
}

#[derive(Debug, Default)]
pub struct RestoreGuard {
    pub made_this_session: bool,
}

fn classify(text: &str) -> Option<RestorePointOutcome> {
    let t = text.to_lowercase();
    if t.contains("1440 minutes") {
        Some(RestorePointOutcome::Throttled)
    } else if t.contains("service cannot be started")
        || t.contains("system restore is disabled")
        || t.contains("turned off")
    {
        Some(RestorePointOutcome::Disabled)
    } else {
        None
    }
}

pub fn ensure<S: System>(g: &mut RestoreGuard, sys: &mut S) -> RestorePointOutcome {
    if g.made_this_session {
        return RestorePointOutcome::AlreadyMadeThisSession;
    }
    let outcome = match sys.run_powershell(CHECKPOINT_SCRIPT) {
        Ok(out) => classify(&out).unwrap_or(RestorePointOutcome::Created),
        Err(SysError::Other(msg)) => classify(&msg).unwrap_or(RestorePointOutcome::Failed(msg)),
        Err(e) => RestorePointOutcome::Failed(format!("{e:?}")),
    };
    // Throttled means a restore point from the last 24h exists: stop asking.
    if matches!(outcome, RestorePointOutcome::Created | RestorePointOutcome::Throttled) {
        g.made_this_session = true;
    }
    outcome
}

pub fn enable<S: System>(sys: &mut S) -> Result<(), SysError> {
    sys.run_powershell(ENABLE_SCRIPT).map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::system::fake::FakeSystem;

    #[test]
    fn created_once_per_session() {
        let mut sys = FakeSystem::new().with_ps(CHECKPOINT_SCRIPT, "");
        let mut g = RestoreGuard::default();
        assert_eq!(ensure(&mut g, &mut sys), RestorePointOutcome::Created);
        assert_eq!(ensure(&mut g, &mut sys), RestorePointOutcome::AlreadyMadeThisSession);
        assert_eq!(sys.ps_calls.len(), 1);
    }

    #[test]
    fn throttled_detected() {
        let mut sys = FakeSystem::new();
        sys.ps_errors.insert(
            CHECKPOINT_SCRIPT.into(),
            "A new system restore point cannot be created because one was already created within the past 1440 minutes.".into(),
        );
        let mut g = RestoreGuard::default();
        assert_eq!(ensure(&mut g, &mut sys), RestorePointOutcome::Throttled);
        assert!(g.made_this_session);
        // also via Ok output
        let mut sys = FakeSystem::new().with_ps(CHECKPOINT_SCRIPT, "WARNING: ... 1440 MINUTES ...");
        let mut g = RestoreGuard::default();
        assert_eq!(ensure(&mut g, &mut sys), RestorePointOutcome::Throttled);
    }

    #[test]
    fn disabled_detected() {
        let mut sys = FakeSystem::new();
        sys.ps_errors.insert(
            CHECKPOINT_SCRIPT.into(),
            "The service cannot be started, either because it is disabled".into(),
        );
        let mut g = RestoreGuard::default();
        assert_eq!(ensure(&mut g, &mut sys), RestorePointOutcome::Disabled);
        assert!(!g.made_this_session);
        let mut sys = FakeSystem::new().with_ps(CHECKPOINT_SCRIPT, "System Restore is disabled on this drive");
        assert_eq!(ensure(&mut RestoreGuard::default(), &mut sys), RestorePointOutcome::Disabled);
    }

    #[test]
    fn failed_does_not_mark_session() {
        let mut sys = FakeSystem::new();
        sys.ps_errors.insert(CHECKPOINT_SCRIPT.into(), "boom".into());
        let mut g = RestoreGuard::default();
        assert_eq!(ensure(&mut g, &mut sys), RestorePointOutcome::Failed("boom".into()));
        assert!(!g.made_this_session);
        ensure(&mut g, &mut sys);
        assert_eq!(sys.ps_calls.len(), 2);
    }

    #[test]
    fn enable_runs_script() {
        let mut sys = FakeSystem::new().with_ps(ENABLE_SCRIPT, "");
        enable(&mut sys).unwrap();
        assert_eq!(sys.ps_calls, vec![ENABLE_SCRIPT]);
    }
}
