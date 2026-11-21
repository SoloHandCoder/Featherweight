# Featherweight Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build Featherweight, a Tauri 2 Windows optimizer/debloater with presets, explained tweaks, a restore point, per-tweak undo and "Undo all".

**Architecture:** A Rust backend holds a declarative tweak catalog (TOML, bundled at compile time) and an engine that checks/applies/undoes actions through a `System` trait. A `FakeSystem` makes all engine logic testable off-Windows; `WindowsSystem` does the real work. A per-value JSON journal in `%ProgramData%\Featherweight\` powers undo. A Svelte 5 frontend talks to the backend only through typed Tauri commands.

**Tech Stack:** Tauri 2, Rust (serde, toml, serde_json, thiserror, winreg, windows crate), Svelte 5 + TypeScript + Vite, lucide-svelte, Vitest.

**Spec:** `docs/superpowers/specs/2026-11-21-featherweight-design.md` (mockups in `docs/superpowers/specs/mockups/`)

## Global Constraints

- OS: Windows 10 22H2 (build 19045) and Windows 11 23H2+ (build 22631+), x64 only.
- App manifest requests `requireAdministrator`.
- Never: turn off Windows Defender, block Windows Update entirely, RAM/registry "cleaners", silent changes, own telemetry, bundled software, any network calls in v1.
- Every tweak has a plain-words description and shows its exact change in the explainer panel.
- Levels: `safe` / `advanced`. Presets: **Everyday** = SAFE items in Remove + Background (not startup apps) + Cleanup; **Gaming** = Everyday + SAFE Gaming items.
- Irreversible steps (app removal, cleanup) always get a separate confirmation screen, even inside a preset.
- Tweaks never touch the same value as another tweak (enforced by a test).
- Undo restores a value only if it still equals what Featherweight set; otherwise skip and report, with "restore anyway".
- Dark theme, sky-blue → violet accent used sparingly; Lucide icons, **no emoji in the UI**; Segoe UI Variable.
- License MIT. Commits and PRs carry **no AI attribution lines** (no `Co-Authored-By`, no "Generated with").
- Tweaks are never applied to the developer's own PC by tooling. Integration tests that change real system state run only in Windows Sandbox / a VM / CI.

## Review Focus

1. **Applying a tweak that's already applied** — must be a no-op, and must not overwrite the journal's original "before" value with the applied value (else undo restores the wrong thing). Test in Task 5 and Task 6.
2. **App crashes between journaling and applying** — on next launch, undo finds the value still equal to "before" and reports `AlreadyOriginal` instead of `ChangedSince`. Test in Task 6.
3. **Service, task or package that doesn't exist on this edition** (e.g. Fax on Home, Recall on Win10) — tweak shows as not applicable and is hidden, never "failed". Test in Task 4 and Task 6.
4. **Locked or access-denied files during cleanup** — skip them, keep going, report only bytes actually freed. Test in Task 11.
5. **Restore point refused** (System Restore off, or one made in the last 24 h) — apply still proceeds after the user is told; outcome is reported distinctly. Test in Task 9.

---

## File Structure

```
featherweight/
  package.json  vite.config.ts  svelte.config.js  tsconfig.json  index.html
  LICENSE  README.md  .gitignore
  .github/workflows/ci.yml          # lint, cargo test, vitest, build on windows-latest
  .github/workflows/release.yml     # tag → installer + portable exe on GitHub Releases
  tweaks/*.toml                     # one file per tweak
  assets/logo/                      # logo source SVG, README header, social preview
  src/                              # Svelte frontend
    main.ts  App.svelte
    styles/tokens.css  styles/base.css
    lib/types.ts  lib/api.ts  lib/format.ts  lib/state.svelte.ts
    components/Sidebar.svelte  AccountBanner.svelte  StatTile.svelte  PresetCard.svelte
               TweakRow.svelte  ExplainerPanel.svelte  ConfirmDialog.svelte
               ResultSummary.svelte  LevelPill.svelte  Logo.svelte
    pages/Home.svelte  Section.svelte  Background.svelte  Cleanup.svelte
    lib/*.test.ts
  src-tauri/
    Cargo.toml  build.rs  tauri.conf.json  app.manifest  icons/
    src/main.rs  lib.rs  commands.rs
    src/catalog/mod.rs  catalog/validate.rs
    src/model.rs                    # ValueKey, Snapshot, RegData, StartType, Hive, enums
    src/system/mod.rs  system/fake.rs  system/windows.rs
    src/actions.rs
    src/journal.rs
    src/engine.rs
    src/env.rs                      # build, laptop, account check, presets
    src/restore.rs  src/startup.rs  src/cleanup.rs  src/stats.rs
    tests/windows_integration.rs    # #[ignore], Windows only
```

---

### Task 1: Scaffold the repo

**Files:**
- Create: everything under "File Structure" that is config (`package.json`, `vite.config.ts`, `svelte.config.js`, `tsconfig.json`, `index.html`, `src/main.ts`, `src/App.svelte`, `src-tauri/*` config, `src-tauri/src/main.rs`, `lib.rs`), `LICENSE` (MIT), `.gitignore` (add `.superpowers/`, `node_modules/`, `target/`, `dist/`), `.github/workflows/ci.yml`

**Interfaces:**
- Produces: `npm run dev`, `npm run build`, `npm run check` (svelte-check), `npm test` (vitest), `cargo test` in `src-tauri/`.

- [ ] **Step 1:** `git init`, then create the app with `npm create tauri-app@latest` (Svelte, TypeScript, npm) into the repo root. Window: title "Featherweight", 1100×720, min 960×640, `decorations: true`, dark background `#0f1115`.
- [ ] **Step 2:** Add `src-tauri/app.manifest` with `<requestedExecutionLevel level="requireAdministrator" uiAccess="false"/>` and embed it from `build.rs` via `tauri_build::WindowsAttributes::new().app_manifest(include_str!("app.manifest"))`.
- [ ] **Step 3:** Add `ci.yml`: on push/PR, `windows-latest`: `npm ci`, `npm run check`, `npm test`, `cargo test --manifest-path src-tauri/Cargo.toml`, `npm run tauri build -- --no-bundle`.
- [ ] **Step 4:** Verify: `npm run check` → 0 errors; `cargo test` → ok; `npm run tauri dev` opens a window (elevation prompt expected).
- [ ] **Step 5:** Commit: `git commit -m "chore: scaffold Tauri 2 + Svelte 5 app"` (no attribution trailer).

---

### Task 2: Core model types and the tweak catalog

**Files:**
- Create: `src-tauri/src/model.rs`, `src-tauri/src/catalog/mod.rs`, `src-tauri/src/catalog/validate.rs`, `tweaks/_example.toml` (test fixture only, excluded from the real catalog)

**Interfaces:**
- Produces (`model.rs`, all `Serialize + Deserialize + Clone + PartialEq + Debug`):
  - `enum Hive { Hklm, Hkcu }`
  - `enum RegData { Dword(u32), Qword(u64), String(String), Binary(Vec<u8>) }`
  - `enum StartType { Disabled, Manual, Automatic }`
  - `enum Level { Safe, Advanced }`, `enum Section { Remove, Background, Gaming, Cleanup }`, `enum Preset { Everyday, Gaming }`
  - `enum ValueKey { Registry{hive,path,name}, Service{name}, Task{path}, Appx{package}, PowerScheme, Script{tweak_id, index:usize} }` (also `Eq + Hash`)
  - `enum Snapshot { Reg(Option<RegData>), Service(StartType), Task(bool), Appx(bool), Power(String), Script(String) }`
- Produces (`catalog`):
  - `enum ActionDef` (serde `tag = "type"`, `rename_all = "snake_case"`):
    `Registry{hive, path, name, value: Option<RegData>}` (None = delete value),
    `Service{name, start: StartType}`, `ScheduledTask{path, enabled: bool}`,
    `Appx{package: String, all_users: AllUsers, reinstall: Option<String>}` with `enum AllUsers { Always, Optional }`,
    `PowerPlan{scheme_guid: String, duplicate_from: Option<String>}`,
    `PowerShell{check: String, apply: String, undo: String}` (check prints `1` when applied)
  - `struct TweakDef { id, title, description, section: Section, level: Level, laptop_level: Option<Level>, presets: Vec<Preset>, min_build: Option<u32>, max_build: Option<u32>, irreversible: bool, actions: Vec<ActionDef> }`
  - `struct Catalog { tweaks: Vec<TweakDef> }`, `fn Catalog::bundled() -> Catalog` (uses `include_dir!` or a `build.rs`-generated list of `tweaks/*.toml`, skipping files starting with `_`), `fn Catalog::from_strs(&[&str]) -> Result<Catalog, CatalogError>`, `fn Catalog::get(&self, id:&str) -> Option<&TweakDef>`
  - `fn TweakDef::is_per_user(&self) -> bool` — true if any action is `Registry{hive: Hkcu}` or `Appx`.
  - `fn validate(c:&Catalog) -> Vec<CatalogError>`; `enum CatalogError { Parse{file,msg}, DuplicateId(String), OverlappingValue{key: ValueKey, a: String, b: String}, BadBuildRange(String), EmptyActions(String), EmptyText(String) }`
  - `fn ActionDef::key(&self, tweak_id:&str, index:usize) -> ValueKey`

- [ ] **Step 1: Write failing tests** in `catalog/mod.rs` `#[cfg(test)]`:
  - `parses_example_tweak` — `_example.toml` with one registry + one service action parses; `level == Safe`, `actions.len() == 2`.
  - `validate_rejects_duplicate_ids` → `DuplicateId("a")`.
  - `validate_rejects_overlapping_registry_value` — tweaks `a`,`b` both set `HKCU\Software\X : Y` → `OverlappingValue{a:"a", b:"b", ..}`.
  - `validate_rejects_min_greater_than_max` → `BadBuildRange`.
  - `validate_rejects_empty_description_or_actions`.
  - `per_user_detection` — HKCU registry → true; HKLM-only service → false.
- [ ] **Step 2:** `cargo test catalog` → FAIL (types missing).
- [ ] **Step 3:** Implement the types and functions above.
- [ ] **Step 4:** `cargo test catalog` → PASS.
- [ ] **Step 5:** Commit `feat: tweak catalog model and validation`.

---

### Task 3: The `System` trait and `FakeSystem`

**Files:**
- Create: `src-tauri/src/system/mod.rs`, `src-tauri/src/system/fake.rs`

**Interfaces:**
- Produces:
```rust
pub trait System {
    fn reg_get(&self, hive: Hive, path: &str, name: &str) -> Result<Option<RegData>, SysError>;
    fn reg_set(&mut self, hive: Hive, path: &str, name: &str, value: Option<&RegData>) -> Result<(), SysError>; // None deletes; creates keys as needed
    fn service_start(&self, name: &str) -> Result<Option<StartType>, SysError>;   // None = service absent
    fn set_service_start(&mut self, name: &str, start: StartType) -> Result<(), SysError>; // Disabled also stops it
    fn task_enabled(&self, path: &str) -> Result<Option<bool>, SysError>;        // None = task absent
    fn set_task_enabled(&mut self, path: &str, enabled: bool) -> Result<(), SysError>;
    fn appx_installed(&self, package: &str) -> Result<Option<bool>, SysError>;   // None = unknown package name on this build
    fn remove_appx(&mut self, package: &str, all_users: bool) -> Result<(), SysError>;
    fn active_power_scheme(&self) -> Result<String, SysError>;                   // lowercase GUID
    fn ensure_power_scheme(&mut self, guid: &str, duplicate_from: Option<&str>) -> Result<(), SysError>;
    fn set_power_scheme(&mut self, guid: &str) -> Result<(), SysError>;
    fn run_powershell(&mut self, script: &str) -> Result<String, SysError>;      // trimmed stdout
}
#[derive(thiserror::Error, Debug)] pub enum SysError { #[error("access denied: {0}")] AccessDenied(String), #[error("not found: {0}")] NotFound(String), #[error("{0}")] Other(String) }
```
- `FakeSystem` — in-memory maps for each kind; `pub fail_next: Option<SysError>` makes the next mutating call fail (for rollback tests); `pub ps_responses: HashMap<String,String>` scripted PowerShell output; `known_appx: HashSet<String>`.

- [ ] **Step 1: Failing tests** in `fake.rs`: `reg_roundtrip_and_delete`, `service_absent_is_none`, `fail_next_fails_once`.
- [ ] **Step 2:** `cargo test system` → FAIL.
- [ ] **Step 3:** Implement trait + `FakeSystem`.
- [ ] **Step 4:** `cargo test system` → PASS.
- [ ] **Step 5:** Commit `feat: System trait and in-memory fake`.

---

### Task 4: Actions — observe, target, apply, restore

**Files:**
- Create: `src-tauri/src/actions.rs`

**Interfaces:**
- Consumes: `ActionDef`, `System`, `Snapshot`, `ValueKey`.
- Produces:
  - `enum Observed { Present(Snapshot), NotApplicable }`
  - `fn observe<S: System>(a:&ActionDef, sys:&S) -> Result<Observed, SysError>` — absent service/task/unknown appx → `NotApplicable`; registry absent → `Present(Reg(None))`; PowerShell → `Script(check output)`.
  - `fn target(a:&ActionDef) -> Snapshot` — registry → `Reg(value.clone())`; service → `Service(start)`; task → `Task(enabled)`; appx → `Appx(false)`; power → `Power(scheme_guid.to_lowercase())`; powershell → `Script("1")`.
  - `fn apply<S: System>(a:&ActionDef, sys:&mut S, all_users: bool) -> Result<(), SysError>` (`all_users` only matters for `Appx{all_users: Optional}`; `Always` ignores it and passes true)
  - `fn restore<S: System>(a:&ActionDef, sys:&mut S, before:&Snapshot) -> Result<RestoreResult, SysError>`; `enum RestoreResult { Done, ManualReinstall(Option<String>) }` — appx never reinstalls automatically; PowerShell runs `undo`.
  - `fn describe(a:&ActionDef) -> String` — exact human-readable change for the explainer, e.g. `HKCU\Software\Microsoft\Windows\CurrentVersion\ContentDeliveryManager\SystemPaneSuggestionsEnabled = 0 (DWORD)`, `Service "DiagTrack" → Disabled`, `Scheduled task "\Microsoft\Windows\..." → Disabled`, `Remove app "king.com.CandyCrushSaga" (all users)`, `Power plan → High Performance`, `PowerShell: <apply script>`.

- [ ] **Step 1: Failing tests:**
  - `registry_target_matches_after_apply` — apply then observe == `Present(target)`.
  - `registry_restore_absent_deletes_value` — before `Reg(None)`; after restore `reg_get` is `None`.
  - `missing_service_is_not_applicable` (Review Focus 3).
  - `appx_restore_returns_manual_reinstall_hint` — `ManualReinstall(Some("9NBLGGH4..."))`.
  - `appx_optional_respects_all_users_flag` — fake records the `all_users` argument passed.
  - `describe_registry_dword` — exact string above.
- [ ] **Step 2:** `cargo test actions` → FAIL.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** `cargo test actions` → PASS.
- [ ] **Step 5:** Commit `feat: action observe/apply/restore`.

---

### Task 5: The undo journal

**Files:**
- Create: `src-tauri/src/journal.rs`

**Interfaces:**
- Produces:
  - `struct JournalEntry { key: ValueKey, tweak_id: String, action_index: usize, before: Snapshot, after: Snapshot, applied_at: String /* RFC 3339 */ }`
  - `struct Journal` with `fn load(path: PathBuf) -> Result<Journal, JournalError>` (missing file → empty), `fn record(&mut self, e: JournalEntry) -> Result<(), JournalError>`, `fn remove(&mut self, key:&ValueKey) -> Result<(), JournalError>`, `fn for_tweak(&self, id:&str) -> Vec<JournalEntry>`, `fn all(&self) -> Vec<JournalEntry>` (newest first), `fn default_path() -> PathBuf` = `%ProgramData%\Featherweight\journal.json`.
  - File format: `{ "version": 1, "entries": [...] }`. Every mutation writes to `journal.json.tmp` then renames over `journal.json`.
  - `record` on an existing key **keeps the original `before`** and updates `after`/`applied_at` (Review Focus 1).

- [ ] **Step 1: Failing tests** (use `tempfile`): `load_missing_file_is_empty`, `record_persists_and_reloads`, `record_same_key_keeps_original_before`, `remove_deletes_entry`, `all_is_newest_first`, `corrupt_file_errors_without_overwriting` (load returns `JournalError::Corrupt` and the bad file is left in place).
- [ ] **Step 2:** `cargo test journal` → FAIL.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** `cargo test journal` → PASS.
- [ ] **Step 5:** Commit `feat: crash-safe per-value undo journal`.

---

### Task 6: Engine — status, apply with rollback, undo rules

**Files:**
- Create: `src-tauri/src/engine.rs`

**Interfaces:**
- Consumes: Tasks 2–5; `Env` from Task 7 (define `Env` here as `pub struct Env { pub build: u32, pub is_laptop: bool, pub per_user_allowed: bool }` and re-export from `env.rs` in Task 7).
- Produces:
  - `enum TweakStatus { Applied, NotApplied, Partial, NotApplicable(NaReason) }`, `enum NaReason { WrongBuild, AccountMismatch, MissingOnThisPc }`
  - `enum ApplyOutcome { Applied, AlreadyApplied, Failed(String), NotApplicable(NaReason) }`
  - `enum UndoOutcome { Restored, AlreadyOriginal, ChangedSince(String /*describe()*/), ManualReinstall(Option<String>), Failed(String) }`
  - `struct Progress { done: usize, total: usize, current: String /*tweak title*/ }`
  - `struct Engine<S: System> { pub sys: S, pub catalog: Catalog, pub journal: Journal, pub env: Env }`
  - `fn status(&self, id:&str) -> TweakStatus`
  - `fn apply(&mut self, ids:&[String], all_users:&HashSet<String>, on_progress: &mut dyn FnMut(Progress)) -> Vec<(String, ApplyOutcome)>`
  - `fn undo(&mut self, ids:&[String], force: bool) -> Vec<(String, Vec<UndoOutcome>)>`
  - `fn undo_all(&mut self, force: bool) -> Vec<(String, Vec<UndoOutcome>)>` — tweaks in reverse order of `applied_at`.
- Rules (the algorithm the tests pin):
  - Apply per tweak: skip if not applicable or already `Applied`. For each action in order: observe → journal `record(before, after = target)` → `apply`. On error: `restore` the already-applied actions of this tweak in reverse, remove their journal entries, outcome `Failed(msg)`; continue with the next tweak.
  - Undo per journal entry: observe current. `current == after` → restore → `Restored` (or `ManualReinstall`), remove entry. `current == before` → `AlreadyOriginal`, remove entry. Else → `ChangedSince`, keep entry, unless `force` → restore anyway → `Restored`.
  - Startup-app toggles (Task 10) journal under `tweak_id = "startup:<id>"` and are included in `undo_all`.

- [ ] **Step 1: Failing tests** (all on `FakeSystem` + temp journal):
  - `status_applied_partial_not_applied`
  - `status_not_applicable_on_wrong_build` / `..._account_mismatch_for_per_user` / `..._missing_service`
  - `apply_then_undo_restores_original`
  - `apply_twice_keeps_original_before` (Review Focus 1)
  - `apply_failure_rolls_back_earlier_actions_of_same_tweak` — set `fail_next` before action 2; action 1's value is back to original and `Failed(_)` reported; next tweak still applied.
  - `undo_skips_value_changed_since` → `ChangedSince`, entry kept.
  - `undo_force_restores_changed_value`
  - `undo_after_crash_before_apply_reports_already_original` — journal has entry but system still at `before` (Review Focus 2).
  - `undo_all_reverses_in_applied_order`
  - `appx_undo_reports_manual_reinstall`
- [ ] **Step 2:** `cargo test engine` → FAIL.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** `cargo test engine` → PASS.
- [ ] **Step 5:** Commit `feat: apply/undo engine with rollback and change detection`.

---

### Task 7: Environment detection and presets

**Files:**
- Create: `src-tauri/src/env.rs`

**Interfaces:**
- Produces:
  - `fn effective_level(t:&TweakDef, env:&Env) -> Level` — `laptop_level` when `env.is_laptop`, else `level`.
  - `fn resolve_preset<S: System>(engine:&Engine<S>, p: Preset) -> Vec<String>` — tweak ids with `p` in `presets`, effective level `Safe`, status not `NotApplicable`. Startup apps are never included.
  - `fn detect_env() -> Env` (Windows only, `#[cfg(windows)]`): build from `RtlGetVersion`; `is_laptop` from `GetSystemPowerStatus` (`BatteryFlag != 128 && != 255`); `per_user_allowed` = elevated process user SID == owner SID of `explorer.exe` in the active console session (`WTSGetActiveConsoleSessionId`).
  - `fn account_message(env:&Env) -> Option<String>` — when `!per_user_allowed`: `"You're running Featherweight with a different administrator account than the one signed in. Settings that only affect your account are turned off. Sign in as an administrator to use them."`

- [ ] **Step 1: Failing tests:** `laptop_promotes_high_perf_to_advanced` (tweak with `laptop_level = Advanced` excluded from Gaming preset when laptop, included on desktop), `everyday_excludes_advanced_and_gaming_section`, `gaming_includes_everyday`, `preset_excludes_not_applicable`.
- [ ] **Step 2:** `cargo test env` → FAIL.
- [ ] **Step 3:** Implement (pure functions first; `detect_env` behind `cfg(windows)`).
- [ ] **Step 4:** `cargo test env` → PASS.
- [ ] **Step 5:** Commit `feat: environment detection and preset resolution`.

---

### Task 8: `WindowsSystem` — the real implementation

**Files:**
- Create: `src-tauri/src/system/windows.rs`, `src-tauri/tests/windows_integration.rs`

**Interfaces:**
- Produces: `pub struct WindowsSystem;` implementing `System`.
  - Registry: `winreg` (64-bit view, `KEY_WOW64_64KEY`).
  - Services: `windows` crate Service Control Manager (`OpenSCManagerW`, `ChangeServiceConfigW`, `ControlService(SERVICE_CONTROL_STOP)`); map Automatic/AutoDelayed → `Automatic`.
  - Scheduled tasks: `schtasks.exe /Query /TN <path> /FO CSV /V` and `/Change /TN <path> /ENABLE|/DISABLE`.
  - Appx: PowerShell `Get-AppxPackage -AllUsers -Name <package>`; removal `Remove-AppxPackage -AllUsers` / current user; when `all_users`, also `Get-AppxProvisionedPackage -Online | ? DisplayName -eq <package> | Remove-AppxProvisionedPackage -Online`.
  - Power: `powercfg /getactivescheme`, `/duplicatescheme <from> <guid>` when needed, `/setactive <guid>`.
  - PowerShell: `powershell.exe -NoProfile -NonInteractive -ExecutionPolicy Bypass -Command <script>`, `CREATE_NO_WINDOW`.
  - Map `ERROR_ACCESS_DENIED` → `SysError::AccessDenied`.

- [ ] **Step 1: Write `#[ignore]` integration tests** (`#[cfg(windows)]`), touching only `HKCU\Software\FeatherweightTest` and read-only queries: `reg_roundtrip_dword_string_delete`, `reads_start_type_of_a_known_service` (`"EventLog"` → `Some(Automatic)`), `missing_service_is_none` (`"FwNoSuchService"`), `active_power_scheme_is_guid`, `powershell_echo` (`"Write-Output 1"` → `"1"`).
- [ ] **Step 2:** `cargo test --test windows_integration -- --ignored` → FAIL (not implemented).
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** Run the ignored tests **in CI (windows-latest) or Windows Sandbox only**; add `cargo test -- --include-ignored` to `ci.yml`. Expected: PASS.
- [ ] **Step 5:** Commit `feat: Windows system implementation`.

---

### Task 9: Restore point

**Files:**
- Create: `src-tauri/src/restore.rs`

**Interfaces:**
- Produces:
  - `enum RestorePointOutcome { Created, AlreadyMadeThisSession, Throttled, Disabled, Failed(String) }`
  - `struct RestoreGuard { made_this_session: bool }`, `fn ensure<S: System>(g:&mut RestoreGuard, sys:&mut S) -> RestorePointOutcome` — runs `Checkpoint-Computer -Description "Featherweight" -RestorePointType MODIFY_SETTINGS -WarningAction Stop` via `run_powershell`; output containing `1440 minutes` → `Throttled`; `service cannot be started` / restore disabled → `Disabled`.
  - `fn enable<S: System>(sys:&mut S) -> Result<(), SysError>` — `Enable-ComputerRestore -Drive "$env:SystemDrive\"`.

- [ ] **Step 1: Failing tests** with `FakeSystem.ps_responses`: `created_once_per_session`, `throttled_detected`, `disabled_detected` (Review Focus 5).
- [ ] **Step 2:** `cargo test restore` → FAIL.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** `cargo test restore` → PASS.
- [ ] **Step 5:** Commit `feat: restore point before first change`.

---

### Task 10: Startup apps manager

**Files:**
- Create: `src-tauri/src/startup.rs`

**Interfaces:**
- Produces:
  - `struct StartupEntry { id: String, name: String, command: String, location: StartupLocation, enabled: bool, per_user: bool }`; `enum StartupLocation { HkcuRun, HklmRun, HklmRun32, UserFolder, CommonFolder }`
  - `fn list<S: System>(sys:&S, folders:&StartupFolders) -> Result<Vec<StartupEntry>, SysError>` — reads Run keys (`HKCU|HKLM\Software\Microsoft\Windows\CurrentVersion\Run`, `HKLM\Software\WOW6432Node\...\Run`) and both Startup folders; enabled state from `...\Explorer\StartupApproved\{Run,Run32,StartupFolder}` (first byte `0x02`/absent = enabled, `0x03` = disabled).
  - `fn set_enabled<S: System>(engine:&mut Engine<S>, entry:&StartupEntry, enabled: bool) -> Result<(), SysError>` — writes the StartupApproved value (12 bytes: flag byte, 3 zero bytes, 8-byte FILETIME) through the journal under `tweak_id = "startup:<id>"`, so per-entry undo and Undo all work.
  - `StartupFolders { user: PathBuf, common: PathBuf }` (injected so tests use temp dirs).

- [ ] **Step 1: Failing tests:** `lists_run_and_folder_entries`, `approved_03_means_disabled`, `disable_then_undo_all_reenables`, `hklm_entry_is_not_per_user`.
- [ ] **Step 2:** `cargo test startup` → FAIL.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** `cargo test startup` → PASS.
- [ ] **Step 5:** Commit `feat: startup apps manager via StartupApproved`.

---

### Task 11: Cleanup and Home stats

**Files:**
- Create: `src-tauri/src/cleanup.rs`, `src-tauri/src/stats.rs`

**Interfaces:**
- Produces:
  - `struct CleanupTarget { id: String, label: String, paths: Vec<PathBuf>, bytes: u64, needs_service_stop: Option<String> }`
  - `fn default_targets() -> Vec<CleanupTarget>` — ids/labels: `user-temp` "Your temporary files" (`%TEMP%`), `system-temp` "Windows temporary files" (`%SystemRoot%\Temp`), `wu-cache` "Old Windows Update downloads" (`%SystemRoot%\SoftwareDistribution\Download`, stop/start `wuauserv`), `thumbs` "Thumbnail cache" (`%LocalAppData%\Microsoft\Windows\Explorer\thumbcache_*.db`), `delivery-opt` "Delivery Optimization cache" (`%SystemRoot%\ServiceProfiles\NetworkService\AppData\Local\Microsoft\Windows\DeliveryOptimization\Cache`).
  - `fn scan(targets:&mut [CleanupTarget])` — fills `bytes`, ignoring unreadable entries.
  - `fn clean<S: System>(sys:&mut S, t:&CleanupTarget) -> CleanupReport`; `struct CleanupReport { freed_bytes: u64, skipped_files: u32 }` — deletes contents (not the folder itself), skips locked/denied files and counts them, never follows symlinks/junctions out of the target; restarts any stopped service even on error.
  - `struct Stats { startup_enabled: u32, services_running: u32, junk_bytes: u64 }`, `fn collect<S: System>(...) -> Stats`.

- [ ] **Step 1: Failing tests** (temp dirs): `scan_sums_sizes`, `clean_skips_locked_file_and_reports_only_freed` (hold a file open with share-none on Windows / make it read-only dir elsewhere; Review Focus 4), `clean_does_not_follow_junction`, `clean_restarts_stopped_service_on_error`.
- [ ] **Step 2:** `cargo test cleanup` → FAIL.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** `cargo test cleanup` → PASS.
- [ ] **Step 5:** Commit `feat: cleanup scan/clean and home stats`.

---

### Task 12: Tauri commands and the typed frontend API

**Files:**
- Create: `src-tauri/src/commands.rs`, `src/lib/types.ts`, `src/lib/api.ts`, `src/lib/format.ts`, `src/lib/format.test.ts`
- Modify: `src-tauri/src/lib.rs` (managed state `Mutex<Engine<WindowsSystem>>`, `Mutex<RestoreGuard>`, register commands)

**Interfaces:**
- Produces (Rust commands → TS functions in `api.ts`, same names camelCased; TS types mirror Rust with `serde(rename_all = "camelCase")`):
  - `get_overview() -> Overview { build, isLaptop, perUserAllowed, accountMessage: string|null, stats: Stats }`
  - `list_tweaks() -> TweakView[]` where `TweakView { id, title, description, section, level /*effective*/, irreversible, perUser, status, changes: string[] /*describe() per action*/, reinstallHint: string|null, allUsersOptional: boolean }`
  - `preview_preset(preset) -> string[]`
  - `apply_tweaks(ids: string[], allUsers: string[]) -> ApplyReport { results: {id, outcome}[] }` — emits `"apply-progress"` events with `Progress`.
  - `undo_tweaks(ids, force) -> UndoReport`, `undo_all(force) -> UndoReport`
  - `ensure_restore_point() -> RestorePointOutcome`, `enable_system_restore() -> void`
  - `list_startup() -> StartupEntry[]`, `set_startup_enabled(id, enabled) -> void`
  - `scan_cleanup() -> CleanupTarget[]`, `run_cleanup(ids) -> CleanupReport`
  - `format.ts`: `formatBytes(n: number): string` (`6_657_199_308` → `"6.2 GB"`, `0` → `"0 B"`, `1536` → `"1.5 KB"`).

- [ ] **Step 1: Failing tests:** `format.test.ts` for the three `formatBytes` cases; Rust `commands_serialize_camel_case` (serialize a `TweakView` and assert key `reinstallHint`).
- [ ] **Step 2:** `npm test` and `cargo test commands` → FAIL.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** Both → PASS; `npm run check` → 0 errors.
- [ ] **Step 5:** Commit `feat: Tauri command layer and typed API`.

---

### Task 13: The v1 tweak catalog

**Files:**
- Create: `tweaks/*.toml` (one per id below), test in `src-tauri/src/catalog/mod.rs`

**Interfaces:**
- Produces these tweak ids (section, level, presets). Exact registry paths, service names, task paths, package names and power-scheme GUIDs are the builder's to verify on current Windows builds (spec §11); every description is plain words, ≤ 2 sentences.

| id | section | level | presets | notes |
|---|---|---|---|---|
| `remove-junk-apps` | Remove | safe | everyday, gaming | irreversible; `Appx{all_users: Always}` per app: Candy Crush (all), Solitaire, Clipchamp, Bing News, Bing Weather, Get Help, Get Started/Tips, Feedback Hub, Microsoft To Do, People, Maps, Mixed Reality Portal, 3D Viewer |
| `block-suggested-app-installs` | Remove | safe | everyday, gaming | ContentDeliveryManager `SilentInstalledAppsEnabled`, `OemPreInstalledAppsEnabled`, `PreInstalledAppsEnabled` = 0 |
| `disable-copilot` | Remove | safe | everyday, gaming | policy + taskbar button |
| `disable-recall` | Remove | safe | everyday, gaming | `min_build` 26100 |
| `disable-widgets` | Remove | safe | everyday, gaming | |
| `no-bing-in-start` | Remove | safe | everyday, gaming | |
| `no-start-ads` | Remove | safe | everyday, gaming | Start/Settings suggestions |
| `no-lockscreen-ads` | Remove | safe | everyday, gaming | Spotlight tips/"fun facts" |
| `no-explorer-ads` | Remove | safe | everyday, gaming | sync provider notifications |
| `remove-onedrive` | Remove | advanced | — | irreversible; `all_users: Optional` |
| `remove-xbox-apps` | Remove | advanced | — | irreversible; `all_users: Optional`; never in Gaming |
| `disable-telemetry-services` | Background | safe | everyday, gaming | DiagTrack, dmwappushservice → Disabled; AllowTelemetry policy |
| `disable-telemetry-tasks` | Background | safe | everyday, gaming | CEIP / Compatibility Appraiser tasks |
| `disable-unused-services` | Background | safe | everyday, gaming | Fax, MapsBroker, RetailDemo, WMPNetworkSvc |
| `disable-print-spooler` | Background | advanced | — | description warns printing stops |
| `disable-bluetooth-support` | Background | advanced | — | description warns Bluetooth stops |
| `power-high-performance` | Gaming | safe, `laptop_level = advanced` | gaming | |
| `power-ultimate-performance` | Gaming | advanced | — | `duplicate_from` the Ultimate template; `exclusive_with = "power-high-performance"` (and vice versa) |

Both power tweaks write `ValueKey::PowerScheme`. Add `exclusive_with: Option<String>` to `TweakDef`; `validate` allows an overlap **only** between two tweaks that name each other in `exclusive_with`, and rejects one-sided exclusivity. The UI turns the other one off when one is switched on.
| `game-mode-on` | Gaming | safe | gaming | |
| `game-bar-recording-off` | Gaming | safe | gaming | |
| `mouse-accel-off` | Gaming | safe | gaming | |
| `network-throttling-off` | Gaming | advanced | — | |
| `system-responsiveness` | Gaming | advanced | — | |

- [ ] **Step 1: Failing tests:** `bundled_catalog_validates` (`validate(&Catalog::bundled())` is empty); `bundled_catalog_has_v1_ids` (every id in the table exists); `xbox_never_in_gaming_preset`; `irreversible_flags` (`remove-junk-apps`, `remove-onedrive`, `remove-xbox-apps` are irreversible, nothing else is); `exclusive_with_is_symmetric`.
- [ ] **Step 2:** `cargo test catalog` → FAIL.
- [ ] **Step 3:** Add `exclusive_with` to `TweakDef` + validation; write the TOML files.
- [ ] **Step 4:** `cargo test` → PASS. Then in **Windows Sandbox**, apply Everyday and Gaming, reboot-free sanity check each tweak's effect, run Undo all, confirm values are back (record results in the PR description).
- [ ] **Step 5:** Commit `feat: v1 tweak catalog`.

---

### Task 14: Design system and app shell

**Files:**
- Create: `src/styles/tokens.css`, `src/styles/base.css`, `src/components/Sidebar.svelte`, `AccountBanner.svelte`, `LevelPill.svelte`, `Logo.svelte` (placeholder feather mark until Task 18), `src/lib/state.svelte.ts`
- Modify: `src/App.svelte`

**Interfaces:**
- Produces: CSS tokens `--bg #0f1115`, `--surface #15181e`, `--surface-2 #171a21`, `--border #242833`, `--text #e7e9ee`, `--muted #8a90a0`, `--accent #38bdf8`, `--accent-2 #818cf8`, `--safe-bg #123524`, `--safe #6ee7a8`, `--adv-bg #3a2a12`, `--adv #fbbf6b`, `--radius 8px`, spacing scale 4/8/12/16/24/32; font `"Segoe UI Variable Text", "Segoe UI", system-ui`.
- `state.svelte.ts`: `page = $state<'home'|'remove'|'background'|'gaming'|'cleanup'>('home')`, `tweaks = $state<TweakView[]>([])`, `overview = $state<Overview|null>(null)`, `refresh(): Promise<void>`.
- Sidebar items (Lucide): Home `House`, Remove `Trash2`, Background `Activity`, Gaming `Gamepad2`, Cleanup `Sparkles`; "Undo all" `RotateCcw` pinned bottom.
- Rule: the accent gradient is used **only** on the primary action button and the logo.

- [ ] **Step 1:** Implement tokens, shell layout (sidebar 200px + content), Sidebar with active state, AccountBanner showing `overview.accountMessage` when present.
- [ ] **Step 2:** Verify: `npm run check` → 0 errors; `npm run tauri dev`, screenshot the shell; compare against `mockups/layout-chosen.html` for structure.
- [ ] **Step 3:** Commit `feat: design tokens and app shell`.

---

### Task 15: Home page

**Files:**
- Create: `src/pages/Home.svelte`, `src/components/StatTile.svelte`, `PresetCard.svelte`, `ConfirmDialog.svelte`, `ResultSummary.svelte`, `src/lib/applyFlow.ts`, `src/lib/applyFlow.test.ts`

**Interfaces:**
- Consumes: `api.ts`, `state.svelte.ts`.
- Produces: `applyFlow.ts`:
  - `planApply(ids: string[], tweaks: TweakView[]): { reversible: string[]; irreversible: string[] }`
  - `runApply(ids, allUsers, deps: { ensureRestorePoint, applyTweaks, confirmIrreversible: (ids) => Promise<boolean>, confirmRestoreIssue: (o) => Promise<boolean> }): Promise<ApplyReport | null>` — order: restore point → if outcome is `Throttled`/`Disabled`/`Failed`, ask (`Disabled` offers "Turn on System Restore") → if any irreversible ids, `confirmIrreversible` (cancel removes only those ids) → apply.
- Home layout: three StatTiles (Startup apps / Background services / Junk files with `formatBytes`), "Pick a preset" with two PresetCards (Everyday: "Faster, quieter PC. Only safe changes." / Gaming: "Everyday plus FPS and input-lag tweaks."), restore-point note, primary button `Apply Everyday (N changes)`, secondary `Review list` (opens a checklist of the preset's tweaks with switches).
- ConfirmDialog copy for irreversible: title "These can't be undone with one click", list app names and cleanup sizes, buttons "Remove and clean" / "Skip these".

- [ ] **Step 1: Failing tests** `applyFlow.test.ts`: `irreversible_requires_confirmation`, `cancel_irreversible_applies_only_reversible`, `throttled_restore_point_asks_then_continues`, `disabled_restore_point_offers_enable`.
- [ ] **Step 2:** `npm test` → FAIL.
- [ ] **Step 3:** Implement `applyFlow.ts` and the components.
- [ ] **Step 4:** `npm test` → PASS; `npm run check` → 0 errors; screenshot Home in dev.
- [ ] **Step 5:** Commit `feat: home page with presets and safe apply flow`.

---

### Task 16: Section pages, explainer and undo

**Files:**
- Create: `src/pages/Section.svelte`, `src/components/TweakRow.svelte`, `ExplainerPanel.svelte`, `src/lib/selection.ts`, `src/lib/selection.test.ts`

**Interfaces:**
- Produces: `selection.ts`:
  - `pendingChanges(tweaks: TweakView[], toggled: Record<string, boolean>): { apply: string[]; undo: string[] }` — toggled on & not Applied → apply; toggled off & Applied/Partial → undo.
  - `defaultToggles(tweaks: TweakView[]): Record<string, boolean>` — Applied/Partial → true, else false (ADVANCED never pre-selected).
- Section page (used for Remove and Gaming; Background composes it in Task 17): list header "N items · M selected" with SAFE/ADVANCED legend; TweakRow = switch + title + LevelPill; selecting a row shows ExplainerPanel: title, description, "What it changes" (monospace `changes` lines), status, for `allUsersOptional` a checkbox "Also remove for all users", "Undo this one" (when Applied/Partial), and for `ChangedSince` results a "Restore anyway" button. Bottom primary button "Apply N changes" runs `runApply` for `apply` ids and `undoTweaks` for `undo` ids. NotApplicable tweaks are hidden.
- "Undo all" (sidebar): confirm dialog "Put everything Featherweight changed back?" → `undoAll(false)` → ResultSummary listing Restored / Already original / Changed since (with Restore anyway) / Reinstall manually (with Store link).

- [ ] **Step 1: Failing tests:** `toggle_on_unapplied_goes_to_apply`, `toggle_off_applied_goes_to_undo`, `advanced_not_preselected`.
- [ ] **Step 2:** `npm test` → FAIL.
- [ ] **Step 3:** Implement.
- [ ] **Step 4:** `npm test` → PASS; dev screenshot of Remove page with the explainer open.
- [ ] **Step 5:** Commit `feat: section pages with explainer and undo`.

---

### Task 17: Background (with startup apps) and Cleanup pages

**Files:**
- Create: `src/pages/Background.svelte`, `src/pages/Cleanup.svelte`

**Interfaces:**
- Consumes: `Section.svelte`, `listStartup`, `setStartupEnabled`, `scanCleanup`, `runCleanup`.
- Background: two tabs — "Services & telemetry" (Section component, section=Background) and "Startup apps" (rows: switch, name, command in muted monospace, "All users" tag for HKLM; toggles apply immediately; per-user entries disabled when `!perUserAllowed`).
- Cleanup: scans on open with a skeleton state; each target row with checkbox and size; total; "Clean N GB" → irreversible confirm → `runCleanup` → result "Freed X. Y files were in use and were skipped."

- [ ] **Step 1:** Implement both pages.
- [ ] **Step 2:** Verify in dev: `npm run check` → 0 errors; screenshot both pages.
- [ ] **Step 3:** Commit `feat: background and cleanup pages`.

---

### Task 18: Logo (owner picks)

**Files:**
- Create: `assets/logo/featherweight.svg`, `assets/logo/readme-header.svg`, `assets/logo/social-preview.png` (1280×640), `src-tauri/icons/*` (generated)
- Modify: `src/components/Logo.svelte`

- [ ] **Step 1:** Draw 3–4 distinct feather-mark SVG options (geometric, single-stroke, monogram-with-feather, negative-space) that read at 16 px; show them to the owner in the brainstorm browser companion, each at 16/32/256 px on dark and light.
- [ ] **Step 2:** **Owner picks one.** Refine it.
- [ ] **Step 3:** Export a 1024 px PNG; run `npm run tauri icon assets/logo/featherweight-1024.png`; update `Logo.svelte`; build README header and social preview.
- [ ] **Step 4:** Verify the taskbar/titlebar icon in `npm run tauri dev`.
- [ ] **Step 5:** Commit `feat: Featherweight logo and app icons`.

---

### Task 19: Polish pass, README and release

**Files:**
- Create: `README.md`, `.github/workflows/release.yml`
- Modify: any UI files found lacking

- [ ] **Step 1:** Screenshot every page at 1100×720 and review against the spec's anti-slop list: no emoji, accent only on primary action + logo, consistent 4/8 spacing, aligned baselines, real copy (no lorem/filler), hover/focus states, keyboard focus visible, subtle motion only on apply/progress. Fix findings; repeat until clean. **Owner reviews the final screenshots.**
- [ ] **Step 2:** README: header logo; one-line pitch; GIF of Home → Apply Everyday → summary; "What it changes" (sections); "What it will never do" (the anti-goals verbatim); "How to undo"; SmartScreen note ("unsigned for now — here's the source; click More info → Run anyway"); build-from-source steps; MIT.
- [ ] **Step 3:** `release.yml`: on tag `v*`, `windows-latest`, `npm ci`, `npm run tauri build`, upload the NSIS installer and the bare `featherweight.exe` (portable) to the GitHub Release.
- [ ] **Step 4:** Verify: `npm run tauri build` produces both artifacts locally; portable exe launches.
- [ ] **Step 5:** Commit `docs: README; ci: release workflow`, then tag `v0.1.0` **only when the owner says to**.
