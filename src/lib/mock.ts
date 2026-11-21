// Dev-only stand-in for the Tauri backend, loaded by state.svelte.ts when
// `import.meta.env.DEV && !isTauri()` and the URL has `?mock`. Never
// imported in a production build. Data mirrors tweaks/*.toml.

import type { Backend } from "./backend";
import type {
  ApplyReport,
  Level,
  Overview,
  Preset,
  Progress,
  Section,
  TweakStatus,
  TweakView,
  UndoResult,
  StartupEntry,
  CleanupTarget,
} from "./types";

// URL switches for screenshots of the rarer states:
//   ?mock&account   running as another admin (per-user tweaks locked)
//   ?mock&journal   the undo record can't be read (changes paused)
//   ?mock&failapply applyTweaks throws
//   ?mock&restore=throttled|disabled|failed   restore point problems
const flags = new URLSearchParams(typeof location === "undefined" ? "" : location.search);

const overview: Overview = {
  build: 26100,
  isLaptop: false,
  perUserAllowed: !flags.has("account"),
  accountMessage: flags.has("account")
    ? "You're running Featherweight with a different administrator account than the one signed in. Settings that only affect your account are turned off. Sign in as an administrator to use them."
    : null,
  // junkBytes is the sum of the scan below, as the real overview reports it.
  stats: { startupEnabled: 5, servicesRunning: 41, junkBytes: 4_214_436_658 },
  journalError: flags.has("journal") ? "The file journal.json is damaged (expected value at line 1 column 1)." : null,
};

/** Descriptions straight from tweaks/*.toml, so the explainer shows the real copy. */
const tomls = import.meta.glob("../../tweaks/*.toml", { query: "?raw", import: "default", eager: true }) as Record<
  string,
  string
>;
const descriptions = new Map(
  Object.values(tomls).flatMap((src) => {
    const id = /^id = "([^"]+)"/m.exec(src)?.[1];
    const d = /^description = "((?:[^"\\]|\\.)*)"/m.exec(src)?.[1];
    return id && d ? [[id, d.replace(/\\"/g, '"')] as const] : [];
  }),
);

const CDM = String.raw`HKCU\Software\Microsoft\Windows\CurrentVersion\ContentDeliveryManager`;

function t(
  id: string,
  title: string,
  section: Section,
  level: Level,
  presets: Preset[],
  changes: string[],
  extra: Partial<TweakView> = {},
): TweakView & { presets: Preset[] } {
  return {
    id,
    title,
    section,
    level,
    irreversible: false,
    perUser: false,
    status: { kind: "notApplied" },
    changes,
    reinstallHint: null,
    allUsersOptional: false,
    exclusiveWith: null,
    presets,
    ...extra,
    description: descriptions.get(id) ?? extra.description ?? "",
  };
}

const both: Preset[] = ["everyday", "gaming"];
const junkApps = [
  "king.com.CandyCrushSaga",
  "king.com.CandyCrushSodaSaga",
  "king.com.CandyCrushFriends",
  "Microsoft.MicrosoftSolitaireCollection",
  "Clipchamp.Clipchamp",
  "Microsoft.BingNews",
  "Microsoft.BingWeather",
  "Microsoft.Getstarted",
  "Microsoft.WindowsFeedbackHub",
  "Microsoft.Todos",
  "Microsoft.People",
  "Microsoft.WindowsMaps",
  "Microsoft.Microsoft3DViewer",
];

const startup: StartupEntry[] = [
  { id: "hkcu-run-Discord", name: "Discord", command: String.raw`C:\Users\Sam\AppData\Local\Discord\Update.exe --processStart Discord.exe`, location: "hkcuRun", enabled: true, perUser: true },
  { id: "hkcu-run-Spotify", name: "Spotify", command: String.raw`C:\Users\Sam\AppData\Roaming\Spotify\Spotify.exe --autostart --minimized`, location: "hkcuRun", enabled: true, perUser: true },
  { id: "hkcu-run-Steam", name: "Steam", command: String.raw`"C:\Program Files (x86)\Steam\steam.exe" -silent`, location: "hkcuRun", enabled: false, perUser: true },
  { id: "hklm-run-RtkAudUService", name: "Realtek HD Audio Manager", command: String.raw`"C:\Windows\System32\DriverStore\FileRepository\realtekservice.inf_amd64_8f2a\RtkAudUService64.exe" -background`, location: "hklmRun", enabled: true, perUser: false },
  { id: "hklm-run-SecurityHealth", name: "Windows Security notification icon", command: String.raw`%windir%\system32\SecurityHealthSystray.exe`, location: "hklmRun", enabled: true, perUser: false },
  { id: "common-folder-Zoom", name: "Zoom", command: String.raw`C:\ProgramData\Microsoft\Windows\Start Menu\Programs\StartUp\Zoom.lnk`, location: "commonFolder", enabled: true, perUser: false },
];

const catalog = [
  t("remove-junk-apps", "Remove preinstalled junk apps", "remove", "safe", both,
    junkApps.map((p) => `Remove app "${p}" (all users)`), { irreversible: true, reinstallHint: "9NBLGGH18846", description:
    "Removes games and apps that come preinstalled, like Candy Crush, Solitaire, Clipchamp and News, for every account on this PC. You can reinstall most of them from the Microsoft Store." }),
  t("block-suggested-app-installs", "Stop Windows installing suggested apps", "remove", "safe", both,
    [`${CDM}\\SilentInstalledAppsEnabled = 0 (DWORD)`], { perUser: true, description:
    "Stops Windows quietly installing promoted apps and games from the Store." }),
  t("disable-copilot", "Turn off Copilot in Windows", "remove", "safe", both,
    [String.raw`HKCU\Software\Policies\Microsoft\Windows\WindowsCopilot\TurnOffWindowsCopilot = 1 (DWORD)`],
    { perUser: true, status: { kind: "applied" } }),
  t("disable-recall", "Turn off Recall", "remove", "safe", both,
    [String.raw`HKLM\SOFTWARE\Policies\Microsoft\Windows\WindowsAI\DisableAIDataAnalysis = 1 (DWORD)`]),
  t("disable-widgets", "Turn off Widgets and News", "remove", "safe", both,
    [String.raw`HKLM\SOFTWARE\Policies\Microsoft\Dsh\AllowNewsAndInterests = 0 (DWORD)`]),
  t("no-bing-in-start", "No Bing web results in Start search", "remove", "safe", both,
    [String.raw`HKCU\Software\Policies\Microsoft\Windows\Explorer\DisableSearchBoxSuggestions = 1 (DWORD)`],
    { perUser: true }),
  t("no-start-ads", "No ads or suggestions in Start and Settings", "remove", "safe", both,
    [`${CDM}\\SystemPaneSuggestionsEnabled = 0 (DWORD)`, `${CDM}\\SubscribedContent-338388Enabled = 0 (DWORD)`],
    { perUser: true, status: { kind: "partial" }, description:
    "Stops Windows showing suggested apps and promotions in the Start menu and the Settings app." }),
  t("no-lockscreen-ads", "No tips or ads on the lock screen", "remove", "safe", both,
    [`${CDM}\\RotatingLockScreenOverlayEnabled = 0 (DWORD)`], { perUser: true }),
  t("no-explorer-ads", "No ads in File Explorer", "remove", "safe", both,
    [String.raw`HKCU\Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced\ShowSyncProviderNotifications = 0 (DWORD)`],
    { perUser: true }),
  t("remove-onedrive", "Uninstall OneDrive", "remove", "advanced", [],
    ["PowerShell: OneDriveSetup.exe /uninstall"], { irreversible: true, perUser: true, description:
    "Uninstalls OneDrive for your account. Files already in your OneDrive folder stay on this PC." }),
  t("remove-xbox-apps", "Remove Xbox apps", "remove", "advanced", [],
    ['Remove app "Microsoft.GamingApp"', 'Remove app "Microsoft.XboxApp"'],
    { irreversible: true, allUsersOptional: true }),
  t("disable-telemetry-services", "Turn off diagnostic data services", "background", "safe", both,
    ['Service "DiagTrack" → Disabled', 'Service "dmwappushservice" → Disabled']),
  t("disable-telemetry-tasks", "Turn off diagnostic data tasks", "background", "safe", both,
    [String.raw`Scheduled task "\Microsoft\Windows\Application Experience\ProgramDataUpdater" → Disabled`]),
  t("disable-unused-services", "Turn off rarely used services", "background", "safe", both,
    ['Service "Fax" → Disabled', 'Service "MapsBroker" → Disabled']),
  t("disable-print-spooler", "Turn off printing", "background", "advanced", [],
    ['Service "Spooler" → Disabled']),
  t("disable-bluetooth-support", "Turn off Bluetooth support", "background", "advanced", [],
    ['Service "bthserv" → Disabled']),
  t("power-high-performance", "High Performance power plan", "gaming", "safe", ["gaming"],
    ["Power plan → High Performance"], { exclusiveWith: "power-ultimate-performance" }),
  t("power-ultimate-performance", "Ultimate Performance power plan", "gaming", "advanced", [],
    ["Power plan → Ultimate Performance"], { exclusiveWith: "power-high-performance" }),
  t("game-mode-on", "Game Mode on", "gaming", "safe", ["gaming"],
    [String.raw`HKCU\Software\Microsoft\GameBar\AutoGameModeEnabled = 1 (DWORD)`],
    { perUser: true, status: { kind: "applied" } }),
  t("game-bar-recording-off", "Game Bar recording off", "gaming", "safe", ["gaming"],
    [String.raw`HKCU\System\GameConfigStore\GameDVR_Enabled = 0 (DWORD)`], { perUser: true }),
  t("mouse-accel-off", "Mouse acceleration off", "gaming", "safe", ["gaming"],
    [String.raw`HKCU\Control Panel\Mouse\MouseSpeed = 0 (String)`], { perUser: true }),
  t("network-throttling-off", "Network throttling off", "gaming", "advanced", [],
    [String.raw`HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Multimedia\SystemProfile\NetworkThrottlingIndex = 4294967295 (DWORD)`]),
  t("system-responsiveness", "Less CPU reserved for background tasks", "gaming", "advanced", [],
    [String.raw`HKLM\SOFTWARE\Microsoft\Windows NT\CurrentVersion\Multimedia\SystemProfile\SystemResponsiveness = 10 (DWORD)`]),
];

// Running as another admin: the backend marks per-user tweaks not applicable.
const status = new Map<string, TweakStatus>(
  catalog.map((x) => [x.id, flags.has("account") && x.perUser ? { kind: "notApplicable", reason: "accountMismatch" } : x.status]),
);
const listeners = new Set<(p: Progress) => void>();
const wait = (ms: number) => new Promise<void>((r) => setTimeout(r, ms));

function views(): TweakView[] {
  return catalog.map((v) => {
    const st = status.get(v.id)!;
    const inPresets = v.level === "safe" && st.kind !== "notApplicable";
    return { ...v, status: st, presets: inPresets ? v.presets : [] };
  });
}

/** Copilot's value was "changed since" until forced; irreversible ones need a reinstall. */
function undoOne(id: string, force: boolean): UndoResult {
  const tw = catalog.find((x) => x.id === id);
  if (tw?.irreversible) {
    status.set(id, { kind: "notApplied" });
    const apps = tw.changes.map((c) => /^Remove app "([^"]+)"/.exec(c)?.[1]).filter((a) => a !== undefined);
    return {
      id,
      outcomes: apps.map((app, i) => ({ kind: "manualReinstall", app, hint: i === 0 ? tw.reinstallHint : null })),
    };
  }
  if (id === "disable-copilot" && !force) {
    return { id, outcomes: [{ kind: "changedSince", what: "TurnOffWindowsCopilot" }] };
  }
  const was = status.get(id)?.kind;
  status.set(id, { kind: "notApplied" });
  return { id, outcomes: [{ kind: was === "notApplied" ? "alreadyOriginal" : "restored" }] };
}

function scanTargets(): CleanupTarget[] {
  return [
    { id: "temp", label: "Temporary files", paths: ["%TEMP%"], bytes: 1_288_490_188, needsServiceStop: null },
    { id: "wu-cache", label: "Windows Update cache", paths: ["SoftwareDistribution\\Download"], bytes: 2_254_857_830, needsServiceStop: "wuauserv" },
    { id: "thumbnails", label: "Thumbnail cache", paths: ["Explorer\\thumbcache_*.db"], bytes: 146_800_640, needsServiceStop: null },
    { id: "crash-dumps", label: "Crash dumps and error reports", paths: ["%LOCALAPPDATA%\\CrashDumps"], bytes: 524_288_000, needsServiceStop: null },
    { id: "delivery-opt", label: "Delivery Optimization cache", paths: ["DeliveryOptimization\\Cache"], bytes: 0, needsServiceStop: null },
  ];
}

export const mockApi: Backend = {
  getOverview: async () => overview,
  listTweaks: async () => views(),
  previewPreset: async (p) =>
    catalog.filter((x) => x.presets.includes(p) && x.level === "safe").map((x) => x.id),
  applyTweaks: async (ids) => {
    if (flags.has("failapply")) {
      await wait(400);
      throw new Error("The Featherweight service stopped responding.");
    }
    const report: ApplyReport = { results: [] };
    for (const [i, id] of ids.entries()) {
      const tw = catalog.find((x) => x.id === id);
      listeners.forEach((cb) => cb({ done: i, total: ids.length, current: tw?.title ?? id }));
      await wait(180);
      if (id === "disable-recall") {
        report.results.push({ id, outcome: { kind: "notApplicable", reason: "wrongBuild" } });
      } else if (status.get(id)?.kind === "applied") {
        report.results.push({ id, outcome: { kind: "alreadyApplied" } });
      } else if (id === "disable-telemetry-tasks") {
        report.results.push({ id, outcome: { kind: "failed", message: "Access is denied. (os error 5)" } });
      } else {
        status.set(id, { kind: "applied" });
        report.results.push({ id, outcome: { kind: "applied" } });
      }
    }
    listeners.forEach((cb) => cb({ done: ids.length, total: ids.length, current: "" }));
    return report;
  },
  undoTweaks: async (ids, force) => {
    await wait(300);
    return { results: ids.map((id) => undoOne(id, force)) };
  },
  undoAll: async (force) => {
    await wait(300);
    const ids = [...status].filter(([, s]) => s.kind !== "notApplied").map(([id]) => id);
    return { results: ids.map((id) => undoOne(id, force)) };
  },
  ensureRestorePoint: async () => {
    await wait(400);
    const kind = new URLSearchParams(location.search).get("restore");
    if (kind === "throttled" || kind === "disabled") return { kind };
    if (kind === "failed") return { kind, message: "The service cannot be started. (0x80070422)" };
    return { kind: "created" };
  },
  enableSystemRestore: async () => wait(300),
  listStartup: async () => {
    await wait(200);
    return startup.map((e) => ({ ...e }));
  },
  setStartupEnabled: async (id, enabled) => {
    await wait(300);
    const e = startup.find((x) => x.id === id);
    if (e) e.enabled = enabled;
  },
  scanCleanup: async () => {
    await wait(700);
    return scanTargets();
  },
  runCleanup: async (ids) => {
    await wait(900);
    const bytes = scanTargets().filter((t) => ids.includes(t.id)).reduce((n, t) => n + t.bytes, 0);
    // A few files are always in use and get skipped.
    return { freedBytes: Math.round(bytes * 0.97), skippedFiles: 12 };
  },
  onApplyProgress: async (cb) => {
    listeners.add(cb);
    return () => listeners.delete(cb);
  },
};
