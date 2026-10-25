import assert from "node:assert/strict";
import { test } from "node:test";
import { checkCatalogs } from "./check-catalogs.mjs";
import { resolveLanguage } from "../../src/shared/i18n/resolveLanguage.ts";

const en = {
  a: { title: "Title", count_one: "{{count}} item", count_other: "{{count}} items" },
  errors: { boom: "Boom {{code}}" },
};
const uk = {
  a: { title: "Заголовок", count_one: "{{count}} елемент", count_few: "{{count}} елементи", count_many: "{{count}} елементів", count_other: "{{count}} елемента" },
  errors: { boom: "Бум {{code}}" },
};

test("complete catalogs pass", () => {
  assert.deepEqual(checkCatalogs({ en, uk }), []);
});

test("a missing key is reported", () => {
  const broken = structuredClone(uk);
  delete broken.a.title;
  assert.match(checkCatalogs({ en, uk: broken }).join("\n"), /uk: missing key "a.title"/);
});

test("a missing Ukrainian plural form is reported", () => {
  const broken = structuredClone(uk);
  delete broken.a.count_few;
  assert.match(checkCatalogs({ en, uk: broken }).join("\n"), /needs the plural form "few"/);
});

test("a plural form the language does not use is reported", () => {
  const broken = structuredClone(en);
  broken.a.count_few = "{{count}} items";
  assert.match(checkCatalogs({ en: broken, uk }).join("\n"), /unused plural form "few"/);
});

test("a changed placeholder is reported", () => {
  const broken = structuredClone(uk);
  broken.errors.boom = "Бум {{kod}}";
  const text = checkCatalogs({ en, uk: broken }).join("\n");
  assert.match(text, /lacks the placeholder {{code}}/);
  assert.match(text, /unknown placeholder {{kod}}/);
});

test("an empty value and an extra key are reported", () => {
  const broken = structuredClone(uk);
  broken.a.title = "  ";
  broken.a.extra = "Зайве";
  const text = checkCatalogs({ en, uk: broken }).join("\n");
  assert.match(text, /"a.title" is empty/);
  assert.match(text, /extra key "a.extra"/);
});

test("an unused key is reported, dynamic error keys are not", () => {
  const problems = checkCatalogs({ en, uk }, new Set(["a.title"]));
  assert.deepEqual(problems.sort(), ['unused key "a.count"']);
});

test("language resolution", () => {
  assert.equal(resolveLanguage(["uk-UA", "en-US"]), "uk");
  assert.equal(resolveLanguage(["en-GB"]), "en");
  assert.equal(resolveLanguage(["de-DE", "uk"]), "uk");
  assert.equal(resolveLanguage(["UK_ua"]), "uk");
  assert.equal(resolveLanguage(["de-DE", "fr"]), "en");
  assert.equal(resolveLanguage([]), "en");
  assert.equal(resolveLanguage(["", "  "]), "en");
});
