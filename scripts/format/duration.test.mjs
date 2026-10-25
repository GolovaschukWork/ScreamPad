import assert from "node:assert/strict";
import { test } from "node:test";
import { formatDuration } from "../../src/shared/format/duration.ts";

test("short durations show seconds with one decimal in the locale's format", () => {
  assert.equal(formatDuration(250, "en"), "0.3s");
  assert.equal(formatDuration(1500, "en"), "1.5s");
  assert.match(formatDuration(1500, "uk"), /^1,5\s?с$/);
});

test("durations from a minute show minutes and padded seconds", () => {
  assert.equal(formatDuration(60_000, "en"), "1:00");
  assert.equal(formatDuration(61_400, "uk"), "1:01");
  assert.equal(formatDuration(125_000, "en"), "2:05");
  assert.equal(formatDuration(3_599_000, "en"), "59:59");
});

test("invalid input never produces a negative or NaN duration", () => {
  assert.equal(formatDuration(-5, "en"), "0.0s");
  assert.equal(formatDuration(0, "en"), "0.0s");
});
