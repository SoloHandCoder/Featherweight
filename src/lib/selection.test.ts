import { describe, expect, it } from "vitest";
import { defaultToggles, pendingChanges, setToggle, visibleTweaks } from "./selection";
import type { Level, TweakStatus, TweakView } from "./types";

function tweak(
  id: string,
  status: TweakStatus["kind"] = "notApplied",
  level: Level = "safe",
  exclusiveWith: string | null = null,
): TweakView {
  return {
    id,
    title: id,
    description: "",
    section: "gaming",
    level,
    irreversible: false,
    perUser: false,
    status: status === "notApplicable" ? { kind: status, reason: "wrongBuild" } : { kind: status },
    changes: [],
    reinstallHint: null,
    allUsersOptional: false,
    exclusiveWith,
  };
}

describe("pendingChanges", () => {
  it("toggle_on_unapplied_goes_to_apply", () => {
    const tweaks = [tweak("a"), tweak("b"), tweak("c", "applied")];
    expect(pendingChanges(tweaks, { a: true, b: false, c: true })).toEqual({ apply: ["a"], undo: [] });
  });

  it("toggle_off_applied_goes_to_undo", () => {
    const tweaks = [tweak("a", "applied"), tweak("b", "partial"), tweak("c")];
    expect(pendingChanges(tweaks, { a: false, b: false, c: false })).toEqual({ apply: [], undo: ["a", "b"] });
  });

  it("partial_left_on_is_applied_again", () => {
    expect(pendingChanges([tweak("p", "partial")], { p: true })).toEqual({ apply: ["p"], undo: [] });
  });

  it("missing_toggle_means_no_change_and_not_applicable_is_ignored", () => {
    const tweaks = [tweak("a", "applied"), tweak("b"), tweak("na", "notApplicable")];
    expect(pendingChanges(tweaks, { na: true })).toEqual({ apply: [], undo: [] });
  });
});

describe("defaultToggles", () => {
  it("advanced_not_preselected", () => {
    const tweaks = [
      tweak("safe-off"),
      tweak("adv-off", "notApplied", "advanced"),
      tweak("adv-on", "applied", "advanced"),
      tweak("part", "partial"),
      tweak("na", "notApplicable", "advanced"),
    ];
    expect(defaultToggles(tweaks)).toEqual({
      "safe-off": false,
      "adv-off": false,
      "adv-on": true,
      part: true,
      na: false,
    });
  });
});

describe("setToggle", () => {
  const tweaks = [
    tweak("hp", "applied", "safe", "up"),
    tweak("up", "notApplied", "advanced", "hp"),
    tweak("x"),
  ];

  it("switching_on_turns_off_exclusive_partner", () => {
    const next = setToggle(tweaks, { hp: true, up: false, x: false }, "up", true);
    expect(next).toEqual({ hp: false, up: true, x: false });
    expect(pendingChanges(tweaks, next)).toEqual({ apply: ["up"], undo: ["hp"] });
  });

  it("switching_off_leaves_partner_alone", () => {
    expect(setToggle(tweaks, { hp: true, up: false, x: false }, "hp", false)).toEqual({
      hp: false,
      up: false,
      x: false,
    });
  });

  it("does_not_mutate_input", () => {
    const before = { hp: true, up: false, x: false };
    setToggle(tweaks, before, "up", true);
    expect(before).toEqual({ hp: true, up: false, x: false });
  });
});

describe("visibleTweaks", () => {
  it("keeps_the_section_and_hides_not_applicable", () => {
    const other = { ...tweak("r"), section: "remove" as const };
    const tweaks = [tweak("g"), tweak("na", "notApplicable"), other];
    expect(visibleTweaks(tweaks, "gaming").map((t) => t.id)).toEqual(["g"]);
  });
});
