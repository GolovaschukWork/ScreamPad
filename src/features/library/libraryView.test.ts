import { describe, expect, it } from "vitest";
import { sound } from "../../test/fakeBindings";
import { filterSounds, sortSounds } from "./libraryView";

const sounds = [
  sound({ id: 1, displayName: "Sound 10", durationMs: 3000 }),
  sound({ id: 2, displayName: "sound 2", durationMs: 1000 }),
  sound({ id: 3, displayName: "Гудок", durationMs: 2000 }),
  sound({ id: 4, displayName: "Airhorn", durationMs: 2000 }),
];

describe("filterSounds", () => {
  it("returns everything for an empty or blank query", () => {
    expect(filterSounds(sounds, "", "en")).toHaveLength(4);
    expect(filterSounds(sounds, "   ", "en")).toHaveLength(4);
  });

  it("matches part of a name, ignoring case", () => {
    expect(filterSounds(sounds, "SOUND", "en").map((s) => s.id)).toEqual([1, 2]);
    expect(filterSounds(sounds, "horn", "en").map((s) => s.id)).toEqual([4]);
  });

  it("ignores case for Cyrillic in the active language", () => {
    expect(filterSounds(sounds, "гуд", "uk").map((s) => s.id)).toEqual([3]);
    expect(filterSounds(sounds, "ГУДОК", "uk").map((s) => s.id)).toEqual([3]);
  });

  it("trims the query and finds nothing for an unknown name", () => {
    expect(filterSounds(sounds, "  airhorn ", "en").map((s) => s.id)).toEqual([4]);
    expect(filterSounds(sounds, "zzz", "en")).toEqual([]);
  });

  it("never changes the input", () => {
    const before = [...sounds];
    filterSounds(sounds, "a", "en");
    expect(sounds).toEqual(before);
  });
});

describe("sortSounds", () => {
  it("sorts names in the language's order with numbers in numeric order", () => {
    expect(sortSounds(sounds, "name", "en").map((s) => s.displayName)).toEqual([
      "Airhorn",
      "sound 2",
      "Sound 10",
      "Гудок",
    ]);
  });

  it("sorts by duration, breaking ties by name", () => {
    expect(sortSounds(sounds, "duration", "en").map((s) => s.id)).toEqual([2, 4, 3, 1]);
  });

  it("puts the most recently added first", () => {
    expect(sortSounds(sounds, "recent", "en").map((s) => s.id)).toEqual([4, 3, 2, 1]);
  });

  it("returns a copy and keeps equal names in id order", () => {
    const twins = [sound({ id: 9, displayName: "Same" }), sound({ id: 5, displayName: "same" })];
    expect(sortSounds(twins, "name", "en").map((s) => s.id)).toEqual([5, 9]);
    expect(twins.map((s) => s.id)).toEqual([9, 5]);
  });
});
