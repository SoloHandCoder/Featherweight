# Sandbox checklist: v1 tweak catalog

Run this before every release that changes `tweaks/`. Never run it on your
everyday PC: do it in Windows Sandbox (fast, thrown away on close) or a VM.

Windows Sandbox always runs the **same Windows build as the host**. To cover
both supported OSes, run it once on a Windows 11 23H2+ host and once in a
Windows 10 22H2 VM (Hyper-V "Quick Create" or any VM). Sandbox also ships
with very few Store apps, so the app-removal checks (`remove-junk-apps`,
`remove-onedrive`, `remove-xbox-apps`) are best done in a VM made from a
normal Windows install image.

Windows Sandbox needs Windows **Pro, Education or Enterprise**. On Windows
**Home** there is no Sandbox: use a VirtualBox or VMware VM instead (same
checklist, take a snapshot before you start). Windows Update checks may not
work inside Sandbox, so do the "updates still work" checks in a VM.

Write down for each step: build number (`winver`), result (OK / wrong /
"not on this PC"), and anything odd.

## 0. Set up

1. Build the portable exe on the host: `npm run tauri build` in the repo.
   The exe is `src-tauri\target\release\featherweight.exe`.
2. Turn on Windows Sandbox (once, needs Pro/Enterprise/Education):
   Start > "Turn Windows features on or off" > tick **Windows Sandbox** >
   OK > restart.
3. Start > **Windows Sandbox**.
4. Copy `featherweight.exe` on the host (Ctrl+C) and paste it on the
   Sandbox desktop (Ctrl+V). (Or start Sandbox from a `.wsb` file with a
   read-only `<MappedFolder>` pointing at `src-tauri\target\release`.)
5. In the Sandbox, open these and keep them open:
   - `regedit` (Registry Editor)
   - `services.msc` (Services)
   - `taskschd.msc` (Task Scheduler)
   - an admin PowerShell (for `powercfg` and `Get-AppxPackage`)
6. Before touching anything, record the "before" state of every item below
   (a screenshot of each regedit key is enough). Handy dumps, in the admin
   PowerShell:
   ```powershell
   $cdm = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\ContentDeliveryManager'
   Get-ItemProperty $cdm | Format-List *Enabled*
   Get-ItemProperty 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced' |
     Format-List ShowCopilotButton, Start_IrisRecommendations, ShowSyncProviderNotifications
   Get-Service DiagTrack, dmwappushservice, Fax, MapsBroker, RetailDemo, WMPNetworkSvc, Spooler, bthserv |
     Format-Table Name, StartType, Status
   powercfg /getactivescheme; powercfg /list
   ```
7. Run `featherweight.exe` (accept the admin prompt). Note the Home numbers.

"Missing" below means the value/key does not exist; that is the normal
"before" state for most policy values.

## 1. Per-tweak checks

For each tweak: look at the before state, switch it on and press Apply,
check the "after" state, then press **Undo this one** and check it is back
to the before state. Then re-apply it (for the "Undo all" pass in section 2).

`CDM` = `HKCU\Software\Microsoft\Windows\CurrentVersion\ContentDeliveryManager`
`ADV` = `HKCU\Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced`

### Remove

| id | after Apply | after Undo |
|---|---|---|
| `remove-junk-apps` | Confirm screen lists the apps. In PowerShell, `Get-AppxPackage -AllUsers \| Where-Object Name -match 'CandyCrush\|Solitaire\|Clipchamp\|BingNews\|BingWeather\|Getstarted\|FeedbackHub\|Todos\|People\|WindowsMaps\|3DViewer' \| Select-Object Name` returns nothing, and `Get-AppxProvisionedPackage -Online \| Where-Object DisplayName -match '<same pattern>'` returns nothing. Start menu no longer shows them. `Microsoft.GetHelp` and `Microsoft.MixedReality.Portal` are still installed (they are deliberately kept). | Nothing comes back (expected). The UI shows "reinstall from the Store" with a link for each app that had one; click two of them and check the right Store page opens. |
| `block-suggested-app-installs` | `CDM` `SilentInstalledAppsEnabled`, `OemPreInstalledAppsEnabled`, `PreInstalledAppsEnabled` = 0 | each back to its before value (or missing) |
| `disable-copilot` | `HKCU\Software\Policies\Microsoft\Windows\WindowsCopilot\TurnOffWindowsCopilot` = 1; `ADV\ShowCopilotButton` = 0; Copilot button gone from the taskbar (after Explorer restart) | policy value missing again; `ShowCopilotButton` back |
| `disable-recall` | Only on build 26100+ (otherwise the tweak shows "not for this Windows version" - check that too). `HKLM\SOFTWARE\Policies\Microsoft\Windows\WindowsAI\DisableAIDataAnalysis` = 1 and the same under `HKCU\Software\Policies\...` | both missing again |
| `disable-widgets` | `HKLM\SOFTWARE\Policies\Microsoft\Dsh\AllowNewsAndInterests` = 0 and `HKLM\SOFTWARE\Policies\Microsoft\Windows\Windows Feeds\EnableFeeds` = 0; Widgets (Win 11) / News and interests (Win 10) gone from the taskbar after sign-out or Explorer restart; Settings > Taskbar shows the Widgets switch as managed | both missing; Widgets back after sign-out |
| `no-bing-in-start` | `HKCU\Software\Policies\Microsoft\Windows\Explorer\DisableSearchBoxSuggestions` = 1; `HKCU\Software\Microsoft\Windows\CurrentVersion\Search\BingSearchEnabled` = 0. After sign-out/in, typing "weather" in Start shows no web results | both back; web results return after sign-out/in |
| `no-start-ads` | `CDM` `SystemPaneSuggestionsEnabled`, `SubscribedContent-338388Enabled`, `-338389Enabled`, `-338393Enabled`, `-353694Enabled`, `-353696Enabled` = 0; `ADV\Start_IrisRecommendations` = 0. Win 11: Settings > Personalization > Start shows "Show recommendations for tips..." off. Settings > Privacy > General shows "suggested content" off | all back |
| `no-lockscreen-ads` | `CDM` `RotatingLockScreenOverlayEnabled` = 0 and `SubscribedContent-338387Enabled` = 0; Settings > Personalization > Lock screen shows "Get fun facts, tips..." unticked | both back |
| `no-explorer-ads` | `ADV\ShowSyncProviderNotifications` = 0; File Explorer Options > View shows "Show sync provider notifications" unticked | back |
| `remove-onedrive` | Advanced, not in any preset. Test twice: once with the normal per-user install (`%LOCALAPPDATA%\Microsoft\OneDrive\OneDrive.exe`), and once in a VM where OneDrive is installed per-machine (`OneDriveSetup.exe /allusers`, giving `Program Files\Microsoft OneDrive\OneDrive.exe`). After: OneDrive gone from the tray and from Settings > Apps, both exe paths gone; UI shows "applied" within a few seconds (it must not sit until a 2-minute timeout). Files in `%USERPROFILE%\OneDrive` are still there. With mismatched accounts (elevate with a different admin's password) the tweak shows as unavailable | **Check carefully:** Undo returns promptly (not after ~2 minutes) even though OneDrive starts running. OneDrive is installed again for this user (exe back in `%LOCALAPPDATA%`, listed in Settings > Apps) and the tweak shows "not applied"; you have to sign in to OneDrive again. Note whether setup complained about running as administrator, and that a per-machine install comes back as per-user. |
| `remove-xbox-apps` | Advanced, not in any preset. Try both with and without "also for all users". `Get-AppxPackage \| Where-Object Name -match '^Microsoft\.(GamingApp\|XboxApp)$'` returns nothing; with "all users" ticked, `Get-AppxPackage -AllUsers \| Where-Object Name -match '<same pattern>'` and `Get-AppxProvisionedPackage -Online \| Where-Object DisplayName -match '<same pattern>'` return nothing too. `Microsoft.XboxIdentityProvider`, `Microsoft.Xbox.TCUI`, `Microsoft.XboxGamingOverlay` are still installed; Win+G still opens Game Bar | Nothing comes back (expected); reinstall links open the Xbox / Xbox Console Companion Store pages |

### Background

| id | after Apply | after Undo |
|---|---|---|
| `disable-telemetry-services` | services.msc: "Connected User Experiences and Telemetry" (DiagTrack) and "Device Management Wireless Application Protocol (WAP) Push message Routing Service" (dmwappushservice) are **Disabled** and **stopped**. `HKLM\SOFTWARE\Policies\Microsoft\Windows\DataCollection\AllowTelemetry` = 1. Settings > Windows Update > Check for updates still works | both services back to their before start type (usually Automatic / Manual); policy value missing |
| `disable-telemetry-tasks` | Task Scheduler > Microsoft > Windows > Customer Experience Improvement Program: `Consolidator`, `UsbCeip` **Disabled**; Application Experience: `Microsoft Compatibility Appraiser` **Disabled**. If the tweak says "not on this PC", note which task is missing on this build | all three back to Ready |
| `disable-unused-services` | Fax, Downloaded Maps Manager (MapsBroker), Retail Demo Service, Windows Media Player Network Sharing Service are **Disabled**. If the tweak says "not on this PC", note which service is missing (`Get-Service Fax, MapsBroker, RetailDemo, WMPNetworkSvc`) | each back to its before start type. MapsBroker is normally "Automatic (Delayed Start)" - check it comes back as delayed, not plain Automatic |
| `disable-print-spooler` | Print Spooler **Disabled**, stopped; printing to "Microsoft Print to PDF" fails | back to Automatic; restart the service (or reboot) and Print to PDF works |
| `disable-bluetooth-support` | Bluetooth Support Service (bthserv) **Disabled**, stopped | back to Manual |

### Gaming

| id | after Apply | after Undo |
|---|---|---|
| `power-high-performance` | `powercfg /getactivescheme` shows `8c5e7fda-e8bf-4a96-9a85-cf2dd2b47a5c (High Performance)`. If the PC has no High Performance plan the tweak should fail cleanly with "not found" and change nothing | active scheme back to the before GUID (usually Balanced `381b4222-f694-41f0-9685-ff5bb260df2e`) |
| `power-ultimate-performance` | Switching it on turns `power-high-performance` off in the UI. `powercfg /list` now has `fe1a7e00-5eed-4f00-9c00-0000000000f1 (Ultimate Performance)` and it is active. Apply it a second time: no second copy appears | active scheme back to the before GUID (the copied plan stays in the list; that's expected) |
| `game-mode-on` | `HKCU\Software\Microsoft\GameBar\AutoGameModeEnabled` = 1; Settings > Gaming > Game Mode shows On | back (usually missing) |
| `game-bar-recording-off` | `HKCU\Software\Microsoft\Windows\CurrentVersion\GameDVR` `AppCaptureEnabled` = 0, `HistoricalCaptureEnabled` = 0; `HKCU\System\GameConfigStore\GameDVR_Enabled` = 0. Settings > Gaming > Captures shows recording off | all back |
| `mouse-accel-off` | `HKCU\Control Panel\Mouse` `MouseSpeed`, `MouseThreshold1`, `MouseThreshold2` = "0", "0", "0" (strings). After sign-out/in, Control Panel > Mouse > Pointer Options shows "Enhance pointer precision" unticked | back to "1", "6", "10" (or whatever they were) |
| `network-throttling-off` | `HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Multimedia\SystemProfile\NetworkThrottlingIndex` = 0xffffffff (4294967295) | back to 10 (0xa) |
| `system-responsiveness` | same key, `SystemResponsiveness` = 10 | back to 20 (0x14) |

## 2. Presets and Undo all

Start a fresh Sandbox (close and reopen it) so everything is at defaults.

1. Record the before state again (step 0.6).
2. Home > **Everyday** > Review list. Check it lists exactly the SAFE
   Remove and Background tweaks (`remove-junk-apps` through
   `no-explorer-ads`, `disable-telemetry-services`, `disable-telemetry-tasks`,
   `disable-unused-services`) and **no** Gaming tweaks, OneDrive, Xbox,
   Print Spooler or Bluetooth. The confirm screen lists the apps to be
   removed and says they can't be undone with one click.
3. Apply. Every tweak shows "applied" (or "not on this PC" with a reason).
   Spot-check five values from section 1.
4. Home > **Gaming** > Review list. It lists everything in Everyday plus
   `power-high-performance` (desktop; on a laptop it must be missing and
   ADVANCED), `game-mode-on`, `game-bar-recording-off`, `mouse-accel-off`.
   It must not include `remove-xbox-apps`, `power-ultimate-performance`,
   `network-throttling-off` or `system-responsiveness`.
5. Apply. Spot-check the power plan and the mouse values.
6. Press **Undo all**. Then compare everything with the recorded before
   state: every registry value, service start type, task state and power
   plan must be back. Only the removed apps stay gone (the UI should say so).
   If you applied `power-ultimate-performance`, its copied plan
   (`fe1a7e00-5eed-4f00-9c00-0000000000f1`) stays in `powercfg /list` after
   Undo, but is no longer active - that is expected.
7. Nothing should be reported as "changed since we touched it" unless you
   changed it by hand.

## 3. Record

Paste into the PR description: host build(s) tested, the table of results,
and any tweak that was "not on this PC" or behaved differently from the
table above.
