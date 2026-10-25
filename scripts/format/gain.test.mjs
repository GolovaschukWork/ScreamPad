import assert from "node:assert/strict";
import { test } from "node:test";
import { dbToMillibels, formatDb, millibelsToDb } from "../../src/shared/format/gain.ts";

test("millibels and decibels convert both ways", () => {
  assert.equal(millibelsToDb(-2000), -20);
  assert.equal(millibelsToDb(600), 6);
  assert.equal(dbToMillibels(-20), -2000);
  assert.equal(dbToMillibels(5.6), 600);
  assert.equal(dbToMillibels(0), 0);
});

test("gains are shown with an explicit sign except zero", () => {
  assert.equal(formatDb(0, "en"), "0 dB");
  assert.equal(formatDb(600, "en"), "+6 dB");
  assert.match(formatDb(-2000, "en"), /^[-−]20 dB$/);
  assert.match(formatDb(-2000, "uk"), /^[-−]20 dB$/);
});
