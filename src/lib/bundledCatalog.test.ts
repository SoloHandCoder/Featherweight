// Feeds the real bundled catalog's app removals (tweaks/*.toml) through the
// undo grouping, so a catalog format change the UI can't link is caught.
import { readdirSync, readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { describe, expect, it } from "vitest";
import { groupUndoReport, storeLink } from "./applyFlow";
import type { UndoOutcome, UndoReport } from "./types";

const dir = fileURLToPath(new URL("../../tweaks/", import.meta.url));

/** tweak id -> its Appx actions (package, reinstall product id). */
function bundledAppRemovals(): Map<string, { app: string; hint: string | null }[]> {
  const out = new Map<string, { app: string; hint: string | null }[]>();
  for (const f of readdirSync(dir).filter((n) => n.endsWith(".toml") && !n.startsWith("_"))) {
    const src = readFileSync(dir + f, "utf8");
    const id = /^id = "([^"]+)"/m.exec(src)![1];
    const apps = src
      .split("[[actions]]")
      .slice(1)
      .filter((b) => /^type = "appx"/m.test(b))
      .map((b) => ({
        app: /^package = "([^"]+)"/m.exec(b)![1],
        hint: /^reinstall = "([^"]+)"/m.exec(b)?.[1] ?? null,
      }));
    if (apps.length > 0) out.set(id, apps);
  }
  return out;
}

describe("bundled catalog through the undo summary", () => {
  const removals = bundledAppRemovals();

  it("finds the app-removing tweaks", () => {
    expect([...removals.keys()].sort()).toEqual(["remove-junk-apps", "remove-xbox-apps"]);
  });

  it("gives every removed app its own row, and every product id a Store link", () => {
    const report: UndoReport = {
      results: [...removals].map(([id, apps]) => ({
        id,
        outcomes: apps.map((a): UndoOutcome => ({ kind: "manualReinstall", app: a.app, hint: a.hint })),
      })),
    };
    const groups = groupUndoReport(report, []);
    expect(groups.map((g) => g.label)).toEqual(["Reinstall manually"]);
    const items = groups[0].items;
    const all = [...removals.values()].flat();
    expect(items).toHaveLength(all.length);
    all.forEach((a, i) => {
      if (a.hint) {
        expect(items[i].storeId).toBe(a.hint);
        expect(storeLink(a.hint)).toBe(`ms-windows-store://pdp/?ProductId=${a.hint}`);
        // The id stays visible as text in case the Store link can't open.
        expect(items[i].detail).toContain(a.hint);
      } else {
        expect(items[i].storeId).toBeUndefined();
      }
    });
    expect(all.filter((a) => a.hint).length).toBeGreaterThan(5);
  });
});
