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
} from "./types";

const overview: Overview = {
  build: 26100,
  isLaptop: false,
  perUserAllowed: true,
  accountMessage: null,
  stats: { startupEnabled: 14, servicesRunning: 41, junkBytes: 6_657_199_308 },
  journalError: null,
};

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
    description: "",
    section,
    level,
    irreversible: false,
    perUser: false,
    status: { kind: "notApplied" },
    changes,
    reinstallHint: null,
    allUsersOptional: false,
    presets,
    ...extra,
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

const catalog = [
  t("remove-junk-apps", "Remove preinstalled junk apps", "remove", "safe", both,
    junkApps.map((p) => `Remove app "${p}" (all users)`), { irreversible: true, description:
    "Removes games and apps that come preinstalled, like Candy Crush, Solitaire, Clipchamp and News, for every account on this PC. You can reinstall most of them from the Microsoft Store." }),
  t("block-suggested-app-installs", "Stop Windows installing suggested apps", "remove", "safe", both,
    [`${CDM}\\SilentInstalledAppsEnabled = 0 (DWORD)`], { perUser: true }),
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
    { perUser: true, status: { kind: "partial" } }),
  t("no-lockscreen-ads", "No tips or ads on the lock screen", "remove", "safe", both,
    [`${CDM}\\RotatingLockScreenOverlayEnabled = 0 (DWORD)`], { perUser: true }),
  t("no-explorer-ads", "No ads in File Explorer", "remove", "safe", both,
    [String.raw`HKCU\Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced\ShowSyncProviderNotifications = 0 (DWORD)`],
    { perUser: true }),
  t("remove-onedrive", "Uninstall OneDrive", "remove", "advanced", [],
    ["PowerShell: OneDriveSetup.exe /uninstall"], { irreversible: true, perUser: true }),
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
    ["Power plan → High Performance"]),
  t("power-ultimate-performance", "Ultimate Performance power plan", "gaming", "advanced", [],
    ["Power plan → Ultimate Performance"]),
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

const status = new Map<string, TweakStatus>(catalog.map((x) => [x.id, x.status]));
const listeners = new Set<(p: Progress) => void>();
const wait = (ms: number) => new Promise<void>((r) => setTimeout(r, ms));

function views(): TweakView[] {
  return catalog.map(({ presets: _p, ...v }) => ({ ...v, status: status.get(v.id)! }));
}

export const mockApi: Backend = {
  getOverview: async () => overview,
  listTweaks: async () => views(),
  previewPreset: async (p) =>
    catalog.filter((x) => x.presets.includes(p) && x.level === "safe").map((x) => x.id),
  applyTweaks: async (ids) => {
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
  undoTweaks: async (ids) => {
    ids.forEach((id) => status.set(id, { kind: "notApplied" }));
    return { results: ids.map((id) => ({ id, outcomes: [{ kind: "restored" }] })) };
  },
  undoAll: async () => {
    const ids = [...status].filter(([, s]) => s.kind !== "notApplied").map(([id]) => id);
    ids.forEach((id) => status.set(id, { kind: "notApplied" }));
    return { results: ids.map((id) => ({ id, outcomes: [{ kind: "restored" }] })) };
  },
  ensureRestorePoint: async () => {
    await wait(400);
    const kind = new URLSearchParams(location.search).get("restore");
    if (kind === "throttled" || kind === "disabled") return { kind };
    if (kind === "failed") return { kind, message: "The service cannot be started. (0x80070422)" };
    return { kind: "created" };
  },
  enableSystemRestore: async () => wait(300),
  listStartup: async () => [],
  setStartupEnabled: async () => {},
  scanCleanup: async () => {
    await wait(700);
    return [
      { id: "temp", label: "Temporary files", paths: ["%TEMP%"], bytes: 1_288_490_188, needsServiceStop: null },
      { id: "wu-cache", label: "Windows Update cache", paths: ["SoftwareDistribution\\Download"], bytes: 2_254_857_830, needsServiceStop: "wuauserv" },
      { id: "thumbnails", label: "Thumbnail cache", paths: ["Explorer\\thumbcache_*.db"], bytes: 146_800_640, needsServiceStop: null },
      { id: "crash-dumps", label: "Crash dumps and error reports", paths: ["%LOCALAPPDATA%\\CrashDumps"], bytes: 524_288_000, needsServiceStop: null },
      { id: "delivery-opt", label: "Delivery Optimization cache", paths: ["DeliveryOptimization\\Cache"], bytes: 0, needsServiceStop: null },
    ];
  },
  runCleanup: async (ids) => {
    await wait(900);
    return { freedBytes: ids.length * 700_000_000, skippedFiles: 12 };
  },
  onApplyProgress: async (cb) => {
    listeners.add(cb);
    return () => listeners.delete(cb);
  },
};
