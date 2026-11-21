# Featherweight — Design

*2026-11-21 · Windows optimizer and debloater · open source on GitHub*

## 1. Intent

Featherweight makes a Windows PC faster and less annoying. It removes the things people dislike (preinstalled junk, ads, Copilot/Recall), calms background activity (startup apps, telemetry, unneeded services), applies gaming tweaks, and cleans junk files.

It is also meant to be a GitHub project that earns stars. Debloaters are a crowded space (WinUtil, Win11Debloat, O&O ShutUp10, Talon), so Featherweight stands out on **trust and friendliness**:

- One-click presets for people who just want a faster PC.
- Every tweak explained in plain words, with the exact change visible.
- A restore point before the first change, per-tweak undo, and "Undo all".
- A modern, good-looking app that screenshots well.

**Who it's for:** both gamers chasing FPS and latency, and normal people who want a slow PC to feel fast without risk.

## 2. Goals, non-goals, anti-goals

**Goals**
- Presets: **Everyday** (only SAFE tweaks) and **Gaming** (Everyday plus gaming tweaks).
- Per-section control for power users, with SAFE / ADVANCED labels.
- Every change can be undone individually or all at once. The only exception is app removal, which is clearly marked "can be reinstalled, not instantly undone".
- Show real numbers: startup apps, background services, junk size, and space freed.

**Non-goals (v1)**
- Driver updates, BIOS or overclocking settings, GPU control-panel tweaks.
- A command-line or scripted mode (maybe later).
- Auto-updating the app itself, or any network features beyond what tweaks themselves need.
- macOS or Linux.

**Anti-goals (the project fails if it does these, even if it works)**
- Turns off Windows Defender or blocks Windows Update entirely.
- Ships "RAM cleaners", "registry cleaners" or placebo "boosters".
- Changes anything silently, or anything it can't explain in the UI.
- Leaves a PC in a state the user can't get back from.
- Collects any telemetry of its own, or bundles anything.

## 3. Constraints

- **OS:** Windows 10 22H2 and Windows 11 (23H2 and newer), x64. Every tweak declares which builds it applies to; tweaks that don't apply are hidden.
- **Needs admin.** The app asks once at launch via its manifest (`requireAdministrator`).
- **Unsigned at first.** Expect SmartScreen warnings. The README explains this and links to the source.
- **Tested only in Windows Sandbox or a VM** during development. Tweaks are never applied to the developer's daily PC by tooling.
- **Repo hygiene:** commits, PRs and README carry no AI-assistant attribution.

## 4. Approaches considered

| Approach | Trade-off |
|---|---|
| A. PowerShell script + simple window (WinUtil style) | Easiest to audit and one-line install, but looks dated and is awkward for non-tech users. |
| B. C# .NET + WPF native app | Native Windows 11 look and simple `.exe`, but heavier runtime and less modern UI flexibility. |
| **C. Tauri 2 (Rust) + web UI — chosen** | Small binary (~10 MB), the most polished UI possible, and Rust is a solid, safe base for system changes. More moving parts than A. |

The owner chose **C**.

## 5. What the owner decided

- **Name:** Featherweight.
- **Stack:** Tauri 2 with a Rust backend.
- **Layout: Dashboard (A) + Checklist with explainer (C)**, as mocked up in [mockups/layout-chosen.html](mockups/layout-chosen.html) (the three original options are in [mockups/layout-options.html](mockups/layout-options.html)). The mockups set layout and structure; visual polish goes beyond them:
  - A left **sidebar**: Home, Remove, Background, Gaming, Cleanup, and "Undo all" pinned at the bottom.
  - **Home**: three stat tiles (startup apps, background services, junk files), preset cards (Everyday / Gaming), a note that a restore point is made first, and an **Apply [preset] (N tweaks)** button with a **Review list** button.
  - **Section pages** (Remove, Background, Gaming, Cleanup): a list of switches, each with a SAFE or ADVANCED pill; a right-hand **explainer panel** for the selected tweak showing a plain description, the exact change (e.g. registry path and value), its status, and **Undo this one**. There's an "Apply N changes" button at the bottom of the list.
- **Dark theme** with a sky-blue to violet accent gradient (as in the mockups).
- **A real logo.** A custom mark built around a feather (light, fast), used for the app icon, taskbar, installer, README header and GitHub social preview. Logo options get mocked up and the owner picks one before it's finalized.
- **It must look designed, not like "AI slop".** Concretely: no generic purple-gradient-everything, no emoji as icons (the emoji in the mockups are placeholders; real builds use a consistent icon set such as Lucide or custom SVGs), no default-looking cards stacked everywhere, no filler copy. Deliberate typography (Segoe UI Variable or a chosen typeface), consistent spacing, restrained use of the accent, subtle motion only where it helps (applying, progress). The finished screens are reviewed against screenshots and polished before release.
- **v1 scope** (section 6).

## 6. v1 scope

**Remove**
- Preinstalled apps (Candy Crush, Clipchamp, News, Solitaire, etc.) as a checklist — SAFE.
- Copilot, Recall, Widgets, Bing results in Start search — SAFE.
- Ads and "suggestions" in Start, Settings, lock screen, File Explorer — SAFE.
- OneDrive uninstall — ADVANCED. Xbox apps — ADVANCED (off by default, and never in the Gaming preset).

**Background**
- Startup apps manager: lists everything that launches at boot (Run keys in HKCU/HKLM and the Startup folders). Disabling uses the same `StartupApproved` mechanism as Task Manager, so it's fully reversible and Task Manager agrees.
- Telemetry: DiagTrack and dmwappushservice services, telemetry policy, related scheduled tasks — SAFE.
- Rarely needed services (Fax, Maps updates, Retail Demo, etc.) — SAFE; services some people need (Print Spooler, Bluetooth support, etc.) — ADVANCED with a warning.

**Gaming**
- High Performance power plan — SAFE on desktops. On laptops (a battery is detected) it is ADVANCED and off by default.
- Ultimate Performance power plan — ADVANCED.
- Game Mode on; Game Bar background recording off — SAFE.
- Mouse acceleration ("Enhance pointer precision") off — SAFE, included in the Gaming preset, and listed in "Review list" so anyone who likes it can drop it.
- Network throttling index and system responsiveness tweaks — ADVANCED.

**Cleanup**
- User and system temp folders, Windows Update download cache, thumbnail cache, Delivery Optimization cache. Shows the size of each before cleaning and the total freed after. Cleanup is not undoable, and the UI says so.

**Presets**
- **Everyday:** all SAFE items in Remove, Background (except startup apps, which the user picks), and Cleanup.
- **Gaming:** Everyday plus all SAFE gaming items.
- "Review list" shows exactly which tweaks a preset will apply, with switches to drop any before applying.
- **Things that can't be undone get their own confirmation**, even inside a preset. Before applying, one confirm screen lists the apps to be removed and the cleanup sizes, and says plainly these can't be undone with one click.
- **App removal scope:** for the SAFE junk list (Candy Crush, Solitaire, Clipchamp, etc.), the preset removes the app for the current user *and* the provisioned package, and turns off Windows' "silently install suggested apps" setting, so the junk doesn't come back for new accounts. ADVANCED removals (OneDrive, Xbox) are current-user only unless the user ticks "also for all users".

## 7. How it works

**Tweak definitions are data.** Each tweak is a small declarative file (TOML) bundled into the app:
`id`, `title`, `description` (plain words), `section`, `level` (safe/advanced), `presets`, `min_build`/`max_build`, and a list of **actions**.

**Action types** (each knows how to *check*, *apply* and *undo* itself):
- `registry` — set or delete a value. Undo restores the recorded previous value (or deletes it if it didn't exist).
- `service` — change startup type (and stop it). Undo restores the recorded startup type.
- `scheduled_task` — enable or disable. Undo restores the previous state.
- `appx` — remove a package for the current user, optionally the provisioned package. Undo is "reinstall": a Store link or `winget` ID, shown as best effort.
- `power_plan` — activate or duplicate a scheme. Undo reactivates the previous one.
- `powershell` — escape hatch for anything else. Must ship with an explicit undo script, and is shown in the explainer panel.

A tweak's status (applied / not applied / partly applied) is computed by running each action's *check* against the live system, so the app is correct even if Windows or the user changed something.

**Undo journal.** Before any action runs, its previous state is written to a journal in `%ProgramData%\Featherweight\` (JSON). Per-tweak undo and "Undo all" replay the journal in reverse. The journal survives app reinstalls.

**Undo rules.**
- The journal stores the previous state **per value** (each registry value, service, task), tagged with the tweak that changed it.
- Tweak definitions must not touch the same value as another tweak. A build-time check fails CI if two tweaks overlap, so "undo the earlier one only" never happens.
- On undo, Featherweight restores the old value **only if the current value is still what Featherweight set**. If Windows Update, the user or another tool changed it since, that item is skipped and reported ("changed since we touched it — left alone"), with a "restore anyway" button.

**Which account gets changed.** v1 targets the common case: the person at the keyboard is an administrator on their own PC. At launch the app compares the elevated account with the signed-in desktop user. If they differ (a standard user elevated with someone else's admin password), it shows a clear message and only allows machine-wide tweaks; per-user tweaks are disabled with an explanation. Proper support for standard-user accounts (writing to the signed-in user's registry hive) is a later version.

**Restore point.** Made before the first change of a session via System Restore. If System Restore is off, the app offers to turn it on. If Windows refuses because one was made in the last 24 hours, the app says so and continues (the journal is still the main safety net).

**Frontend ↔ backend.** The Svelte UI calls Rust commands through Tauri: list tweaks with status, apply a set, undo a set, undo all, get Home stats, scan and run cleanup. Long operations stream progress events to the UI.

**Errors.** If an action fails, that tweak is rolled back to its previous state and marked failed with the reason. Other tweaks continue. A summary shows what succeeded and what didn't.

## 8. Shipping

- GitHub Releases: an installer plus a portable `.exe`.
- README: a GIF of the Home screen and one-click optimize at the top; a "what it never does" section; how to undo; the SmartScreen explanation.
- License: MIT.
- Launch posts: r/Windows11, r/pcgaming, r/opensource, r/software.

## 9. Sign-off

The owner is available throughout. They pick the logo from mockups and review screenshots of the finished screens before release.

## 10. Calls made by the designer (owner to check)

- Elevation: v1 supports admin-at-the-keyboard only; mismatched accounts get machine-wide tweaks only.
- Undo: per-value journal, no overlapping tweaks, skip-and-report if a value changed since.
- Gaming labels: High Performance SAFE on desktops, ADVANCED on laptops; Ultimate ADVANCED; mouse acceleration off is in the Gaming preset.
- Junk app removal in presets also removes provisioned packages; irreversible steps get a separate confirmation.
- Svelte 5 + TypeScript, MIT license, Windows 10 22H2+ / 11 23H2+ x64.

## 11. Left to the builder

- Frontend framework details: Svelte 5 + TypeScript + Vite is the default call; component structure is up to the builder.
- Exact registry paths, service names and package IDs for each tweak, verified against current Windows builds.
- Folder layout, test structure, icon and exact colours.
- Whether tweak files are TOML or JSON, as long as they stay human-readable and reviewable in PRs.
