// Exact mirrors of the Rust command views (src-tauri/src/commands.rs and the
// Serialize types it returns). Field names are camelCase; enums that carry
// data are tagged unions on `kind`, so the UI can `switch (x.kind)`.
// Every command rejects with a plain `string` error message.

/** model::Section (snake_case in Rust). */
export type Section = "remove" | "background" | "gaming" | "cleanup";

/** model::Level. */
export type Level = "safe" | "advanced";

/** model::Preset, the argument of `previewPreset`. */
export type Preset = "everyday" | "gaming";

/** commands::NaReasonView */
export type NaReason = "wrongBuild" | "accountMismatch" | "missingOnThisPc";

/** commands::StatusView */
export type TweakStatus =
  | { kind: "applied" }
  | { kind: "notApplied" }
  | { kind: "partial" }
  | { kind: "notApplicable"; reason: NaReason };

/** stats::Stats */
export interface Stats {
  startupEnabled: number;
  servicesRunning: number;
  junkBytes: number;
}

/** commands::Overview */
export interface Overview {
  build: number;
  isLaptop: boolean;
  perUserAllowed: boolean;
  accountMessage: string | null;
  stats: Stats;
  /** Non-null when the undo journal can't be trusted; apply/undo are refused. */
  journalError: string | null;
}

/** commands::TweakView */
export interface TweakView {
  id: string;
  title: string;
  description: string;
  section: Section;
  /** Effective level on this PC (laptop level wins on laptops). */
  level: Level;
  irreversible: boolean;
  perUser: boolean;
  status: TweakStatus;
  /** One human-readable line per action. */
  changes: string[];
  reinstallHint: string | null;
  allUsersOptional: boolean;
  /** Id of the tweak that can't be on together with this one. */
  exclusiveWith: string | null;
}

/** commands::ApplyOutcomeView */
export type ApplyOutcome =
  | { kind: "applied" }
  | { kind: "alreadyApplied" }
  | { kind: "failed"; message: string }
  | { kind: "notApplicable"; reason: NaReason };

/** commands::ApplyResult */
export interface ApplyResult {
  id: string;
  outcome: ApplyOutcome;
}

/** commands::ApplyReport */
export interface ApplyReport {
  results: ApplyResult[];
}

/** commands::UndoOutcomeView */
export type UndoOutcome =
  | { kind: "restored" }
  | { kind: "alreadyOriginal" }
  /** `what` names the value something else changed since it was applied. */
  | { kind: "changedSince"; what: string }
  /** A removed app: `app` is its package name, `hint` its Store product id. */
  | { kind: "manualReinstall"; app: string; hint: string | null }
  /** Journaled for another Windows account and left alone; `what` names the value. */
  | { kind: "otherAccount"; what: string }
  | { kind: "failed"; message: string };

/** commands::UndoResult */
export interface UndoResult {
  id: string;
  /** One per changed value of the tweak, in undo order. */
  outcomes: UndoOutcome[];
}

/** commands::UndoReport */
export interface UndoReport {
  results: UndoResult[];
}

/**
 * commands::ProgressView, payload of the "apply-progress" event. Emitted
 * before each tweak (`done` = tweaks finished so far) and once at the end
 * with `done === total` and `current === ""`.
 */
export interface Progress {
  done: number;
  total: number;
  current: string;
}

/** commands::RestorePointView */
export type RestorePointOutcome =
  | { kind: "created" }
  | { kind: "alreadyMadeThisSession" }
  | { kind: "throttled" }
  | { kind: "disabled" }
  | { kind: "failed"; message: string };

/** startup::StartupLocation */
export type StartupLocation =
  | "hkcuRun"
  | "hklmRun"
  | "hklmRun32"
  | "userFolder"
  | "commonFolder";

/** startup::StartupEntry */
export interface StartupEntry {
  id: string;
  name: string;
  command: string;
  location: StartupLocation;
  enabled: boolean;
  perUser: boolean;
}

/** cleanup::CleanupTarget */
export interface CleanupTarget {
  id: string;
  label: string;
  paths: string[];
  bytes: number;
  needsServiceStop: string | null;
}

/** cleanup::CleanupReport */
export interface CleanupReport {
  freedBytes: number;
  skippedFiles: number;
}
