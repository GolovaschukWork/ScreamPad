import { cleanup } from "@testing-library/react";
import i18next from "i18next";
import { afterEach, beforeEach } from "vitest";
import { initI18n } from "../shared/i18n";
import { installMatchMedia } from "./matchMedia";

// jsdom has no layout engine, so a few browser APIs the components rely on are stood in for, and nothing more.
class ResizeObserverStub {
  observe() {}
  unobserve() {}
  disconnect() {}
}
globalThis.ResizeObserver = ResizeObserverStub;
Element.prototype.scrollIntoView = () => {};

initI18n(["en"]);

beforeEach(async () => {
  await i18next.changeLanguage("en");
  installMatchMedia();
  document.documentElement.className = "";
  document.documentElement.lang = "en";
});

afterEach(() => {
  cleanup();
});
