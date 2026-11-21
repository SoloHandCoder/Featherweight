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

## VirtualBox setup (Windows 11 Home host)

The maintainer's PC runs Windows 11 Home, so there is no Sandbox: every
pass below runs in a VirtualBox VM. Make two VMs, one Windows 11 23H2+ and
one Windows 10 22H2, from Microsoft's ISO images.

1. New VM > pick the ISO > type Windows 11 (64-bit) or Windows 10 (64-bit).
   Give it at least 4 GB RAM, 2 CPUs and a 64 GB disk.
2. **Windows 11 only:** Settings > System > Motherboard: tick **Enable EFI**
   and **Enable Secure Boot**; Settings > System > TPM: **v2.0**. Without
   these, Windows 11 setup stops with "This PC can't run Windows 11".
3. Install Windows with a local account that is an administrator. For the
   "different admin" check (section 7) also create a second local admin.
4. Devices > **Insert Guest Additions CD image**, run
   `VBoxWindowsAdditions.exe` in the VM, restart. This gives shared
   clipboard, drag and drop and a resizable screen.
5. Share the build folder read-only: Settings > Shared Folders > add
   `src-tauri\target\release` from the host, **Read-only**, Auto-mount.
   Copy `featherweight.exe` from that share to the VM's desktop.
6. Shut the VM down and take a **snapshot** ("clean"). Restore it before
   each pass, so every pass starts from defaults; take another snapshot
   before anything you want to repeat (e.g. before Undo all).
7. For the Windows 10 pass, use a 22H2 ISO and **do not** install updates
   before testing (see section 9), then snapshot it as "unpatched".

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
| `disable-widgets` (Windows 11 only) | `HKLM\SOFTWARE\Policies\Microsoft\Dsh\AllowNewsAndInterests` = 0; Widgets gone from the taskbar after sign-out or Explorer restart; Settings > Taskbar shows the Widgets switch as managed | value missing; Widgets back after sign-out |
| `disable-news-and-interests` (Windows 10 only) | `HKLM\SOFTWARE\Policies\Microsoft\Windows\Windows Feeds\EnableFeeds` = 0; News and interests gone from the taskbar after sign-out | value missing; feed back after sign-out |
| `no-bing-in-start` | `HKCU\Software\Policies\Microsoft\Windows\Explorer\DisableSearchBoxSuggestions` = 1; `HKCU\Software\Microsoft\Windows\CurrentVersion\Search\BingSearchEnabled` = 0. After sign-out/in, typing "weather" in Start shows no web results | both back; web results return after sign-out/in |
| `no-start-ads` | `CDM` `SystemPaneSuggestionsEnabled`, `SubscribedContent-338388Enabled`, `-338389Enabled`, `-338393Enabled`, `-353694Enabled`, `-353696Enabled` = 0; `ADV\Start_IrisRecommendations` = 0. Win 11: Settings > Personalization > Start shows "Show recommendations for tips..." off. Settings > Privacy > General shows "suggested content" off | all back |
| `no-lockscreen-ads` | `CDM` `RotatingLockScreenOverlayEnabled` = 0 and `SubscribedContent-338387Enabled` = 0; Settings > Personalization > Lock screen shows "Get fun facts, tips..." unticked | both back |
| `no-explorer-ads` | `ADV\ShowSyncProviderNotifications` = 0; File Explorer Options > View shows "Show sync provider notifications" unticked | back |
| `remove-onedrive` | Advanced, not in any preset. Test twice: once with the normal per-user install (`%LOCALAPPDATA%\Microsoft\OneDrive\OneDrive.exe`), and once in a VM where OneDrive is installed per-machine (`OneDriveSetup.exe /allusers`, giving `Program Files\Microsoft OneDrive\OneDrive.exe`). After: OneDrive gone from the tray and from Settings > Apps, both exe paths gone; UI shows "applied" within a few seconds (it must not sit until a 2-minute timeout). Files in `%USERPROFILE%\OneDrive` are still there. With mismatched accounts (elevate with a different admin's password) the tweak shows as unavailable. **Watch for a false failure:** Apply must not end as "Failed" and then reinstall OneDrive on its own (that means setup returned a non-zero exit code even though it worked — write the result down so the check can be loosened). Featherweight runs setup with a reduced, locked-down environment, so also note any setup error dialog. | **Check carefully:** Undo returns promptly (not after ~2 minutes) even though OneDrive starts running. OneDrive is installed again for this user (exe back in `%LOCALAPPDATA%`, listed in Settings > Apps) and the tweak shows "not applied"; you have to sign in to OneDrive again. Note whether setup complained about running as administrator, and that a per-machine install comes back as per-user. |
| `remove-xbox-apps` | Advanced, not in any preset. Try both with and without "also for all users". `Get-AppxPackage \| Where-Object Name -match '^Microsoft\.(GamingApp\|XboxApp)$'` returns nothing; with "all users" ticked, `Get-AppxPackage -AllUsers \| Where-Object Name -match '<same pattern>'` and `Get-AppxProvisionedPackage -Online \| Where-Object DisplayName -match '<same pattern>'` return nothing too. `Microsoft.XboxIdentityProvider`, `Microsoft.Xbox.TCUI`, `Microsoft.XboxGamingOverlay` are still installed; Win+G still opens Game Bar | Nothing comes back (expected); reinstall links open the Xbox / Xbox Console Companion Store pages |

### Background

| id | after Apply | after Undo |
|---|---|---|
| `disable-telemetry-services` | services.msc: "Connected User Experiences and Telemetry" (DiagTrack) and "Device Management Wireless Application Protocol (WAP) Push message Routing Service" (dmwappushservice) are **Disabled** and **stopped**. `HKLM\SOFTWARE\Policies\Microsoft\Windows\DataCollection\AllowTelemetry` = 1. Settings > Windows Update > Check for updates still works | both services back to their before start type (usually Automatic / Manual); policy value missing |
| `disable-telemetry-tasks` | Task Scheduler > Microsoft > Windows > Customer Experience Improvement Program: `Consolidator`, `UsbCeip` **Disabled**; Application Experience: `Microsoft Compatibility Appraiser` and/or `Microsoft Compatibility Appraiser Exp` (newer Windows 11 builds have only the Exp one) **Disabled**. A task that doesn't exist on this build is skipped, not an error | every task it disabled back to Ready |
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

## 3. Startup apps

1. Open Background > **Startup apps**. Compare the list with Task Manager > Startup apps
   and with `HKCU\...\CurrentVersion\Run`, `HKLM\...\CurrentVersion\Run`
   (and `WOW6432Node`) and the two Startup folders.
2. Turn one HKCU entry and one HKLM entry **off**. Before the first change
   the restore point question appears (same as for tweaks). Check the entry's
   value in regedit changed the way Task Manager shows "Disabled"
   (`...\Explorer\StartupApproved\Run`), and Task Manager agrees.
3. Turn one back **on**: the value returns to exactly what it was.
4. Turn the other one off again, then use **Undo all** in the sidebar: it is
   listed, and its value goes back to the recorded before state.
5. Sign out and in: the apps you left on start, the one you turned off
   doesn't.

## 4. Cleanup

1. Open the Cleanup page. Each target shows a size; compare "Your temporary
   files" with `%LOCALAPPDATA%\Temp` (files changed in the last 24 hours
   are not counted or deleted, so the size can be lower than Explorer's).
2. Create `%LOCALAPPDATA%\Temp\fw-fresh.txt` just before cleaning; it must
   still be there afterwards.
3. Clean "Old Windows Update downloads". During the clean `wuauserv` stops;
   afterwards check `Get-Service wuauserv` is **Running** again (or back to
   Stopped if it was stopped before). Then Settings > Windows Update >
   Check for updates works.
4. Clean everything. The result shows freed space and any skipped (in-use)
   files; nothing outside the listed folders is touched (spot-check
   `%LOCALAPPDATA%`, `C:\Windows\Temp` itself still exists).
5. If `%TEMP%` points somewhere other than `%LOCALAPPDATA%\Temp` (set it in
   System > Advanced > Environment Variables, sign out/in), "Your temporary
   files" shows nothing to clean and that folder is left alone.

## 5. Undo all after a restart of the app

1. Apply three tweaks (one HKCU, one HKLM/service, one task) and turn off
   one startup app.
2. Close Featherweight. Check `C:\ProgramData\Featherweight\journal.json`
   exists, and in an admin PowerShell `icacls C:\ProgramData\Featherweight`
   shows only `BUILTIN\Administrators` and `NT AUTHORITY\SYSTEM` (no
   Users, no inheritance). `journal.lock` sits next to the journal.
3. Reopen Featherweight. The tweaks still show "applied".
4. **Undo all**: every value goes back to the before state; the journal file
   is empty of entries afterwards.
5. While Featherweight is open, start a second copy: a message box says
   "Featherweight is already running." and no second window opens.

## 6. Restore point

1. On the first change of a session the app makes a restore point. Open
   `SystemPropertiesProtection.exe` > **System Restore...** > "Show more
   restore points": a point named **Featherweight** with the current time
   is listed (type "Modify settings").
2. A second change in the same session makes no new point.
3. With System Protection turned off for C:, the app asks; "Turn on System
   Restore" turns it on (check the dialog) and then makes the point.
4. Restart the app within 24 hours and change something: it says Windows
   made one recently (throttled) and lets you continue.

## 7. A different administrator

1. Sign in as the standard or second admin account, then start
   Featherweight and enter the **other** admin's password at the UAC prompt.
2. Home shows the banner about a different administrator account; per-user
   tweaks (e.g. `no-start-ads`, `remove-onedrive`, app removals) show as
   unavailable, system-wide ones work.
3. Apply something per-user as admin A (signed in as A), then run
   Featherweight as admin B and press **Undo all**: A's entries are listed
   under "Belongs to another account" and A's values are untouched (check
   in A's session); running again as A undoes them.

## 8. Cold start and console

1. Restart the VM, wait for the desktop to settle, then start Featherweight
   and time from the UAC prompt being accepted until the Home numbers show.
   Write it down (target: under 3 s on 2 CPUs; note if the tweak list makes
   the window feel stuck).
2. In a dev build (`npm run tauri dev`), open the devtools (right-click >
   Inspect, or Ctrl+Shift+I) and check the Console for **Content Security
   Policy** errors or blocked resources while visiting every page, applying,
   undoing and cleaning. There should be none.
3. On the Undo summary, click a **Microsoft Store** link. Opening
   `ms-windows-store:` links from WebView2 is not verified yet: note whether
   the Store opens on the right app. If it doesn't, the product id is still
   shown in the row's text to search for by hand.

## 9. Fresh, unpatched Windows 10 22H2

1. Restore the "unpatched" Windows 10 snapshot (no cumulative updates
   since the 22H2 ISO). Confirm with `winver` (build 19045.xxxx, a low
   revision).
2. Start `featherweight.exe`. It must open (no "entry point not found" or
   missing DLL error, e.g. for `GetTempPath2W`). If WebView2 is missing, the
   installer build offers to install it; the portable exe needs it present.
3. Run section 1's Background checks and the Cleanup page once.

## 10. Record

Paste into the PR description: host build(s) tested, the table of results,
and any tweak that was "not on this PC" or behaved differently from the
table above. Include the cold-start time from section 8.
