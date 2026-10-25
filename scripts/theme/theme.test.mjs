import assert from "node:assert/strict";
import { test } from "node:test";
import { applyTheme, parseTheme, resolveTheme, THEMES } from "../../src/shared/theme/theme.ts";

function fakeRoot(initial = []) {
  const classes = new Set(initial);
  return {
    classes,
    classList: { add: (name) => classes.add(name), remove: (name) => classes.delete(name) },
    style: { colorScheme: "" },
  };
}

test("only the three known themes parse", () => {
  assert.deepEqual([...THEMES], ["system", "dark", "light"]);
  for (const theme of THEMES) assert.equal(parseTheme(theme), theme);
  for (const bad of ["", "Dark", "auto", "dark ", "high-contrast", "system\0"]) assert.equal(parseTheme(bad), undefined, bad);
});

test("system follows the operating system, explicit themes ignore it", () => {
  assert.equal(resolveTheme("system", true), "dark");
  assert.equal(resolveTheme("system", false), "light");
  for (const systemDark of [true, false]) {
    assert.equal(resolveTheme("dark", systemDark), "dark");
    assert.equal(resolveTheme("light", systemDark), "light");
  }
});

test("applying a theme leaves exactly one scheme class and sets the native color scheme", () => {
  const root = fakeRoot(["other"]);
  assert.equal(applyTheme(root, "dark", false), "dark");
  assert.deepEqual([...root.classes].sort(), ["dark", "other"]);
  assert.equal(root.style.colorScheme, "dark");

  assert.equal(applyTheme(root, "light", true), "light");
  assert.deepEqual([...root.classes].sort(), ["light", "other"]);
  assert.equal(root.style.colorScheme, "light");

  assert.equal(applyTheme(root, "system", true), "dark");
  assert.deepEqual([...root.classes].sort(), ["dark", "other"]);
});

test("applying the same theme twice changes nothing", () => {
  const root = fakeRoot();
  applyTheme(root, "system", false);
  applyTheme(root, "system", false);
  assert.deepEqual([...root.classes], ["light"]);
});
