#!/usr/bin/env node
// Catalog parity check (ADR 0008). English is the source of truth. No dependencies.
//   node scripts/i18n/check-catalogs.mjs [localesDir] [sourceDir]
import { readdirSync, readFileSync, statSync } from "node:fs";
import { basename, extname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const SOURCE_LANGUAGE = "en";
const PLURAL_SUFFIXES = ["zero", "one", "two", "few", "many", "other"];
/** Keys looked up with a computed name; everything under these prefixes counts as used. */
const DYNAMIC_PREFIXES = ["errors."];

function flatten(object, prefix = "") {
  const out = {};
  for (const [key, value] of Object.entries(object)) {
    const path = prefix ? `${prefix}.${key}` : key;
    if (value !== null && typeof value === "object") Object.assign(out, flatten(value, path));
    else out[path] = value;
  }
  return out;
}

function splitPlural(key) {
  const match = /^(.*)_(zero|one|two|few|many|other)$/.exec(key);
  return match ? { base: match[1], category: match[2] } : { base: key, category: null };
}

function placeholders(text) {
  return new Set([...String(text).matchAll(/{{\s*([\w.]+)\s*}}/g)].map((m) => m[1]));
}

/** Groups flattened entries by base key: `{ base: { plural: boolean, variants: { category|"": text } } }`. */
function group(flat) {
  const groups = {};
  for (const [key, value] of Object.entries(flat)) {
    const { base, category } = splitPlural(key);
    const entry = (groups[base] ??= { plural: false, variants: {} });
    entry.variants[category ?? ""] = value;
    if (category) entry.plural = true;
  }
  return groups;
}

function unionPlaceholders(entry) {
  const all = new Set();
  for (const text of Object.values(entry.variants)) for (const name of placeholders(text)) all.add(name);
  return all;
}

/** `catalogs`: `{ en: {...}, uk: {...} }` (nested JSON). `usedKeys`: base keys found in the source, or null to skip. */
export function checkCatalogs(catalogs, usedKeys = null) {
  const problems = [];
  const source = catalogs[SOURCE_LANGUAGE];
  if (!source) return [`missing source catalog "${SOURCE_LANGUAGE}"`];
  const sourceGroups = group(flatten(source));

  for (const [language, catalog] of Object.entries(catalogs)) {
    const flat = flatten(catalog);
    for (const [key, value] of Object.entries(flat)) {
      if (typeof value !== "string" || value.trim() === "") problems.push(`${language}: "${key}" is empty or not a string`);
    }
    const groups = group(flat);
    const required = new Set(new Intl.PluralRules(language).resolvedOptions().pluralCategories);

    for (const [base, sourceEntry] of Object.entries(sourceGroups)) {
      const entry = groups[base];
      if (!entry) {
        problems.push(`${language}: missing key "${base}"`);
        continue;
      }
      if (sourceEntry.plural !== entry.plural) {
        problems.push(`${language}: "${base}" must${sourceEntry.plural ? "" : " not"} be a plural key`);
        continue;
      }
      if (entry.plural) {
        for (const category of required) {
          if (!(category in entry.variants)) problems.push(`${language}: "${base}" needs the plural form "${category}"`);
        }
        for (const category of Object.keys(entry.variants)) {
          if (!required.has(category)) problems.push(`${language}: "${base}" has the unused plural form "${category}"`);
        }
      }
      const expected = unionPlaceholders(sourceEntry);
      const actual = unionPlaceholders(entry);
      for (const name of expected) if (!actual.has(name)) problems.push(`${language}: "${base}" lacks the placeholder {{${name}}}`);
      for (const name of actual) if (!expected.has(name)) problems.push(`${language}: "${base}" has the unknown placeholder {{${name}}}`);
    }
    for (const base of Object.keys(groups)) {
      if (!(base in sourceGroups)) problems.push(`${language}: extra key "${base}" not in ${SOURCE_LANGUAGE}`);
    }
  }

  if (usedKeys) {
    for (const base of Object.keys(sourceGroups)) {
      const dynamic = DYNAMIC_PREFIXES.some((prefix) => base.startsWith(prefix));
      if (!dynamic && !usedKeys.has(base)) problems.push(`unused key "${base}"`);
    }
  }
  return problems;
}

function* sourceFiles(directory) {
  for (const name of readdirSync(directory)) {
    const path = join(directory, name);
    if (statSync(path).isDirectory()) {
      if (name !== "locales" && name !== "node_modules") yield* sourceFiles(path);
    } else if ([".ts", ".tsx"].includes(extname(name)) && !name.endsWith(".d.ts") && name !== "bindings.ts") {
      yield path;
    }
  }
}

/** Collects keys referenced as `t("a.b")`-style string literals. */
export function findUsedKeys(directory) {
  const used = new Set();
  for (const file of sourceFiles(directory)) {
    const text = readFileSync(file, "utf8");
    for (const match of text.matchAll(/["'`]([a-z][A-Za-z0-9]*(?:\.[A-Za-z0-9]+)+)["'`]/g)) used.add(match[1]);
  }
  return used;
}

function loadCatalogs(directory) {
  const catalogs = {};
  for (const name of readdirSync(directory)) {
    if (extname(name) === ".json") catalogs[basename(name, ".json")] = JSON.parse(readFileSync(join(directory, name), "utf8"));
  }
  return catalogs;
}

if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  const localesDir = resolve(process.argv[2] ?? "src/shared/i18n/locales");
  const sourceDir = resolve(process.argv[3] ?? "src");
  const catalogs = loadCatalogs(localesDir);
  const problems = checkCatalogs(catalogs, findUsedKeys(sourceDir));
  if (problems.length > 0) {
    console.error(`i18n check failed (${problems.length}):\n${problems.map((p) => `  - ${p}`).join("\n")}`);
    process.exit(1);
  }
  console.log(`i18n check passed: ${Object.keys(catalogs).join(", ")}`);
}
