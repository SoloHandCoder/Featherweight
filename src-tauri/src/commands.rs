//! Tauri command layer. Each command clones the shared state, does its work on
//! a blocking thread (registry reads, PowerShell checks and service stops can
//! take seconds) and converts engine types into camelCase views. The exact
//! TypeScript mirror of every view lives in `src/lib/types.ts`.

use std::collections::HashSet;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use serde::Serialize;
use tauri::{AppHandle, Emitter, State};

use crate::actions;
use crate::catalog::{ActionDef, AllUsers};
use crate::cleanup::{self, CleanupReport, CleanupTarget};
use crate::engine::{ApplyOutcome, Engine, Env, NaReason, Progress, TweakStatus, UndoOutcome};
use crate::env;
use crate::model::{Level, Preset, Section};
use crate::restore::{self, RestoreGuard, RestorePointOutcome};
use crate::startup::{self, StartupEntry, StartupFolders};
use crate::stats::Stats;
use crate::system::System;
use crate::system::windows::WindowsSystem;

// ------------------------------------------------------------------ views

#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Overview {
    pub build: u32,
    pub is_laptop: bool,
    pub per_user_allowed: bool,
    pub account_message: Option<String>,
    pub stats: Stats,
    /// Set when the undo journal can't be trusted; apply/undo are refused.
    pub journal_error: Option<String>,
}

#[derive(Serialize, Debug, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum NaReasonView {
    WrongBuild,
    AccountMismatch,
    MissingOnThisPc,
}

impl From<NaReason> for NaReasonView {
    fn from(r: NaReason) -> Self {
        match r {
            NaReason::WrongBuild => Self::WrongBuild,
            NaReason::AccountMismatch => Self::AccountMismatch,
            NaReason::MissingOnThisPc => Self::MissingOnThisPc,
        }
    }
}

#[derive(Serialize, Debug, Clone, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum StatusView {
    Applied,
    NotApplied,
    Partial,
    NotApplicable { reason: NaReasonView },
}

impl From<TweakStatus> for StatusView {
    fn from(s: TweakStatus) -> Self {
        match s {
            TweakStatus::Applied => Self::Applied,
            TweakStatus::NotApplied => Self::NotApplied,
            TweakStatus::Partial => Self::Partial,
            TweakStatus::NotApplicable(r) => Self::NotApplicable { reason: r.into() },
        }
    }
}

#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TweakView {
    pub id: String,
    pub title: String,
    pub description: String,
    pub section: Section,
    /// Effective level on this PC (`laptop_level` wins on laptops).
    pub level: Level,
    pub irreversible: bool,
    pub per_user: bool,
    pub status: StatusView,
    /// `actions::describe` for each action, in order.
    pub changes: Vec<String>,
    /// From the first Appx action that has one.
    pub reinstall_hint: Option<String>,
    /// Some Appx action can optionally be removed for all users.
    pub all_users_optional: bool,
    /// The tweak that can't be on at the same time as this one.
    pub exclusive_with: Option<String>,
}

#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ApplyOutcomeView {
    Applied,
    AlreadyApplied,
    Failed { message: String },
    NotApplicable { reason: NaReasonView },
}

impl From<ApplyOutcome> for ApplyOutcomeView {
    fn from(o: ApplyOutcome) -> Self {
        match o {
            ApplyOutcome::Applied => Self::Applied,
            ApplyOutcome::AlreadyApplied => Self::AlreadyApplied,
            ApplyOutcome::Failed(message) => Self::Failed { message },
            ApplyOutcome::NotApplicable(r) => Self::NotApplicable { reason: r.into() },
        }
    }
}

#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ApplyResult {
    pub id: String,
    pub outcome: ApplyOutcomeView,
}

#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ApplyReport {
    pub results: Vec<ApplyResult>,
}

#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum UndoOutcomeView {
    Restored,
    AlreadyOriginal,
    /// `what` names the value that something else changed since apply.
    ChangedSince { what: String },
    /// `app` is the removed package's name, `hint` its Store product id.
    ManualReinstall { app: String, hint: Option<String> },
    /// Journaled for another Windows account; `what` names the value.
    OtherAccount { what: String },
    Failed { message: String },
}

impl From<UndoOutcome> for UndoOutcomeView {
    fn from(o: UndoOutcome) -> Self {
        match o {
            UndoOutcome::Restored => Self::Restored,
            UndoOutcome::AlreadyOriginal => Self::AlreadyOriginal,
            UndoOutcome::ChangedSince(what) => Self::ChangedSince { what },
            UndoOutcome::ManualReinstall { app, hint } => Self::ManualReinstall { app, hint },
            UndoOutcome::OtherAccount(what) => Self::OtherAccount { what },
            UndoOutcome::Failed(message) => Self::Failed { message },
        }
    }
}

#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UndoResult {
    pub id: String,
    /// One per journaled value of the tweak, in undo order.
    pub outcomes: Vec<UndoOutcomeView>,
}

#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct UndoReport {
    pub results: Vec<UndoResult>,
}

#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct ProgressView {
    pub done: usize,
    pub total: usize,
    pub current: String,
}

impl From<Progress> for ProgressView {
    fn from(p: Progress) -> Self {
        ProgressView { done: p.done, total: p.total, current: p.current }
    }
}

#[derive(Serialize, Debug, Clone, PartialEq)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum RestorePointView {
    Created,
    AlreadyMadeThisSession,
    Throttled,
    Disabled,
    Failed { message: String },
}

impl From<RestorePointOutcome> for RestorePointView {
    fn from(o: RestorePointOutcome) -> Self {
        match o {
            RestorePointOutcome::Created => Self::Created,
            RestorePointOutcome::AlreadyMadeThisSession => Self::AlreadyMadeThisSession,
            RestorePointOutcome::Throttled => Self::Throttled,
            RestorePointOutcome::Disabled => Self::Disabled,
            RestorePointOutcome::Failed(message) => Self::Failed { message },
        }
    }
}

// ------------------------------------------------------------- pure helpers

pub fn tweak_views<S: System>(e: &mut Engine<S>) -> Vec<TweakView> {
    let tweaks = e.catalog.tweaks.clone();
    tweaks
        .iter()
        .map(|t| TweakView {
            id: t.id.clone(),
            title: t.title.clone(),
            description: t.description.clone(),
            section: t.section,
            level: env::effective_level(t, &e.env),
            irreversible: t.irreversible,
            per_user: t.is_per_user(),
            status: e.status(&t.id).into(),
            changes: t.actions.iter().map(actions::describe).collect(),
            reinstall_hint: t.actions.iter().find_map(|a| match a {
                ActionDef::Appx { reinstall, .. } => reinstall.clone(),
                _ => None,
            }),
            all_users_optional: t
                .actions
                .iter()
                .any(|a| matches!(a, ActionDef::Appx { all_users: AllUsers::Optional, .. })),
            exclusive_with: t.exclusive_with.clone(),
        })
        .collect()
}

pub fn apply_report(results: Vec<(String, ApplyOutcome)>) -> ApplyReport {
    ApplyReport {
        results: results
            .into_iter()
            .map(|(id, o)| ApplyResult { id, outcome: o.into() })
            .collect(),
    }
}

pub fn undo_report(results: Vec<(String, Vec<UndoOutcome>)>) -> UndoReport {
    UndoReport {
        results: results
            .into_iter()
            .map(|(id, outs)| UndoResult { id, outcomes: outs.into_iter().map(Into::into).collect() })
            .collect(),
    }
}

// ------------------------------------------------------------ state & glue

pub struct AppState {
    pub engine: Arc<Mutex<Engine<WindowsSystem>>>,
    pub restore: Arc<Mutex<RestoreGuard>>,
    /// Why the undo journal can't be used; `Some` blocks every journaled change.
    pub journal_error: Option<String>,
}

pub const APPLY_PROGRESS: &str = "apply-progress";

fn lock<T>(m: &Mutex<T>) -> Result<MutexGuard<'_, T>, String> {
    m.lock().map_err(|_| {
        "An earlier operation crashed. Restart Featherweight before trying again.".to_string()
    })
}

/// Runs `f` on the blocking thread pool so the UI thread never waits on it.
async fn blocking<T, F>(f: F) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, String> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(f)
        .await
        .map_err(|e| format!("background task failed: {e}"))?
}

fn journal_ok(state: &AppState) -> Result<(), String> {
    match &state.journal_error {
        Some(e) => Err(e.clone()),
        None => Ok(()),
    }
}

/// Set once `ensure_restore_point` has been tried this session, whatever the
/// outcome, so the user can still proceed after being told it failed.
static RESTORE_ATTEMPTED: AtomicBool = AtomicBool::new(false);

/// Refuses journaled changes until a restore point was at least attempted.
fn require_restore_attempted(flag: &AtomicBool) -> Result<(), String> {
    if flag.load(Ordering::SeqCst) {
        Ok(())
    } else {
        Err("Make a restore point first".to_string())
    }
}

/// Builds the Home overview; a stat that could not be read counts as 0.
fn build_overview(
    env: &Env,
    startup_enabled: Result<u32, String>,
    services_running: Result<u32, String>,
    junk_bytes: Result<u64, String>,
    journal_error: Option<String>,
) -> Overview {
    Overview {
        build: env.build,
        is_laptop: env.is_laptop,
        per_user_allowed: env.per_user_allowed,
        account_message: crate::env::account_message(env),
        stats: Stats {
            startup_enabled: startup_enabled.unwrap_or(0),
            services_running: services_running.unwrap_or(0),
            junk_bytes: junk_bytes.unwrap_or(0),
        },
        journal_error,
    }
}

fn startup_folders() -> Result<StartupFolders, String> {
    match (cleanup::known::startup(), cleanup::known::common_startup()) {
        (Some(user), Some(common)) => Ok(StartupFolders { user, common }),
        _ => Err("could not locate the Startup folders".to_string()),
    }
}

fn scanned_targets() -> Vec<CleanupTarget> {
    let mut targets = cleanup::default_targets();
    cleanup::scan(&mut targets);
    targets
}

// ---------------------------------------------------------------- commands

#[tauri::command]
pub async fn get_overview(state: State<'_, AppState>) -> Result<Overview, String> {
    let engine = state.engine.clone();
    let journal_error = state.journal_error.clone();
    blocking(move || {
        let folders = startup_folders();
        // Scanning can be slow; do it before taking the engine lock.
        let junk_bytes: u64 = scanned_targets().iter().map(|t| t.bytes).sum();
        let e = lock(&engine)?;
        let startup_enabled = folders.and_then(|f| {
            startup::list(&e.sys, &f)
                .map(|l| l.iter().filter(|s| s.enabled).count() as u32)
                .map_err(|err| err.to_string())
        });
        let services = Stats::collect(&e.sys, 0, 0)
            .map(|s| s.services_running)
            .map_err(|err| err.to_string());
        Ok(build_overview(&e.env, startup_enabled, services, Ok(junk_bytes), journal_error))
    })
    .await
}

#[tauri::command]
pub async fn list_tweaks(state: State<'_, AppState>) -> Result<Vec<TweakView>, String> {
    let engine = state.engine.clone();
    blocking(move || Ok(tweak_views(&mut *lock(&engine)?))).await
}

#[tauri::command]
pub async fn preview_preset(
    state: State<'_, AppState>,
    preset: Preset,
) -> Result<Vec<String>, String> {
    let engine = state.engine.clone();
    blocking(move || Ok(env::resolve_preset(&mut *lock(&engine)?, preset))).await
}

#[tauri::command]
pub async fn apply_tweaks(
    app: AppHandle,
    state: State<'_, AppState>,
    ids: Vec<String>,
    all_users: Vec<String>,
) -> Result<ApplyReport, String> {
    journal_ok(&state)?;
    require_restore_attempted(&RESTORE_ATTEMPTED)?;
    let engine = state.engine.clone();
    blocking(move || {
        let mut e = lock(&engine)?;
        let all_users: HashSet<String> = all_users.into_iter().collect();
        let results = e.apply(&ids, &all_users, &mut |p| {
            let _ = app.emit(APPLY_PROGRESS, ProgressView::from(p));
        });
        // Final tick so the UI can show completion.
        let n = ids.len();
        let _ = app.emit(APPLY_PROGRESS, ProgressView { done: n, total: n, current: String::new() });
        Ok(apply_report(results))
    })
    .await
}

#[tauri::command]
pub async fn undo_tweaks(
    state: State<'_, AppState>,
    ids: Vec<String>,
    force: bool,
) -> Result<UndoReport, String> {
    journal_ok(&state)?;
    let engine = state.engine.clone();
    blocking(move || Ok(undo_report(lock(&engine)?.undo(&ids, force)))).await
}

#[tauri::command]
pub async fn undo_all(state: State<'_, AppState>, force: bool) -> Result<UndoReport, String> {
    journal_ok(&state)?;
    let engine = state.engine.clone();
    blocking(move || Ok(undo_report(lock(&engine)?.undo_all(force)))).await
}

#[tauri::command]
pub async fn ensure_restore_point(state: State<'_, AppState>) -> Result<RestorePointView, String> {
    let engine = state.engine.clone();
    let guard = state.restore.clone();
    blocking(move || {
        let mut e = lock(&engine)?;
        let mut g = lock(&guard)?;
        let outcome = restore::ensure(&mut g, &mut e.sys);
        // Any outcome counts: the user has been told and may still proceed.
        RESTORE_ATTEMPTED.store(true, Ordering::SeqCst);
        Ok(outcome.into())
    })
    .await
}

#[tauri::command]
pub async fn enable_system_restore(state: State<'_, AppState>) -> Result<(), String> {
    let engine = state.engine.clone();
    blocking(move || restore::enable(&mut lock(&engine)?.sys).map_err(|e| e.to_string())).await
}

#[tauri::command]
pub async fn list_startup(state: State<'_, AppState>) -> Result<Vec<StartupEntry>, String> {
    let engine = state.engine.clone();
    blocking(move || {
        let folders = startup_folders()?;
        startup::list(&lock(&engine)?.sys, &folders).map_err(|e| e.to_string())
    })
    .await
}

#[tauri::command]
pub async fn set_startup_enabled(
    state: State<'_, AppState>,
    id: String,
    enabled: bool,
) -> Result<(), String> {
    journal_ok(&state)?;
    require_restore_attempted(&RESTORE_ATTEMPTED)?;
    let engine = state.engine.clone();
    blocking(move || {
        let folders = startup_folders()?;
        let mut e = lock(&engine)?;
        let entry = startup::list(&e.sys, &folders)
            .map_err(|err| err.to_string())?
            .into_iter()
            .find(|s| s.id == id)
            .ok_or_else(|| format!("startup entry not found: {id}"))?;
        if entry.per_user && !e.env.per_user_allowed {
            return Err(env::account_message(&e.env).unwrap_or_default());
        }
        startup::set_enabled(&mut e, &entry, enabled).map_err(|err| err.to_string())
    })
    .await
}

#[tauri::command]
pub async fn scan_cleanup() -> Result<Vec<CleanupTarget>, String> {
    blocking(|| Ok(scanned_targets())).await
}

#[tauri::command]
pub async fn run_cleanup(
    state: State<'_, AppState>,
    ids: Vec<String>,
) -> Result<CleanupReport, String> {
    let engine = state.engine.clone();
    blocking(move || {
        // Paths always come from the built-in list, never from the frontend.
        let targets = cleanup::default_targets();
        if let Some(bad) = ids.iter().find(|id| !targets.iter().any(|t| &t.id == *id)) {
            return Err(format!("unknown cleanup target: {bad}"));
        }
        let mut e = lock(&engine)?;
        let mut total = CleanupReport::default();
        for t in targets.iter().filter(|t| ids.contains(&t.id)) {
            let r = cleanup::clean(&mut e.sys, t);
            total.freed_bytes += r.freed_bytes;
            total.skipped_files += r.skipped_files;
        }
        Ok(total)
    })
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::system::System;
    use crate::catalog::Catalog;
    use crate::engine::{Engine, Env};
    use crate::journal::Journal;
    use crate::model::{Hive, RegData};
    use crate::system::fake::FakeSystem;

    const TWEAK_CANDY: &str = r#"
id = "candy"
title = "Remove Candy Crush"
description = "A preinstalled game."
section = "remove"
level = "advanced"
laptop_level = "safe"

[[actions]]
type = "registry"
hive = "hklm"
path = 'Software\Policies\Candy'
name = "Off"
value = { dword = 1 }

[[actions]]
type = "appx"
package = "king.com.CandyCrushSaga"
all_users = "optional"
reinstall = "9NBLGGH18846"
"#;

    const TWEAK_NEW: &str = r#"
id = "newbuild"
title = "New build only"
description = "D"
section = "background"
level = "safe"
min_build = 99999

[[actions]]
type = "registry"
hive = "hklm"
path = 'Software\N'
name = "V"
value = { dword = 1 }
"#;

    fn engine(d: &tempfile::TempDir, sys: FakeSystem) -> Engine<FakeSystem> {
        let catalog = Catalog::from_strs(&[TWEAK_CANDY, TWEAK_NEW]).unwrap();
        let journal = Journal::load(d.path().join("journal.json")).unwrap();
        let env = Env { build: 22631, is_laptop: true, per_user_allowed: true };
        Engine::new(sys, catalog, journal, env)
    }

    #[test]
    fn commands_serialize_camel_case() {
        let v = TweakView {
            id: "x".into(),
            title: "T".into(),
            description: "D".into(),
            section: crate::model::Section::Remove,
            level: crate::model::Level::Safe,
            irreversible: false,
            per_user: true,
            status: StatusView::NotApplicable { reason: NaReasonView::WrongBuild },
            changes: vec!["c".into()],
            reinstall_hint: Some("hint".into()),
            all_users_optional: true,
            exclusive_with: Some("y".into()),
        };
        let j = serde_json::to_value(&v).unwrap();
        assert_eq!(j["exclusiveWith"], "y");
        assert_eq!(j["reinstallHint"], "hint");
        assert_eq!(j["perUser"], true);
        assert_eq!(j["allUsersOptional"], true);
        assert_eq!(j["section"], "remove");
        assert_eq!(j["level"], "safe");
        assert_eq!(j["status"], serde_json::json!({ "kind": "notApplicable", "reason": "wrongBuild" }));
    }

    #[test]
    fn commands_tweak_views_from_engine() {
        let d = tempfile::tempdir().unwrap();
        let sys = FakeSystem::new().with_appx("king.com.CandyCrushSaga", true);
        let mut e = engine(&d, sys);
        let views = tweak_views(&mut e);
        assert_eq!(views.len(), 2);

        let candy = &views[0];
        assert_eq!(candy.id, "candy");
        // laptop_level wins on a laptop.
        assert_eq!(candy.level, crate::model::Level::Safe);
        assert!(candy.per_user);
        assert!(candy.all_users_optional);
        assert_eq!(candy.reinstall_hint.as_deref(), Some("9NBLGGH18846"));
        assert_eq!(
            candy.changes,
            vec![
                r"HKLM\Software\Policies\Candy\Off = 1 (DWORD)".to_string(),
                r#"Remove app "king.com.CandyCrushSaga""#.to_string(),
            ]
        );
        assert_eq!(candy.status, StatusView::NotApplied);

        let newer = &views[1];
        assert_eq!(newer.status, StatusView::NotApplicable { reason: NaReasonView::WrongBuild });
        assert!(!newer.per_user);
        assert!(!newer.all_users_optional);
        assert_eq!(newer.reinstall_hint, None);
    }

    #[test]
    fn commands_status_partial_and_applied() {
        let d = tempfile::tempdir().unwrap();
        let sys = FakeSystem::new()
            .with_appx("king.com.CandyCrushSaga", false)
            .with_reg(Hive::Hklm, r"Software\Policies\Candy", "Off", RegData::Dword(0));
        let mut e = engine(&d, sys);
        assert_eq!(tweak_views(&mut e)[0].status, StatusView::Partial);
        e.sys.reg_set(Hive::Hklm, r"Software\Policies\Candy", "Off", Some(&RegData::Dword(1))).unwrap();
        assert_eq!(tweak_views(&mut e)[0].status, StatusView::Applied);
    }

    #[test]
    fn commands_apply_and_undo_reports_serialize() {
        let d = tempfile::tempdir().unwrap();
        let sys = FakeSystem::new().with_appx("king.com.CandyCrushSaga", true);
        let mut e = engine(&d, sys);
        let ids = vec!["candy".to_string(), "newbuild".to_string(), "nope".to_string()];
        let mut seen = Vec::new();
        let rep = apply_report(e.apply(&ids, &Default::default(), &mut |p| seen.push(ProgressView::from(p))));
        assert_eq!(seen.len(), 3);
        let j = serde_json::to_value(&rep).unwrap();
        assert_eq!(j["results"][0], serde_json::json!({ "id": "candy", "outcome": { "kind": "applied" } }));
        assert_eq!(
            j["results"][1]["outcome"],
            serde_json::json!({ "kind": "notApplicable", "reason": "wrongBuild" })
        );
        assert_eq!(j["results"][2]["outcome"]["kind"], "failed");
        assert!(j["results"][2]["outcome"]["message"].as_str().unwrap().contains("nope"));
        assert_eq!(serde_json::to_value(&seen[0]).unwrap()["current"], "Remove Candy Crush");

        let rep = undo_report(e.undo(&["candy".to_string()], false));
        let j = serde_json::to_value(&rep).unwrap();
        assert_eq!(j["results"][0]["id"], "candy");
        let outs = j["results"][0]["outcomes"].as_array().unwrap();
        assert_eq!(outs.len(), 2);
        assert_eq!(outs[0], serde_json::json!({ "kind": "manualReinstall", "app": "king.com.CandyCrushSaga", "hint": "9NBLGGH18846" }));
        assert_eq!(outs[1], serde_json::json!({ "kind": "restored" }));
    }

    #[test]
    fn undo_of_bundled_app_removal_lists_each_app_with_its_own_store_id() {
        let d = tempfile::tempdir().unwrap();
        let catalog = Catalog::bundled();
        let apps: Vec<(String, Option<String>)> = catalog
            .get("remove-junk-apps")
            .unwrap()
            .actions
            .iter()
            .filter_map(|a| match a {
                ActionDef::Appx { package, reinstall, .. } => Some((package.clone(), reinstall.clone())),
                _ => None,
            })
            .collect();
        assert!(apps.len() > 1);
        let mut sys = FakeSystem::new();
        for (p, _) in &apps {
            sys = sys.with_appx(p, true);
        }
        let journal = Journal::load(d.path().join("journal.json")).unwrap();
        let env = Env { build: 22631, is_laptop: false, per_user_allowed: true };
        let mut e = Engine::new(sys, catalog, journal, env);
        let ids = vec!["remove-junk-apps".to_string()];
        e.apply(&ids, &Default::default(), &mut |_| {});
        let j = serde_json::to_value(undo_report(e.undo(&ids, false))).unwrap();
        let outs = j["results"][0]["outcomes"].as_array().unwrap();
        assert_eq!(outs.len(), apps.len(), "one outcome per removed app");
        let mut got: Vec<(String, Option<String>)> = outs
            .iter()
            .map(|o| {
                assert_eq!(o["kind"], "manualReinstall");
                (o["app"].as_str().unwrap().to_string(), o["hint"].as_str().map(String::from))
            })
            .collect();
        let mut want = apps.clone();
        got.sort();
        want.sort();
        assert_eq!(got, want);
        let bare = regex::Regex::new(r"^[A-Za-z0-9]+$").unwrap();
        assert!(got.iter().filter_map(|(_, h)| h.as_deref()).all(|h| bare.is_match(h)));
    }

    #[test]
    fn restore_gate_refuses_until_attempted() {
        use std::sync::atomic::{AtomicBool, Ordering};
        let flag = AtomicBool::new(false);
        let err = require_restore_attempted(&flag).unwrap_err();
        assert!(err.contains("Make a restore point first"));
        flag.store(true, Ordering::SeqCst);
        assert!(require_restore_attempted(&flag).is_ok());
    }

    #[test]
    fn overview_survives_failed_stats() {
        let env = Env { build: 22631, is_laptop: true, per_user_allowed: false };
        let o = build_overview(
            &env,
            Err("no startup folders".into()),
            Err("scm".into()),
            Err("scan".into()),
            Some("journal bad".into()),
        );
        assert_eq!(o.build, 22631);
        assert!(o.is_laptop);
        assert!(!o.per_user_allowed);
        assert!(o.account_message.is_some());
        assert_eq!(o.stats, Stats { startup_enabled: 0, services_running: 0, junk_bytes: 0 });
        assert_eq!(o.journal_error.as_deref(), Some("journal bad"));
        let o = build_overview(&env, Ok(3), Ok(50), Ok(99), None);
        assert_eq!(o.stats, Stats { startup_enabled: 3, services_running: 50, junk_bytes: 99 });
    }

    #[test]
    fn commands_restore_outcome_is_tagged() {
        use crate::restore::RestorePointOutcome as R;
        let j = serde_json::to_value(RestorePointView::from(R::Failed("boom".into()))).unwrap();
        assert_eq!(j, serde_json::json!({ "kind": "failed", "message": "boom" }));
        let j = serde_json::to_value(RestorePointView::from(R::AlreadyMadeThisSession)).unwrap();
        assert_eq!(j, serde_json::json!({ "kind": "alreadyMadeThisSession" }));
    }
}
