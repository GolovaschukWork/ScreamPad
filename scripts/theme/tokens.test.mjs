// Checks the design tokens in src/app/index.css: both themes define the same set, every semantic color is a plain hex
// value (no raw colors elsewhere is enforced by the grep test below), and text pairs meet WCAG AA contrast.
import assert from "node:assert/strict";
import { readFileSync, readdirSync, statSync } from "node:fs";
import { join } from "node:path";
import { test } from "node:test";

const root = new URL("../../", import.meta.url).pathname;
const css = readFileSync(join(root, "src/app/index.css"), "utf8");

function block(selector) {
  const start = css.indexOf(`${selector} {`);
  assert.ok(start >= 0, `missing ${selector}`);
  return css.slice(start, css.indexOf("\n}", start));
}

function colors(selector) {
  const out = {};
  for (const [, name, value] of block(selector).matchAll(/--([a-z-]+):\s*(#[0-9a-fA-F]{6})\s*;/g)) out[name] = value;
  return out;
}

const light = colors(":root");
const dark = colors(":root.dark");

function luminance(hex) {
  const channels = [1, 3, 5].map((i) => parseInt(hex.slice(i, i + 2), 16) / 255);
  const [r, g, b] = channels.map((c) => (c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4));
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

function contrast(a, b) {
  const [hi, lo] = [luminance(a), luminance(b)].sort((x, y) => y - x);
  return (hi + 0.05) / (lo + 0.05);
}

test("light and dark define exactly the same semantic colors", () => {
  assert.deepEqual(Object.keys(dark).sort(), Object.keys(light).sort());
  assert.ok(Object.keys(light).length >= 15);
});

const TEXT_PAIRS = [
  ["foreground", "background"],
  ["card-foreground", "card"],
  ["popover-foreground", "popover"],
  ["primary-foreground", "primary"],
  ["secondary-foreground", "secondary"],
  ["muted-foreground", "background"],
  ["muted-foreground", "card"],
  ["muted-foreground", "muted"],
  ["accent-foreground", "accent"],
  ["destructive", "background"],
  ["destructive", "card"],
  ["primary", "background"],
];

for (const [name, theme] of [["light", light], ["dark", dark]]) {
  test(`${name}: text pairs reach AA contrast (4.5:1)`, () => {
    for (const [fg, bg] of TEXT_PAIRS) {
      const ratio = contrast(theme[fg], theme[bg]);
      assert.ok(ratio >= 4.5, `${name}: ${fg} on ${bg} is ${ratio.toFixed(2)}:1`);
    }
  });

  test(`${name}: the focus ring and control borders are visible (3:1)`, () => {
    for (const bg of ["background", "card"]) {
      assert.ok(contrast(theme.ring, theme[bg]) >= 3, `${name}: ring on ${bg}`);
    }
    assert.ok(contrast(theme.border, theme.card) >= 1.3, `${name}: border is not invisible`);
  });
}

function sourceFiles(dir) {
  return readdirSync(dir).flatMap((entry) => {
    const path = join(dir, entry);
    if (statSync(path).isDirectory()) return sourceFiles(path);
    return /\.(tsx?|css)$/.test(entry) ? [path] : [];
  });
}

test("no raw colors outside the token stylesheet", () => {
  const raw = /#[0-9a-fA-F]{3,8}\b|\b(?:rgb|rgba|hsl|hsla|oklch)\(|\b(?:bg|text|border|ring|fill|stroke)-(?:white|black)\b/;
  const offenders = sourceFiles(join(root, "src"))
    .filter((file) => !file.endsWith("src/app/index.css") && !file.endsWith("bindings.ts"))
    .filter((file) => raw.test(readFileSync(file, "utf8")));
  assert.deepEqual(offenders, []);
});
