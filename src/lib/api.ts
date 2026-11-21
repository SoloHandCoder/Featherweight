// Typed wrappers over the Tauri commands in src-tauri/src/commands.rs.
// Argument names are camelCase here; Tauri maps them to the Rust
// snake_case parameters. Every call rejects with a `string` message.

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  ApplyReport,
  CleanupReport,
  CleanupTarget,
  Overview,
  Preset,
  Progress,
  RestorePointOutcome,
  StartupEntry,
  TweakView,
  UndoReport,
} from "./types";

export const APPLY_PROGRESS = "apply-progress";

export function getOverview(): Promise<Overview> {
  return invoke("get_overview");
}

export function listTweaks(): Promise<TweakView[]> {
  return invoke("list_tweaks");
}

/** Ids of the preset's tweaks that are Safe and applicable on this PC. */
export function previewPreset(preset: Preset): Promise<string[]> {
  return invoke("preview_preset", { preset });
}

/** `allUsers`: ids whose optional Appx removal should cover all users. */
export function applyTweaks(ids: string[], allUsers: string[]): Promise<ApplyReport> {
  return invoke("apply_tweaks", { ids, allUsers });
}

export function undoTweaks(ids: string[], force: boolean): Promise<UndoReport> {
  return invoke("undo_tweaks", { ids, force });
}

export function undoAll(force: boolean): Promise<UndoReport> {
  return invoke("undo_all", { force });
}

export function ensureRestorePoint(): Promise<RestorePointOutcome> {
  return invoke("ensure_restore_point");
}

export function enableSystemRestore(): Promise<void> {
  return invoke("enable_system_restore");
}

export function listStartup(): Promise<StartupEntry[]> {
  return invoke("list_startup");
}

export function setStartupEnabled(id: string, enabled: boolean): Promise<void> {
  return invoke("set_startup_enabled", { id, enabled });
}

export function scanCleanup(): Promise<CleanupTarget[]> {
  return invoke("scan_cleanup");
}

export function runCleanup(ids: string[]): Promise<CleanupReport> {
  return invoke("run_cleanup", { ids });
}

/** Subscribes to apply progress; call the returned function to stop. */
export function onApplyProgress(cb: (p: Progress) => void): Promise<UnlistenFn> {
  return listen<Progress>(APPLY_PROGRESS, (e) => cb(e.payload));
}
