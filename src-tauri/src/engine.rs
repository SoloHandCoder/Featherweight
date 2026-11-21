//! Apply / undo engine: status checks, journaled apply with per-tweak
//! rollback, and change-aware undo.

use std::collections::HashSet;

use crate::actions::{self, Observed, RestoreResult};
use crate::catalog::{ActionDef, Catalog, TweakDef};
use crate::journal::{Journal, JournalEntry, now_rfc3339};
use crate::model::{Hive, RegData, Snapshot, ValueKey};
use crate::system::{SysError, System};

#[derive(Clone, Debug, PartialEq)]
pub struct Env {
    pub build: u32,
    pub is_laptop: bool,
    pub per_user_allowed: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NaReason {
    WrongBuild,
    AccountMismatch,
    MissingOnThisPc,
}

#[derive(Clone, Debug, PartialEq)]
pub enum TweakStatus {
    Applied,
    NotApplied,
    Partial,
    NotApplicable(NaReason),
}

#[derive(Clone, Debug, PartialEq)]
pub enum ApplyOutcome {
    Applied,
    AlreadyApplied,
    Failed(String),
    NotApplicable(NaReason),
}

#[derive(Clone, Debug, PartialEq)]
pub enum UndoOutcome {
    Restored,
    AlreadyOriginal,
    ChangedSince(String),
    /// A removed app: `app` is its package name, `hint` its Store product id.
    ManualReinstall { app: String, hint: Option<String> },
    /// Journaled for another Windows account (its HKCU or apps): left alone,
    /// it can only be undone by running Featherweight as that account.
    OtherAccount(String),
    Failed(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Progress {
    pub done: usize,
    pub total: usize,
    pub current: String,
}

pub struct Engine<S: System> {
    pub sys: S,
    pub catalog: Catalog,
    pub journal: Journal,
    pub env: Env,
}

/// Reads the current state of a journal key. Registry, service, task, appx and
/// power keys are read directly; script keys need the catalog action's check.
/// `Ok(None)` means the state cannot be observed (absent, or no hint).
pub fn observe_key<S: System>(
    key: &ValueKey,
    sys: &mut S,
    hint: Option<&ActionDef>,
) -> Result<Option<Snapshot>, SysError> {
    Ok(match key {
        ValueKey::Registry { hive, path, name } => {
            Some(Snapshot::Reg(sys.reg_get(*hive, path, name)?))
        }
        ValueKey::Service { name } => sys.service_start(name)?.map(Snapshot::Service),
        ValueKey::Task { path } => sys.task_enabled(path)?.map(Snapshot::Task),
        ValueKey::Appx { package } => sys.appx_installed(package)?.map(Snapshot::Appx),
        ValueKey::PowerScheme => Some(Snapshot::Power(sys.active_power_scheme()?.to_lowercase())),
        ValueKey::Script { .. } => match hint {
            Some(a @ ActionDef::PowerShell { .. }) => match actions::observe(a, sys)? {
                Observed::Present(snap) => Some(snap),
                Observed::NotApplicable => None,
            },
            _ => None,
        },
    })
}

/// Human-readable name of a journal key, for `ChangedSince` and errors.
fn describe_key(key: &ValueKey, hint: Option<&ActionDef>) -> String {
    if let Some(a) = hint {
        return actions::describe(a);
    }
    match key {
        ValueKey::Registry { hive, path, name } => {
            let h = match hive {
                Hive::Hklm => "HKLM",
                Hive::Hkcu => "HKCU",
            };
            format!("{h}\\{path}\\{name}")
        }
        ValueKey::Service { name } => format!("Service \"{name}\""),
        ValueKey::Task { path } => format!("Scheduled task \"{path}\""),
        ValueKey::Appx { package } => format!("App \"{package}\""),
        ValueKey::PowerScheme => "Power plan".to_string(),
        ValueKey::Script { tweak_id, index } => format!("Script {tweak_id}#{index}"),
    }
}

/// One action touched during the current `apply` call, kept for rollback.
struct Done {
    key: ValueKey,
    index: usize,
    observed_before: Snapshot,
    /// Account the journal entry was tagged with.
    sid: Option<String>,
    /// The journal entry for this key before this call (if any), so rollback
    /// puts the journal back exactly as it was.
    prev_entry: Option<JournalEntry>,
}

impl<S: System> Engine<S> {
    pub fn new(sys: S, catalog: Catalog, journal: Journal, env: Env) -> Self {
        Engine { sys, catalog, journal, env }
    }

    /// The account a journal key belongs to: the current user's SID for
    /// HKCU values, Appx packages and per-user scripts, `None` otherwise.
    fn key_sid(&self, key: &ValueKey, per_user_tweak: bool) -> Option<String> {
        let per_user = match key {
            ValueKey::Registry { hive: Hive::Hkcu, .. } | ValueKey::Appx { .. } => true,
            ValueKey::Script { .. } => per_user_tweak,
            _ => false,
        };
        if per_user { self.sys.current_user_sid() } else { None }
    }

    /// The entry was recorded for a different account than the current one.
    fn belongs_to_other_account(&self, e: &JournalEntry) -> bool {
        e.user_sid
            .as_deref()
            .is_some_and(|sid| self.sys.current_user_sid().as_deref() != Some(sid))
    }

    fn applicability(&self, t: &TweakDef) -> Option<NaReason> {
        let b = self.env.build;
        if t.min_build.is_some_and(|m| b < m) || t.max_build.is_some_and(|m| b > m) {
            return Some(NaReason::WrongBuild);
        }
        if t.is_per_user() && !self.env.per_user_allowed {
            return Some(NaReason::AccountMismatch);
        }
        None
    }

    /// Observes every action. `None` entries are actions that do not exist on
    /// this PC (e.g. a service absent on some editions).
    fn observe_all(&mut self, t: &TweakDef) -> Result<Vec<Option<Snapshot>>, SysError> {
        let mut out = Vec::with_capacity(t.actions.len());
        for a in &t.actions {
            out.push(match actions::observe(a, &mut self.sys)? {
                Observed::Present(s) => Some(s),
                Observed::NotApplicable => None,
            });
        }
        Ok(out)
    }

    pub fn status(&mut self, id: &str) -> TweakStatus {
        let Some(t) = self.catalog.get(id).cloned() else {
            return TweakStatus::NotApplicable(NaReason::MissingOnThisPc);
        };
        if let Some(r) = self.applicability(&t) {
            return TweakStatus::NotApplicable(r);
        }
        let observed = match self.observe_all(&t) {
            Ok(o) => o,
            // An unreadable value is treated as "not at target".
            Err(_) => return TweakStatus::NotApplied,
        };
        // Only actions present on this PC count.
        let present: Vec<_> =
            t.actions.iter().zip(&observed).filter_map(|(a, o)| o.as_ref().map(|o| (a, o))).collect();
        if present.is_empty() {
            return TweakStatus::NotApplicable(NaReason::MissingOnThisPc);
        }
        let at = present.iter().filter(|(a, o)| actions::target(a) == **o).count();
        if at == present.len() {
            TweakStatus::Applied
        } else if at == 0 {
            TweakStatus::NotApplied
        } else {
            TweakStatus::Partial
        }
    }

    pub fn apply(
        &mut self,
        ids: &[String],
        all_users: &HashSet<String>,
        on_progress: &mut dyn FnMut(Progress),
    ) -> Vec<(String, ApplyOutcome)> {
        let mut results = Vec::with_capacity(ids.len());
        for (i, id) in ids.iter().enumerate() {
            let tweak = self.catalog.get(id).cloned();
            on_progress(Progress {
                done: i,
                total: ids.len(),
                current: tweak.as_ref().map_or_else(|| id.clone(), |t| t.title.clone()),
            });
            let outcome = match tweak {
                None => ApplyOutcome::Failed(format!("unknown tweak id: {id}")),
                Some(t) => self.apply_one(&t, all_users.contains(id)),
            };
            results.push((id.clone(), outcome));
        }
        results
    }

    fn apply_one(&mut self, t: &TweakDef, all_users: bool) -> ApplyOutcome {
        if let Some(r) = self.applicability(t) {
            return ApplyOutcome::NotApplicable(r);
        }
        let observed = match self.observe_all(t) {
            Ok(o) => o,
            Err(e) => return ApplyOutcome::Failed(e.to_string()),
        };
        if observed.iter().all(Option::is_none) {
            return ApplyOutcome::NotApplicable(NaReason::MissingOnThisPc);
        }
        if t.actions
            .iter()
            .zip(&observed)
            .all(|(a, o)| o.as_ref().is_none_or(|o| actions::target(a) == *o))
        {
            return ApplyOutcome::AlreadyApplied;
        }

        let mut done: Vec<Done> = Vec::new();
        for (index, (a, before)) in t.actions.iter().zip(observed).enumerate() {
            // Absent on this PC: nothing to apply, nothing to journal.
            let Some(before) = before else { continue };
            let after = actions::target(a);
            if before == after {
                // Already at target: leave its (possibly original) journal entry alone.
                continue;
            }
            let key = a.key(&t.id, index);
            let sid = self.key_sid(&key, t.is_per_user());
            let prev_entry = self.journal.find(&key, sid.as_deref());
            let entry = JournalEntry {
                key: key.clone(),
                tweak_id: t.id.clone(),
                action_index: index,
                before: before.clone(),
                after,
                applied_at: now_rfc3339(),
                user_sid: sid.clone(),
            };
            if let Err(e) = self.journal.record(entry) {
                let msg = format!("could not write the undo journal: {e}");
                return ApplyOutcome::Failed(self.rollback(t, done, msg));
            }
            // The journal now holds this key, so it joins the rollback set even if
            // apply fails part-way (restoring an unchanged value is harmless).
            done.push(Done { key, index, observed_before: before, sid, prev_entry });
            if let Err(e) = actions::apply(a, &mut self.sys, all_users) {
                return ApplyOutcome::Failed(self.rollback(t, done, e.to_string()));
            }
        }
        ApplyOutcome::Applied
    }

    /// Restores `done` in reverse order and puts their journal entries back as
    /// they were. Anything that cannot be restored keeps its entry so the user
    /// can undo it later. Returns the failure message.
    fn rollback(&mut self, t: &TweakDef, done: Vec<Done>, msg: String) -> String {
        let mut problems = Vec::new();
        for d in done.into_iter().rev() {
            let hint = t.actions.get(d.index);
            match actions::restore_key(&d.key, &mut self.sys, &d.observed_before, hint) {
                Ok(RestoreResult::Done) => {
                    let r = match d.prev_entry {
                        Some(prev) => self.journal.record(prev),
                        None => self.journal.remove(&d.key, d.sid.as_deref()),
                    };
                    if let Err(e) = r {
                        problems.push(format!("journal update failed: {e}"));
                    }
                }
                Ok(RestoreResult::ManualReinstall(_)) => {
                    problems.push(format!("{} needs a manual reinstall", describe_key(&d.key, hint)));
                }
                Err(e) => {
                    problems.push(format!("could not roll back {}: {e}", describe_key(&d.key, hint)));
                }
            }
        }
        if problems.is_empty() {
            msg
        } else {
            format!("{msg} (rollback incomplete: {})", problems.join("; "))
        }
    }

    pub fn undo(&mut self, ids: &[String], force: bool) -> Vec<(String, Vec<UndoOutcome>)> {
        ids.iter()
            .map(|id| {
                let mut entries = self.journal.for_tweak(id);
                entries.sort_by_key(|e| std::cmp::Reverse(e.action_index));
                let outs = entries.iter().map(|e| self.undo_entry(e, force)).collect();
                (id.clone(), outs)
            })
            .collect()
    }

    pub fn undo_all(&mut self, force: bool) -> Vec<(String, Vec<UndoOutcome>)> {
        // Newest first, grouped per tweak in order of each tweak's newest entry.
        let mut groups: Vec<(String, Vec<JournalEntry>)> = Vec::new();
        for e in self.journal.all() {
            match groups.iter_mut().find(|(id, _)| *id == e.tweak_id) {
                Some((_, v)) => v.push(e),
                None => groups.push((e.tweak_id.clone(), vec![e])),
            }
        }
        groups
            .into_iter()
            .map(|(id, mut entries)| {
                // Same timestamp ordering can tie; within a tweak go in reverse action order.
                entries.sort_by(|a, b| {
                    b.applied_at.cmp(&a.applied_at).then(b.action_index.cmp(&a.action_index))
                });
                let outs = entries.iter().map(|e| self.undo_entry(e, force)).collect();
                (id, outs)
            })
            .collect()
    }

    /// The catalog action the entry was journaled for, if the catalog still
    /// has it at that index (a changed catalog may have moved actions).
    fn hint_for(&self, e: &JournalEntry) -> Option<ActionDef> {
        let t = self.catalog.get(&e.tweak_id)?;
        t.actions
            .get(e.action_index)
            .filter(|a| a.key(&t.id, e.action_index) == e.key)
            .cloned()
    }

    fn undo_entry(&mut self, e: &JournalEntry, force: bool) -> UndoOutcome {
        let hint = self.hint_for(e);
        if self.belongs_to_other_account(e) {
            // Its HKCU / apps are not the ones this process sees: never touch.
            return UndoOutcome::OtherAccount(describe_key(&e.key, hint.as_ref()));
        }
        let sid = e.user_sid.as_deref();
        let current = match observe_key(&e.key, &mut self.sys, hint.as_ref()) {
            Ok(c) => c,
            Err(err) => return UndoOutcome::Failed(err.to_string()),
        };
        let at_after = current.as_ref() == Some(&e.after);
        if !at_after && current.as_ref() == Some(&e.before) {
            return match self.journal.remove(&e.key, sid) {
                Ok(()) => UndoOutcome::AlreadyOriginal,
                Err(err) => {
                    UndoOutcome::Failed(format!("could not update the undo journal: {err}"))
                }
            };
        }
        if !at_after && !force {
            return UndoOutcome::ChangedSince(describe_key(&e.key, hint.as_ref()));
        }
        let hint_ref = hint.as_ref();
        let outcome = match actions::restore_key(&e.key, &mut self.sys, &e.before, hint_ref) {
            Ok(RestoreResult::Done) => UndoOutcome::Restored,
            Ok(RestoreResult::ManualReinstall(hint)) => UndoOutcome::ManualReinstall {
                app: match &e.key {
                    ValueKey::Appx { package } => package.clone(),
                    other => describe_key(other, hint_ref),
                },
                hint,
            },
            Err(err) => return UndoOutcome::Failed(err.to_string()),
        };
        match self.journal.remove(&e.key, sid) {
            Ok(()) => outcome,
            Err(err) => UndoOutcome::Failed(format!(
                "restored, but could not update the undo journal: {err}"
            )),
        }
    }

    /// Journaled registry write for callers outside the catalog (startup apps).
    pub fn set_registry_journaled(
        &mut self,
        tweak_id: &str,
        hive: Hive,
        path: &str,
        name: &str,
        value: Option<RegData>,
    ) -> Result<(), SysError> {
        let key = ValueKey::Registry {
            hive,
            path: path.to_lowercase(),
            name: name.to_lowercase(),
        };
        let current = Snapshot::Reg(self.sys.reg_get(hive, path, name)?);
        let after = Snapshot::Reg(value.clone());
        if current == after {
            return Ok(());
        }
        let sid = self.key_sid(&key, false);
        let prev_entry = self.journal.find(&key, sid.as_deref());
        self.journal
            .record(JournalEntry {
                key: key.clone(),
                tweak_id: tweak_id.to_string(),
                action_index: 0,
                before: current,
                after,
                applied_at: now_rfc3339(),
                user_sid: sid.clone(),
            })
            .map_err(|e| SysError::Other(format!("could not write the undo journal: {e}")))?;
        if let Err(err) = self.sys.reg_set(hive, path, name, value.as_ref()) {
            // Nothing changed on the system; put the journal back as it was.
            let _ = match prev_entry {
                Some(prev) => self.journal.record(prev),
                None => self.journal.remove(&key, sid.as_deref()),
            };
            return Err(err);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::StartType;
    use crate::system::fake::FakeSystem;

    const SVC_A: &str = "SvcA";

    /// System-wide tweak: HKLM value + service.
    const TWEAK_A: &str = r#"
id = "a"
title = "Tweak A"
description = "D"
section = "background"
level = "safe"

[[actions]]
type = "registry"
hive = "hklm"
path = 'Software\A'
name = "V"
value = { dword = 1 }

[[actions]]
type = "service"
name = "SvcA"
start = "disabled"
"#;

    const TWEAK_B: &str = r#"
id = "b"
title = "Tweak B"
description = "D"
section = "background"
level = "safe"

[[actions]]
type = "registry"
hive = "hklm"
path = 'Software\B'
name = "V"
value = { dword = 1 }
"#;

    /// Two services only (no registry action), so it can be fully missing.
    const TWEAK_SVCS: &str = r#"
id = "svcs"
title = "Services"
description = "D"
section = "background"
level = "safe"

[[actions]]
type = "service"
name = "SvcX"
start = "disabled"

[[actions]]
type = "service"
name = "SvcY"
start = "disabled"
"#;

    const TWEAK_NEW_BUILD: &str = r#"
id = "newbuild"
title = "New build only"
description = "D"
section = "background"
level = "safe"
min_build = 22000

[[actions]]
type = "registry"
hive = "hklm"
path = 'Software\N'
name = "V"
value = { dword = 1 }
"#;

    const TWEAK_USER: &str = r#"
id = "user"
title = "Per user"
description = "D"
section = "background"
level = "safe"

[[actions]]
type = "registry"
hive = "hkcu"
path = 'Software\U'
name = "V"
value = { dword = 1 }
"#;

    const TWEAK_APPX: &str = r#"
id = "candy"
title = "Remove Candy"
description = "D"
section = "remove"
level = "safe"

[[actions]]
type = "appx"
package = "king.com.CandyCrushSaga"
all_users = "optional"
reinstall = "9NBLGGH18846"
"#;

    fn env() -> Env {
        Env { build: 19045, is_laptop: false, per_user_allowed: true }
    }

    fn base_sys() -> FakeSystem {
        FakeSystem::new()
            .with_reg(Hive::Hklm, r"Software\A", "V", RegData::Dword(0))
            .with_service(SVC_A, StartType::Automatic)
            .with_appx("king.com.CandyCrushSaga", true)
    }

    fn engine_with(sys: FakeSystem, dir: &tempfile::TempDir) -> Engine<FakeSystem> {
        let catalog = Catalog::from_strs(&[
            TWEAK_A,
            TWEAK_B,
            TWEAK_SVCS,
            TWEAK_NEW_BUILD,
            TWEAK_USER,
            TWEAK_APPX,
        ])
        .unwrap();
        let journal = Journal::load(dir.path().join("journal.json")).unwrap();
        Engine::new(sys, catalog, journal, env())
    }

    fn ids(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    fn apply_ids(e: &mut Engine<FakeSystem>, v: &[&str]) -> Vec<(String, ApplyOutcome)> {
        e.apply(&ids(v), &HashSet::new(), &mut |_| {})
    }

    fn reg_a(e: &Engine<FakeSystem>) -> Option<RegData> {
        e.sys.reg_get(Hive::Hklm, r"Software\A", "V").unwrap()
    }

    fn svc_a(e: &Engine<FakeSystem>) -> Option<StartType> {
        e.sys.service_start(SVC_A).unwrap()
    }

    #[test]
    fn status_applied_partial_not_applied() {
        let d = tempfile::tempdir().unwrap();
        let mut e = engine_with(base_sys(), &d);
        assert_eq!(e.status("a"), TweakStatus::NotApplied);
        e.sys.reg_set(Hive::Hklm, r"Software\A", "V", Some(&RegData::Dword(1))).unwrap();
        assert_eq!(e.status("a"), TweakStatus::Partial);
        e.sys.set_service_start(SVC_A, StartType::Disabled).unwrap();
        assert_eq!(e.status("a"), TweakStatus::Applied);
    }

    #[test]
    fn status_not_applicable_on_wrong_build() {
        let d = tempfile::tempdir().unwrap();
        let mut e = engine_with(base_sys(), &d);
        assert_eq!(e.status("newbuild"), TweakStatus::NotApplicable(NaReason::WrongBuild));
        e.env.build = 22631;
        assert_eq!(e.status("newbuild"), TweakStatus::NotApplied);
    }

    #[test]
    fn status_not_applicable_on_account_mismatch_for_per_user() {
        let d = tempfile::tempdir().unwrap();
        let mut e = engine_with(base_sys(), &d);
        e.env.per_user_allowed = false;
        assert_eq!(e.status("user"), TweakStatus::NotApplicable(NaReason::AccountMismatch));
        // system-wide tweaks are unaffected
        assert_eq!(e.status("a"), TweakStatus::NotApplied);
    }

    #[test]
    fn all_actions_missing_is_not_applicable() {
        let d = tempfile::tempdir().unwrap();
        let mut e = engine_with(base_sys(), &d);
        assert_eq!(e.status("svcs"), TweakStatus::NotApplicable(NaReason::MissingOnThisPc));
        let out = apply_ids(&mut e, &["svcs"]);
        assert_eq!(out, vec![("svcs".into(), ApplyOutcome::NotApplicable(NaReason::MissingOnThisPc))]);
        assert!(e.journal.all().is_empty());
    }

    #[test]
    fn status_ignores_missing_actions_when_some_present() {
        let d = tempfile::tempdir().unwrap();
        let sys = base_sys().with_service("SvcX", StartType::Automatic);
        let mut e = engine_with(sys, &d);
        assert_eq!(e.status("svcs"), TweakStatus::NotApplied);
        e.sys.set_service_start("SvcX", StartType::Disabled).unwrap();
        assert_eq!(e.status("svcs"), TweakStatus::Applied);
    }

    #[test]
    fn apply_skips_missing_action_and_applies_rest() {
        let d = tempfile::tempdir().unwrap();
        let sys = base_sys().with_service("SvcX", StartType::Automatic);
        let mut e = engine_with(sys, &d);
        let out = apply_ids(&mut e, &["svcs"]);
        assert_eq!(out, vec![("svcs".into(), ApplyOutcome::Applied)]);
        assert_eq!(e.sys.service_start("SvcX").unwrap(), Some(StartType::Disabled));
        assert_eq!(e.sys.service_start("SvcY").unwrap(), None, "missing service untouched");
        let entries = e.journal.for_tweak("svcs");
        assert_eq!(entries.len(), 1, "only the present action is journaled");
        assert_eq!(entries[0].action_index, 0);
    }

    #[test]
    fn already_applied_when_present_actions_at_target() {
        let d = tempfile::tempdir().unwrap();
        let sys = base_sys().with_service("SvcY", StartType::Disabled);
        let mut e = engine_with(sys, &d);
        let out = apply_ids(&mut e, &["svcs"]);
        assert_eq!(out, vec![("svcs".into(), ApplyOutcome::AlreadyApplied)]);
        assert!(e.journal.all().is_empty());
    }

    #[test]
    fn apply_then_undo_restores_original() {
        let d = tempfile::tempdir().unwrap();
        let mut e = engine_with(base_sys(), &d);
        let mut seen = Vec::new();
        let out = e.apply(&ids(&["a", "b"]), &HashSet::new(), &mut |p| seen.push(p));
        assert_eq!(
            out,
            vec![("a".into(), ApplyOutcome::Applied), ("b".into(), ApplyOutcome::Applied)]
        );
        assert_eq!(
            seen,
            vec![
                Progress { done: 0, total: 2, current: "Tweak A".into() },
                Progress { done: 1, total: 2, current: "Tweak B".into() },
            ]
        );
        assert_eq!(e.status("a"), TweakStatus::Applied);
        assert_eq!(e.journal.for_tweak("a").len(), 2);

        let u = e.undo(&ids(&["a", "b"]), false);
        assert_eq!(
            u,
            vec![
                ("a".into(), vec![UndoOutcome::Restored, UndoOutcome::Restored]),
                ("b".into(), vec![UndoOutcome::Restored]),
            ]
        );
        assert_eq!(reg_a(&e), Some(RegData::Dword(0)));
        assert_eq!(svc_a(&e), Some(StartType::Automatic));
        // value B was absent before, so undo deletes it
        assert_eq!(e.sys.reg_get(Hive::Hklm, r"Software\B", "V").unwrap(), None);
        assert!(e.journal.all().is_empty());
        assert_eq!(e.status("a"), TweakStatus::NotApplied);
    }

    #[test]
    fn undo_restores_in_reverse_action_order() {
        let d = tempfile::tempdir().unwrap();
        let mut e = engine_with(base_sys(), &d);
        apply_ids(&mut e, &["a"]);
        // Make the second restore (the registry one, action 0) fail: if order
        // is reversed, the service (action 1) is restored first and succeeds.
        e.sys.fail_skip = 1;
        e.sys.fail_next = Some(SysError::AccessDenied("reg".into()));
        let u = e.undo(&ids(&["a"]), false);
        assert_eq!(u[0].1[0], UndoOutcome::Restored);
        assert!(matches!(u[0].1[1], UndoOutcome::Failed(_)));
        assert_eq!(svc_a(&e), Some(StartType::Automatic));
        assert_eq!(reg_a(&e), Some(RegData::Dword(1)));
        // failed entry is kept for a later retry
        let left = e.journal.for_tweak("a");
        assert_eq!(left.len(), 1);
        assert_eq!(left[0].action_index, 0);
    }

    #[test]
    fn apply_twice_keeps_original_before() {
        let d = tempfile::tempdir().unwrap();
        let mut e = engine_with(base_sys(), &d);
        apply_ids(&mut e, &["a"]);
        let journal_before = e.journal.all();
        let out = apply_ids(&mut e, &["a"]);
        assert_eq!(out, vec![("a".into(), ApplyOutcome::AlreadyApplied)]);
        assert_eq!(e.journal.all(), journal_before, "AlreadyApplied writes nothing");

        // Windows resets the service; re-applying must skip the registry action
        // (already at target) and keep both original befores.
        e.sys.set_service_start(SVC_A, StartType::Manual).unwrap();
        assert_eq!(e.status("a"), TweakStatus::Partial);
        let reg_entry_before = e.journal.for_tweak("a").into_iter().find(|x| x.action_index == 0);
        let out = apply_ids(&mut e, &["a"]);
        assert_eq!(out, vec![("a".into(), ApplyOutcome::Applied)]);
        let reg_entry_after = e.journal.for_tweak("a").into_iter().find(|x| x.action_index == 0);
        assert_eq!(reg_entry_before, reg_entry_after, "skipped action not re-journaled");

        e.undo(&ids(&["a"]), false);
        assert_eq!(reg_a(&e), Some(RegData::Dword(0)));
        assert_eq!(svc_a(&e), Some(StartType::Automatic), "original before kept");
    }

    #[test]
    fn apply_failure_rolls_back_earlier_actions_of_same_tweak() {
        let d = tempfile::tempdir().unwrap();
        let mut e = engine_with(base_sys(), &d);
        // action 1 (registry) succeeds, action 2 (service) fails
        e.sys.fail_skip = 1;
        e.sys.fail_next = Some(SysError::AccessDenied("service".into()));
        let out = apply_ids(&mut e, &["a", "b"]);
        assert!(matches!(&out[0], (id, ApplyOutcome::Failed(m)) if id == "a" && m.contains("service")));
        assert_eq!(out[1], ("b".into(), ApplyOutcome::Applied));
        assert_eq!(reg_a(&e), Some(RegData::Dword(0)), "action 1 rolled back");
        assert_eq!(svc_a(&e), Some(StartType::Automatic));
        assert!(e.journal.for_tweak("a").is_empty(), "rolled-back entries removed");
        assert_eq!(e.journal.for_tweak("b").len(), 1);
        assert_eq!(e.status("b"), TweakStatus::Applied);
    }

    #[test]
    fn apply_journal_write_failure_applies_nothing() {
        let d = tempfile::tempdir().unwrap();
        // the journal's parent "directory" is a file, so persisting fails
        let blocker = d.path().join("blocker");
        std::fs::write(&blocker, b"x").unwrap();
        let catalog = Catalog::from_strs(&[TWEAK_A]).unwrap();
        let journal = Journal::load(blocker.join("journal.json")).unwrap();
        let mut e = Engine::new(base_sys(), catalog, journal, env());
        let out = apply_ids(&mut e, &["a", "zzz"]);
        assert!(matches!(&out[0].1, ApplyOutcome::Failed(_)));
        assert_eq!(out[1], ("zzz".into(), ApplyOutcome::Failed("unknown tweak id: zzz".into())));
        assert_eq!(reg_a(&e), Some(RegData::Dword(0)));
        assert_eq!(svc_a(&e), Some(StartType::Automatic));
    }

    #[test]
    fn undo_skips_value_changed_since() {
        let d = tempfile::tempdir().unwrap();
        let mut e = engine_with(base_sys(), &d);
        apply_ids(&mut e, &["b"]);
        e.sys.reg_set(Hive::Hklm, r"Software\B", "V", Some(&RegData::Dword(7))).unwrap();
        let u = e.undo(&ids(&["b"]), false);
        assert_eq!(u.len(), 1);
        match &u[0].1[..] {
            [UndoOutcome::ChangedSince(desc)] => assert!(desc.contains(r"Software\B"), "{desc}"),
            other => panic!("expected ChangedSince, got {other:?}"),
        }
        assert_eq!(e.sys.reg_get(Hive::Hklm, r"Software\B", "V").unwrap(), Some(RegData::Dword(7)));
        assert_eq!(e.journal.for_tweak("b").len(), 1, "entry kept");
        assert!(e.undo(&ids(&["nothing-journaled"]), false)[0].1.is_empty());
    }

    #[test]
    fn undo_force_restores_changed_value() {
        let d = tempfile::tempdir().unwrap();
        let mut e = engine_with(base_sys(), &d);
        apply_ids(&mut e, &["b"]);
        e.sys.reg_set(Hive::Hklm, r"Software\B", "V", Some(&RegData::Dword(7))).unwrap();
        let u = e.undo(&ids(&["b"]), true);
        assert_eq!(u, vec![("b".into(), vec![UndoOutcome::Restored])]);
        assert_eq!(e.sys.reg_get(Hive::Hklm, r"Software\B", "V").unwrap(), None);
        assert!(e.journal.all().is_empty());
    }

    #[test]
    fn undo_after_crash_before_apply_reports_already_original() {
        let d = tempfile::tempdir().unwrap();
        let mut e = engine_with(base_sys(), &d);
        // simulate: journal written, then crash before the change was applied
        let a = e.catalog.get("a").unwrap().actions[0].clone();
        e.journal
            .record(JournalEntry {
                key: a.key("a", 0),
                tweak_id: "a".into(),
                action_index: 0,
                before: Snapshot::Reg(Some(RegData::Dword(0))),
                after: Snapshot::Reg(Some(RegData::Dword(1))),
                applied_at: now_rfc3339(),
                user_sid: None,
            })
            .unwrap();
        let u = e.undo(&ids(&["a"]), false);
        assert_eq!(u, vec![("a".into(), vec![UndoOutcome::AlreadyOriginal])]);
        assert_eq!(reg_a(&e), Some(RegData::Dword(0)));
        assert!(e.journal.all().is_empty(), "entry cleared");
    }

    #[test]
    fn undo_all_reverses_in_applied_order() {
        let d = tempfile::tempdir().unwrap();
        let mut e = engine_with(base_sys(), &d);
        apply_ids(&mut e, &["a"]);
        std::thread::sleep(std::time::Duration::from_millis(5));
        apply_ids(&mut e, &["b"]);
        let u = e.undo_all(false);
        assert_eq!(
            u,
            vec![
                ("b".into(), vec![UndoOutcome::Restored]),
                ("a".into(), vec![UndoOutcome::Restored, UndoOutcome::Restored]),
            ]
        );
        assert_eq!(reg_a(&e), Some(RegData::Dword(0)));
        assert_eq!(svc_a(&e), Some(StartType::Automatic));
        assert!(e.journal.all().is_empty());
    }

    #[test]
    fn appx_undo_reports_manual_reinstall() {
        let d = tempfile::tempdir().unwrap();
        let mut e = engine_with(base_sys(), &d);
        let mut all = HashSet::new();
        all.insert("candy".to_string());
        let out = e.apply(&ids(&["candy"]), &all, &mut |_| {});
        assert_eq!(out, vec![("candy".into(), ApplyOutcome::Applied)]);
        assert_eq!(e.sys.removed_appx, vec![("king.com.CandyCrushSaga".to_string(), true)]);
        let u = e.undo(&ids(&["candy"]), false);
        assert_eq!(
            u,
            vec![(
                "candy".into(),
                vec![UndoOutcome::ManualReinstall {
                    app: "king.com.CandyCrushSaga".into(),
                    hint: Some("9NBLGGH18846".into())
                }]
            )]
        );
    }

    #[test]
    fn hint_is_ignored_when_the_catalog_action_no_longer_matches_the_key() {
        let d = tempfile::tempdir().unwrap();
        let mut e = engine_with(base_sys(), &d);
        // Journaled by an older catalog in which action 1 of "a" was a
        // registry value; today action 1 is the SvcA service.
        e.sys.reg_set(Hive::Hklm, r"Software\Old", "V", Some(&RegData::Dword(9))).unwrap();
        e.journal
            .record(JournalEntry {
                key: ValueKey::Registry { hive: Hive::Hklm, path: r"software\old".into(), name: "v".into() },
                tweak_id: "a".into(),
                action_index: 1,
                before: Snapshot::Reg(None),
                after: Snapshot::Reg(Some(RegData::Dword(1))),
                applied_at: now_rfc3339(),
                user_sid: None,
            })
            .unwrap();
        let u = e.undo(&ids(&["a"]), false);
        match &u[0].1[..] {
            [UndoOutcome::ChangedSince(what)] => {
                assert!(what.contains(r"software\old"), "described by its key, not the service: {what}")
            }
            other => panic!("expected ChangedSince, got {other:?}"),
        }
        assert_eq!(svc_a(&e), Some(StartType::Automatic), "the service is not touched");
    }

    #[test]
    fn undo_entry_outside_catalog_uses_key() {
        let d = tempfile::tempdir().unwrap();
        let mut e = engine_with(base_sys(), &d);
        e.sys.reg_set(Hive::Hkcu, r"Software\Run", "App", Some(&RegData::String("x".into()))).unwrap();
        e.journal
            .record(JournalEntry {
                key: ValueKey::Registry {
                    hive: Hive::Hkcu,
                    path: r"software\run".into(),
                    name: "app".into(),
                },
                tweak_id: "startup:app".into(),
                action_index: 0,
                before: Snapshot::Reg(Some(RegData::String("orig".into()))),
                after: Snapshot::Reg(Some(RegData::String("x".into()))),
                applied_at: now_rfc3339(),
                user_sid: None,
            })
            .unwrap();
        let u = e.undo(&ids(&["startup:app"]), false);
        assert_eq!(u, vec![("startup:app".into(), vec![UndoOutcome::Restored])]);
        assert_eq!(
            e.sys.reg_get(Hive::Hkcu, r"Software\Run", "App").unwrap(),
            Some(RegData::String("orig".into()))
        );
    }

    #[test]
    fn set_registry_journaled_is_undone_by_undo_all() {
        let d = tempfile::tempdir().unwrap();
        let mut e = engine_with(base_sys(), &d);
        e.sys.reg_set(Hive::Hkcu, r"Software\Run", "App", Some(&RegData::String("on".into()))).unwrap();
        e.set_registry_journaled("startup:app", Hive::Hkcu, r"Software\Run", "App", None).unwrap();
        assert_eq!(e.sys.reg_get(Hive::Hkcu, r"Software\Run", "App").unwrap(), None);
        assert_eq!(e.journal.for_tweak("startup:app").len(), 1);
        let u = e.undo_all(false);
        assert_eq!(u, vec![("startup:app".into(), vec![UndoOutcome::Restored])]);
        assert_eq!(
            e.sys.reg_get(Hive::Hkcu, r"Software\Run", "App").unwrap(),
            Some(RegData::String("on".into()))
        );
        assert!(e.journal.all().is_empty());
    }

    #[test]
    fn per_user_entries_are_tagged_with_the_account_sid() {
        let d = tempfile::tempdir().unwrap();
        let mut e = engine_with(base_sys(), &d);
        apply_ids(&mut e, &["user", "a", "candy"]);
        let sid = Some(crate::system::fake::DEFAULT_SID.to_string());
        assert_eq!(e.journal.for_tweak("user")[0].user_sid, sid, "HKCU value");
        assert_eq!(e.journal.for_tweak("candy")[0].user_sid, sid, "Appx package");
        assert!(e.journal.for_tweak("a").iter().all(|x| x.user_sid.is_none()), "HKLM + service");
    }

    #[test]
    fn undo_all_leaves_another_accounts_entries_untouched() {
        let d = tempfile::tempdir().unwrap();
        let mut e = engine_with(base_sys(), &d);
        apply_ids(&mut e, &["user", "b"]);
        let (h, p, n) = (Hive::Hkcu, r"Software\U", "V");
        assert_eq!(e.sys.reg_get(h, p, n).unwrap(), Some(RegData::Dword(1)));

        // Featherweight now runs as a different account.
        e.sys.user_sid = Some("S-1-5-21-9-9-9-2002".into());
        let u = e.undo_all(false);
        let user = u.iter().find(|(id, _)| id == "user").unwrap();
        assert!(
            matches!(&user.1[..], [UndoOutcome::OtherAccount(w)] if w.contains(r"Software\U")),
            "{u:?}"
        );
        assert_eq!(u.iter().find(|(id, _)| id == "b").unwrap().1, vec![UndoOutcome::Restored]);
        assert_eq!(e.sys.reg_get(h, p, n).unwrap(), Some(RegData::Dword(1)), "not touched");
        assert_eq!(e.journal.for_tweak("user").len(), 1, "entry kept for its owner");
        assert!(e.journal.for_tweak("b").is_empty());

        // Back as the owner, it undoes normally.
        e.sys.user_sid = Some(crate::system::fake::DEFAULT_SID.into());
        assert_eq!(e.undo_all(false), vec![("user".into(), vec![UndoOutcome::Restored])]);
        assert_eq!(e.sys.reg_get(h, p, n).unwrap(), None);
    }

    #[test]
    fn same_key_applied_by_two_accounts_is_journaled_twice() {
        let d = tempfile::tempdir().unwrap();
        let mut e = engine_with(base_sys(), &d);
        apply_ids(&mut e, &["user"]);
        // Another account, whose own HKCU value is still absent.
        e.sys.user_sid = Some("S-1-5-21-9-9-9-2002".into());
        e.sys.reg_set(Hive::Hkcu, r"Software\U", "V", None).unwrap();
        assert_eq!(apply_ids(&mut e, &["user"]), vec![("user".into(), ApplyOutcome::Applied)]);
        let entries = e.journal.for_tweak("user");
        assert_eq!(entries.len(), 2);
        assert_ne!(entries[0].user_sid, entries[1].user_sid);
        // Undo as the second account removes only its own entry.
        let u = e.undo(&ids(&["user"]), false);
        assert!(u[0].1.contains(&UndoOutcome::Restored), "{u:?}");
        assert!(u[0].1.iter().any(|o| matches!(o, UndoOutcome::OtherAccount(_))), "{u:?}");
        let left = e.journal.for_tweak("user");
        assert_eq!(left.len(), 1);
        assert_eq!(left[0].user_sid.as_deref(), Some(crate::system::fake::DEFAULT_SID));
    }

    #[test]
    fn observe_key_reads_without_hint() {
        let mut sys = base_sys();
        let k = ValueKey::Service { name: "svca".into() };
        assert_eq!(
            observe_key(&k, &mut sys, None).unwrap(),
            Some(Snapshot::Service(StartType::Automatic))
        );
        let s = ValueKey::Script { tweak_id: "t".into(), index: 0 };
        assert_eq!(observe_key(&s, &mut sys, None).unwrap(), None);
    }
}
